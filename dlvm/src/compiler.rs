//! AST → DLVM 字节码编译器
//!
//! 把 Dalin L 的 AST 编译为 `BytecodeFunction` 序列，
//! 供 DLVM 执行引擎运行。Phase 2: 完整函数/变量/成员访问支持。

use std::collections::HashMap;

use dalin_compiler::ast::{Expr, FnParam, InterpolatePart, Program, Stmt};

use crate::{BytecodeFunction, CallTarget, Opcode};

/// AST → 字节码编译器
pub struct BytecodeCompiler {
    /// 已编译的函数
    pub functions: Vec<BytecodeFunction>,
    /// 当前函数的常量池
    constants: Vec<String>,
    /// 当前函数的字节码
    code: Vec<Opcode>,
    /// 当前函数名
    fn_name: String,
    /// 局部变量表：变量名 → 槽位编号（u8，对应 VM::GetLoc/SetLoc）
    locals: HashMap<String, u8>,
    /// 当前已编译的函数名集合（用于去重）
    compiled_fns: Vec<String>,
    /// 编译错误标志
    had_compile_error: bool,
}

impl Default for BytecodeCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl BytecodeCompiler {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            constants: Vec::new(),
            code: Vec::new(),
            fn_name: "main".into(),
            locals: HashMap::new(),
            compiled_fns: vec![],
            had_compile_error: false,
        }
    }

    fn mark_error(&mut self) {
        self.had_compile_error = true;
    }

    fn has_error(&self) -> bool {
        self.had_compile_error
    }

    /// 编译整个程序
    pub fn compile(&mut self, prog: &Program) -> Vec<BytecodeFunction> {
        self.functions.clear();
        self.compiled_fns.clear();
        self.had_compile_error = false;
        self.start_function("__entry__");

        for stmt in &prog.statements {
            self.compile_stmt(stmt);
            if self.has_error() {
                break;
            }
        }

        if self.has_error() {
            // 编译失败：返回空函数列表，调用方检查 had_compile_error
            return std::mem::take(&mut self.functions);
        }

        // 确保最后有 Return
        if !self.code.is_empty() {
            match self.code.last() {
                Some(Opcode::Return) => {}
                _ => self.emit(Opcode::Return),
            }
        } else {
            self.emit(Opcode::LoadNone);
            self.emit(Opcode::Return);
        }

        self.finish_function();
        std::mem::take(&mut self.functions)
    }

    fn start_function(&mut self, name: &str) {
        self.fn_name = name.to_string();
        self.code.clear();
        self.constants.clear();
        self.locals.clear();
    }

    fn finish_function(&mut self) {
        self.functions.push(BytecodeFunction {
            name: std::mem::take(&mut self.fn_name),
            code: std::mem::take(&mut self.code),
            constants: std::mem::take(&mut self.constants),
            arity: self.locals.len() as u8, // 参数计入 locals
            locals: 0,
            effect: None,
            capability: None,
        });
        self.locals.clear();
    }

    fn emit(&mut self, op: Opcode) {
        self.code.push(op);
    }

    /// 当前字节码位置（用于跳转 patch）
    fn current_offset(&self) -> usize {
        self.code.len()
    }

    fn add_constant(&mut self, s: &str) -> u16 {
        // 去重：已有相同常量则复用
        if let Some(pos) = self.constants.iter().position(|c| c == s) {
            return pos as u16;
        }
        let idx = self.constants.len();
        self.constants.push(s.to_string());
        idx as u16
    }

    /// 为新局部变量分配下一个槽位编号并注册
    fn allocate_local(&mut self, name: &str) -> Option<u8> {
        let max_slot = self.locals.values().copied().max().unwrap_or(0);
        let slot = max_slot.saturating_add(1);
        self.locals.insert(name.to_string(), slot);
        Some(slot)
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(e) => {
                self.compile_expr(e);
            }
            Stmt::Let { name, value, mutable: _, type_annotation: _ } => {
                let sid = match self.allocate_local(name) {
                    Some(s) => s,
                    None => return,
                };
                if let Some(v) = value {
                    self.compile_expr(v);
                } else {
                    self.emit(Opcode::LoadNone);
                }
                self.emit(Opcode::SetLoc(sid));
            }
            Stmt::Const { name, value, type_annotation: _ } => {
                let sid = match self.allocate_local(name) {
                    Some(s) => s,
                    None => return,
                };
                if let Some(v) = value {
                    self.compile_expr(v);
                } else {
                    self.emit(Opcode::LoadNone);
                }
                self.emit(Opcode::SetLoc(sid));
            }
            Stmt::Fn {
                name,
                params,
                return_type: _,
                effect,
                capability,
                body,
                ..
            } => {
                // 编译函数定义为独立 BytecodeFunction
                if self.compiled_fns.contains(name) {
                    return; // 避免重复编译
                }
                self.compiled_fns.push(name.clone());

                // 保存当前编译状态
                let saved_code = std::mem::take(&mut self.code);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_name = std::mem::take(&mut self.fn_name);
                let saved_locals = std::mem::take(&mut self.locals);

                // 开始编译新函数
                self.fn_name = name.clone();

                // 注册参数为局部变量
                for (i, param) in params.iter().enumerate() {
                    let pname = Self::param_name(param, i);
                    // Param slot assignment
                    let p_slot = i as u8;
                    self.locals.insert(pname.clone(), p_slot);
                    // Also ensure locals vec has enough room
                }

                // 编译函数体
                for s in body {
                    self.compile_stmt(s);
                }

                // 确保有 Return：函数返回值取自函数体最后表达式留在栈顶的值，
                // 不再无条件压入 LoadNone（否则会覆盖 match/if/裸表达式的返回值）
                if self.code.is_empty() {
                    self.emit(Opcode::LoadNone);
                    self.emit(Opcode::Return);
                } else if !matches!(self.code.last(), Some(Opcode::Return)) {
                    self.emit(Opcode::Return);
                }

                // 完成函数编译
                let local_count = self.locals.len() as u8;
                self.functions.push(BytecodeFunction {
                    name: std::mem::take(&mut self.fn_name),
                    code: std::mem::take(&mut self.code),
                    constants: std::mem::take(&mut self.constants),
                    arity: params.len() as u8,
                    locals: local_count,
                    effect: effect.clone(),
                    capability: capability.clone(),
                });

                // 恢复主函数编译状态
                self.code = saved_code;
                self.constants = saved_constants;
                self.fn_name = saved_name;
                self.locals = saved_locals;
            }
            Stmt::Return(val) => {
                if let Some(e) = val {
                    self.compile_expr(e);
                } else {
                    self.emit(Opcode::LoadNone);
                }
                self.emit(Opcode::Return);
            }
            // ── 控制流: If 语句 ──
            Stmt::If { condition, then_body, else_body } => {
                self.compile_expr(condition);
                if self.has_error() { return; }
                let jmp_false_pos = self.current_offset();
                self.emit(Opcode::JmpIfNot(0)); // placeholder
                for s in then_body {
                    self.compile_stmt(s);
                    if self.has_error() { return; }
                }
                let jmp_end_pos = self.current_offset();
                self.emit(Opcode::Jmp(0)); // placeholder
                let else_start = self.current_offset();
                for s in else_body {
                    self.compile_stmt(s);
                    if self.has_error() { return; }
                }
                let end = self.current_offset();
                // Patch JmpIfNot → jump to else_start
                if jmp_false_pos < self.code.len()
                    && let Opcode::JmpIfNot(offset) = &mut self.code[jmp_false_pos]
                {
                    *offset = (else_start as i64 - jmp_false_pos as i64 - 1) as i16;
                }
                // Patch Jmp → jump past else
                if jmp_end_pos < self.code.len()
                    && let Opcode::Jmp(offset) = &mut self.code[jmp_end_pos]
                {
                    *offset = (end as i64 - jmp_end_pos as i64 - 1) as i16;
                }
            }
            // ── 控制流: While 循环 ──
            Stmt::While { condition, body } => {
                let cond_start = self.current_offset();
                self.compile_expr(condition);
                if self.has_error() { return; }
                let jmp_end_pos = self.current_offset();
                self.emit(Opcode::JmpIfNot(0)); // placeholder
                for s in body {
                    self.compile_stmt(s);
                    if self.has_error() { return; }
                }
                let end = self.current_offset();
                // Jump back to condition
                let back_offset = (cond_start as i64 - end as i64 - 1) as i16;
                self.emit(Opcode::Jmp(back_offset));
                // Patch JmpIfNot → skip past the Jmp back
                if jmp_end_pos < self.code.len()
                    && let Opcode::JmpIfNot(offset) = &mut self.code[jmp_end_pos]
                {
                    *offset = (end as i64 - jmp_end_pos as i64) as i16;
                }
            }
            // ── 控制流: For 循环（数组迭代） ──
            Stmt::For { target, iterable, body } => {
                // 编译可迭代对象 → 栈顶
                self.compile_expr(iterable);
                if self.has_error() { return; }
                let iter_slot = match self.allocate_local(&format!("__iter_{target}")) {
                    Some(s) => s,
                    None => return,
                };
                self.emit(Opcode::SetLoc(iter_slot));
                // 获取长度
                self.emit(Opcode::GetLoc(iter_slot));
                self.emit(Opcode::Builtin(2)); // len
                let len_slot = match self.allocate_local(&format!("__len_{target}")) {
                    Some(s) => s,
                    None => return,
                };
                self.emit(Opcode::SetLoc(len_slot));
                // 初始化计数器
                self.emit(Opcode::LoadInt(0));
                let i_slot = match self.allocate_local(&format!("__i_{target}")) {
                    Some(s) => s,
                    None => return,
                };
                self.emit(Opcode::SetLoc(i_slot));
                // 循环开始
                let loop_start = self.current_offset();
                // 条件: i < len
                self.emit(Opcode::GetLoc(i_slot));
                self.emit(Opcode::GetLoc(len_slot));
                self.emit(Opcode::Lt);
                let jmp_end_pos = self.current_offset();
                self.emit(Opcode::JmpIfNot(0)); // placeholder
                // 取元素: iter[i]
                self.emit(Opcode::GetLoc(iter_slot));
                self.emit(Opcode::GetLoc(i_slot));
                self.emit(Opcode::Index);
                let target_slot = match self.allocate_local(target) {
                    Some(s) => s,
                    None => return,
                };
                self.emit(Opcode::SetLoc(target_slot));
                // 循环体
                for s in body {
                    self.compile_stmt(s);
                    if self.has_error() { return; }
                }
                // i += 1
                self.emit(Opcode::GetLoc(i_slot));
                self.emit(Opcode::LoadInt(1));
                self.emit(Opcode::Add);
                self.emit(Opcode::SetLoc(i_slot));
                // Jump back to condition
                let end = self.current_offset();
                let back_offset = (loop_start as i64 - end as i64 - 1) as i16;
                self.emit(Opcode::Jmp(back_offset));
                // Patch JmpIfNot → skip past the Jmp back
                if jmp_end_pos < self.code.len()
                    && let Opcode::JmpIfNot(offset) = &mut self.code[jmp_end_pos]
                {
                    *offset = (end as i64 - jmp_end_pos as i64) as i16;
                }
            }
            // ── 控制流: Match 语句（多臂编译） ──
            Stmt::Match { target, arms } => {
                // 编译目标表达式
                self.compile_expr(target);
                if self.has_error() { return; }
                // 将目标值存入临时局部变量
                let match_slot = match self.allocate_local("__match_val") {
                    Some(s) => s,
                    None => { self.mark_error(); return; }
                };
                self.emit(Opcode::SetLoc(match_slot));
                // 跳转表：每个 arm 的 fallthrough 落点顺序进入下一 arm（或结尾）
                let mut end_jumps: Vec<usize> = Vec::new();
                let arm_count = arms.len();
                for (i, arm) in arms.iter().enumerate() {
                    let is_last = i == arm_count - 1;
                    // 根据模式类型生成匹配检查
                    let mut pat_fail_pos: Option<usize> = None;
                    match arm.pattern.kind.as_str() {
                        "wild" | "ident" => {
                            // 通配符/标识符模式：始终匹配，无需检查
                        }
                        "lit" => {
                            // 字面量模式：加载目标值 → 编译字面量 → 比较
                            self.emit(Opcode::GetLoc(match_slot));
                            if let Some(ref lit_expr) = arm.pattern.value {
                                self.compile_expr(lit_expr);
                                if self.has_error() { return; }
                            } else {
                                self.emit(Opcode::LoadNone);
                            }
                            self.emit(Opcode::Eq);
                            // 不匹配 → 跳到本 arm 的 pat_fail 落点（Pop 比较结果后顺序进入下一 arm）
                            let pos = self.current_offset();
                            self.emit(Opcode::JmpIfNot(0));
                            pat_fail_pos = Some(pos);
                        }
                        _ => {
                            // 未知模式类型：回退到始终匹配
                        }
                    }
                    // 注：Eq 的比较结果已由 JmpIfNot 消费（无论是否跳转都 pop_bool），匹配成功后栈已平衡，无需额外 Pop
                    // 变量绑定模式：纯 ident 被解析为 ctor 且无 inner，直接将模式变量别名到
                    // match_slot，使 body/guard 能引用目标值（无需额外指令，无栈副作用）
                    if arm.pattern.kind == "ctor"
                        && arm.pattern.inner.is_empty()
                        && !arm.pattern.name.is_empty()
                    {
                        self.locals.insert(arm.pattern.name.clone(), match_slot);
                    }
                    // guard 检查：有 guard 且为 false 时跳过本 arm
                    let mut guard_fail_pos: Option<usize> = None;
                    if let Some(ref guard) = arm.guard {
                        self.compile_expr(guard);
                        if self.has_error() { return; }
                        let pos = self.current_offset();
                        self.emit(Opcode::JmpIfNot(0));
                        guard_fail_pos = Some(pos);
                    }
                    // 编译 arm body
                    for s in &arm.body {
                        self.compile_stmt(s);
                        if self.has_error() { return; }
                    }
                    if !is_last {
                        // 执行完当前 arm 后跳转到结尾
                        let jmp_end_pos = self.current_offset();
                        self.emit(Opcode::Jmp(0));
                        end_jumps.push(jmp_end_pos);
                    }
                    // 本 arm 的 fallthrough 落点：顺序进入下一 arm
                    // VM 语义: 执行 JmpIfNot 时 self.ip 已 = pos+1, 故 new_ip = pos+1+offset
                    // 目标为 code.len()(下一 arm 起点) => offset = code.len() - pos - 1
                    if let Some(pos) = pat_fail_pos {
                        let off = (self.code.len() as i64 - pos as i64 - 1) as i16;
                        if let Opcode::JmpIfNot(ref mut o) = self.code[pos] {
                            *o = off;
                        }
                    }
                    if let Some(pos) = guard_fail_pos {
                        let off = (self.code.len() as i64 - pos as i64 - 1) as i16;
                        if let Opcode::JmpIfNot(ref mut o) = self.code[pos] {
                            *o = off;
                        }
                        // guard false 时栈已干净（JmpIfNot 已 pop bool），顺序进入下一 arm
                    }
                }
                // 回填所有 Jmp 跳转到结尾
                // Jmp 在 jmp_pos, 执行时 self.ip = jmp_pos+1 => offset = end - jmp_pos - 1
                let end = self.current_offset();
                for jmp_pos in &end_jumps {
                    if let Opcode::Jmp(ref mut off) = self.code[*jmp_pos] {
                        *off = (end as i64 - *jmp_pos as i64 - 1) as i16;
                    }
                }
            }
            // ── Assert 语句 → builtin 3 ──
            Stmt::Assert { condition, message: _ } => {
                self.compile_expr(condition);
                if self.has_error() { return; }
                self.emit(Opcode::Builtin(3)); // assert
            }
            // ── Spawn → 编译内部函数 + Spawn stub ──
            Stmt::Spawn { fn_decl } => {
                self.compile_stmt(fn_decl);
                if self.has_error() { return; }
                self.emit(Opcode::Spawn(0)); // stub: spawn fn 0
            }
            // ── TryCatch → 编译 try_body + catch_body ──
            Stmt::TryCatch { try_body, catch_param, catch_body } => {
                // 编译 try body
                for s in try_body {
                    self.compile_stmt(s);
                    if self.has_error() { return; }
                }
                // try 成功后跳转到结尾（跳过 catch）
                let jmp_end_pos = self.current_offset();
                self.emit(Opcode::Jmp(0));
                // 编译 catch body（捕获异常时执行）
                // 如果 catch_param 存在，分配局部变量并存入错误值（占位）
                if let Some(param_name) = catch_param
                    && let Some(slot) = self.allocate_local(param_name)
                {
                    self.emit(Opcode::LoadNone); // 错误值占位
                    self.emit(Opcode::SetLoc(slot));
                }
                for s in catch_body {
                    self.compile_stmt(s);
                    if self.has_error() { return; }
                }
                // 回填 Jmp 跳转到结尾
                let end = self.current_offset();
                if let Opcode::Jmp(ref mut off) = self.code[jmp_end_pos] {
                    *off = (end as i64 - jmp_end_pos as i64) as i16;
                }
            }
            // ── 声明类语句：由 load_stmt 处理，字节码中跳过 ──
            Stmt::StructDef { .. } | Stmt::EnumDef { .. } | Stmt::TraitDef { .. }
            | Stmt::ImplBlock { .. } | Stmt::TypeAlias { .. } | Stmt::Channel { .. }
            | Stmt::Llm { .. } | Stmt::Use(_) | Stmt::Export(_) => {}
            // ── ExternBlock: 元数据，无字节码 ──
            Stmt::ExternBlock { .. } => {}
        }
    }

    /// 从函数参数提取名称
    fn param_name(param: &FnParam, _idx: usize) -> String {
        param.name.clone()
    }

    fn compile_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::IntLiteral(n) => self.emit(Opcode::LoadInt(*n)),
            Expr::FloatLiteral(f) => self.emit(Opcode::LoadFloat(*f)),
            Expr::StringLiteral(s) => {
                let idx = self.add_constant(s);
                self.emit(Opcode::LoadStr(idx));
            }
            Expr::BoolLiteral(b) => self.emit(Opcode::LoadBool(*b)),
            Expr::CharLiteral(c) => {
                let s = c.to_string();
                let idx = self.add_constant(&s);
                self.emit(Opcode::LoadStr(idx));
            }
            Expr::Ident(name) => {
                if let Some(&slot) = self.locals.get(name.as_str()) {
                    self.emit(Opcode::GetLoc(slot));
                } else {
                    // 未注册的标识符 → LoadNone
                    self.emit(Opcode::LoadNone);
                }
            }

            Expr::BinaryOp { left, op, right } => {
                self.compile_expr(left);
                if self.has_error() { return; }
                self.compile_expr(right);
                if self.has_error() { return; }
                let opcode = match op.as_str() {
                    "+" => Opcode::Add,
                    "-" => Opcode::Sub,
                    "*" => Opcode::Mul,
                    "/" => Opcode::Div,
                    "%" => Opcode::Mod,
                    "&&" => Opcode::And,
                    "||" => Opcode::Or,
                    "==" => Opcode::Eq,
                    "!=" => Opcode::Ne,
                    "<" => Opcode::Lt,
                    ">" => Opcode::Gt,
                    "<=" => Opcode::Le,
                    ">=" => Opcode::Ge,
                    other => {
                        eprintln!("compile error: unknown binary operator '{}'", other);
                        self.mark_error();
                        return;
                    }
                };
                self.emit(opcode);
            }

            Expr::UnaryOp { op, operand } => {
                self.compile_expr(operand);
                if self.has_error() { return; }
                if op == "-" {
                    self.emit(Opcode::Neg);
                } else if op == "!" {
                    self.emit(Opcode::Not);
                } else {
                    eprintln!("compile error: unknown unary operator '{}'", op);
                    self.mark_error();
                }
            }

            Expr::Call { func, args } => {
                // 编译参数
                for a in args {
                    self.compile_expr(a);
                    if self.has_error() { return; }
                }
                // 如果函数是 Ident，用 Builtin 或按名调用
                if let Expr::Ident(name) = func.as_ref() {
                    let builtin_idx = match name.as_str() {
                        "print" => Some(0),
                        "println" => Some(1),
                        "len" => Some(2),
                        "assert" => Some(3),
                        _ => None,
                    };
                    if let Some(idx) = builtin_idx {
                        self.emit(Opcode::Builtin(idx));
                    } else {
                        // 用户定义的函数调用：按名称查找
                        self.emit(Opcode::Call(
                            args.len() as u16,
                            CallTarget::Name(name.clone()),
                        ));
                    }
                } else {
                    // 表达式调用，暂用索引 0
                    self.emit(Opcode::Call(args.len() as u16, CallTarget::Index(0)));
                }
            }

            Expr::IfExpr(cond, then, else_) => {
                // 编译条件
                self.compile_expr(cond);
                if self.has_error() { return; }
                // 假跳转占位
                let jmp_false_pos = self.current_offset();
                self.emit(Opcode::JmpIfNot(0)); // placeholder

                // then 分支
                self.compile_expr(then);
                if self.has_error() { return; }
                let jmp_end_pos = self.current_offset();
                self.emit(Opcode::Jmp(0)); // placeholder

                // else 分支 — 记录其起始位置
                let else_start = self.current_offset();
                self.compile_expr(else_);
                if self.has_error() { return; }

                // 精确 patch 跳转偏移
                // JmpIfNot: 条件为 false 时跳到 else_start
                let false_offset = (else_start as i64 - jmp_false_pos as i64 - 1) as i16;
                if jmp_false_pos < self.code.len()
                    && let Opcode::JmpIfNot(offset) = &mut self.code[jmp_false_pos]
                {
                    *offset = false_offset;
                }

                // Jmp: then 结束后跳过 else
                let end = self.current_offset();
                let end_offset = (end as i64 - jmp_end_pos as i64 - 1) as i16;
                if jmp_end_pos < self.code.len()
                    && let Opcode::Jmp(offset) = &mut self.code[jmp_end_pos]
                {
                    *offset = end_offset;
                }
            }

            Expr::Array(items) => {
                for item in items {
                    self.compile_expr(item);
                    if self.has_error() { return; }
                }
                self.emit(Opcode::MakeArray(items.len() as u16));
            }

            // 成员访问 obj.member → Member(idx)
            Expr::MemberAccess { object, member } => {
                self.compile_expr(object);
                if self.has_error() { return; }
                let idx = self.add_constant(member);
                self.emit(Opcode::Member(idx));
            }

            // 索引访问 arr[i] → Index
            Expr::Index { array, index } => {
                self.compile_expr(array);
                if self.has_error() { return; }
                self.compile_expr(index);
                if self.has_error() { return; }
                self.emit(Opcode::Index);
            }

            Expr::Pipe { input, ops } => {
                // pipe: input |> fn(arg) → compile input, call fn with arg
                self.compile_expr(input);
                if self.has_error() { return; }
                for (fn_name, call_arg) in ops {
                    self.compile_expr(call_arg);
                    if self.has_error() { return; }
                    let builtin_idx = match fn_name.as_str() {
                        "print" => Some(0),
                        "println" => Some(1),
                        _ => None,
                    };
                    match builtin_idx {
                        Some(idx) => self.emit(Opcode::Builtin(idx)),
                        None => {
                            // argv = 2 (input + call_arg) for non-builtin pipe call
                            self.emit(Opcode::Call(
                                2u16,
                                CallTarget::Name(fn_name.clone()),
                            ));
                        }
                    }
                }
            }

            // Range 表达式：编译 start 和 end，结果留待运行时处理
            Expr::Range { start, end, inclusive: _ } => {
                self.compile_expr(start);
                if self.has_error() { return; }
                self.compile_expr(end);
                if self.has_error() { return; }
                // 简化为二元数组 [start, end]
                self.emit(Opcode::MakeArray(2));
            }

            // OptionValue: Some(x) / None
            Expr::OptionValue { is_some: true, value: Some(v) } => {
                self.compile_expr(v);
            }
            Expr::OptionValue { .. } => {
                self.emit(Opcode::LoadNone);
            }

            // ResultValue: Ok(x) / Err(e)
            Expr::ResultValue { value: Some(v), .. } => {
                self.compile_expr(v);
            }
            Expr::ResultValue { .. } => {
                self.emit(Opcode::LoadNone);
            }

            // Match 表达式：多臂编译，每个 arm 返回最后一条表达式的值
            Expr::MatchExpr(target, arms) => {
                // 编译目标表达式
                self.compile_expr(target);
                if self.has_error() { return; }
                // 存入临时局部变量
                let match_slot = match self.allocate_local("__match_val") {
                    Some(s) => s,
                    None => { self.mark_error(); return; }
                };
                self.emit(Opcode::SetLoc(match_slot));
                // 跳转表：每个 arm 的 fallthrough 落点顺序进入下一 arm（或结尾）
                let mut end_jumps: Vec<usize> = Vec::new();
                let arm_count = arms.len();
                for (i, arm) in arms.iter().enumerate() {
                    let is_last = i == arm_count - 1;
                    let mut pat_fail_pos: Option<usize> = None;
                    match arm.pattern.kind.as_str() {
                        "wild" | "ident" => {
                            // 始终匹配
                        }
                        "lit" => {
                            self.emit(Opcode::GetLoc(match_slot));
                            if let Some(ref lit_expr) = arm.pattern.value {
                                self.compile_expr(lit_expr);
                                if self.has_error() { return; }
                            } else {
                                self.emit(Opcode::LoadNone);
                            }
                            self.emit(Opcode::Eq);
                            let pos = self.current_offset();
                            self.emit(Opcode::JmpIfNot(0));
                            pat_fail_pos = Some(pos);
                        }
                        _ => {}
                    }
                    // 注：Eq 的比较结果已由 JmpIfNot 消费（无论是否跳转都 pop_bool），
                    // 匹配成功后栈已平衡，无需额外 Pop
                    // 变量绑定模式：纯 ident 被解析为 ctor 且无 inner，直接将模式变量别名到
                    // match_slot，使 body/guard 能引用目标值（无需额外指令，无栈副作用）
                    if arm.pattern.kind == "ctor"
                        && arm.pattern.inner.is_empty()
                        && !arm.pattern.name.is_empty()
                    {
                        self.locals.insert(arm.pattern.name.clone(), match_slot);
                    }
                    // guard 检查：有 guard 且为 false 时跳过本 arm
                    let mut guard_fail_pos: Option<usize> = None;
                    if let Some(ref guard) = arm.guard {
                        self.compile_expr(guard);
                        if self.has_error() { return; }
                        let pos = self.current_offset();
                        self.emit(Opcode::JmpIfNot(0));
                        guard_fail_pos = Some(pos);
                    }
                    // 编译 arm body 所有语句（保证副作用），取最后一条的值作为表达式结果
                    for (j, s) in arm.body.iter().enumerate() {
                        self.compile_stmt(s);
                        // 中间语句不需要保留返回值（VM 会丢弃），只有最后一个 arm body 的返回值需要保留
                        if j < arm.body.len() - 1 {
                            self.emit(Opcode::Pop);
                        }
                    }
                    if arm.body.is_empty() {
                        self.emit(Opcode::LoadNone);
                    }
                    if self.has_error() { return; }
                    if !is_last {
                        let jmp_end_pos = self.current_offset();
                        self.emit(Opcode::Jmp(0));
                        end_jumps.push(jmp_end_pos);
                    }
                    // 本 arm 的 fallthrough 落点：顺序进入下一 arm
                    // VM 语义: 执行 JmpIfNot 时 self.ip 已 = pos+1, 故 new_ip = pos+1+offset
                    // 目标为 code.len()(下一 arm 起点) => offset = code.len() - pos - 1
                    if let Some(pos) = pat_fail_pos {
                        let off = (self.code.len() as i64 - pos as i64 - 1) as i16;
                        if let Opcode::JmpIfNot(ref mut o) = self.code[pos] {
                            *o = off;
                        }
                    }
                    if let Some(pos) = guard_fail_pos {
                        let off = (self.code.len() as i64 - pos as i64 - 1) as i16;
                        if let Opcode::JmpIfNot(ref mut o) = self.code[pos] {
                            *o = off;
                        }
                    }
                }
                // 回填所有 Jmp 跳转到结尾
                // Jmp 在 jmp_pos, 执行时 self.ip = jmp_pos+1 => offset = end - jmp_pos - 1
                let end = self.current_offset();
                for jmp_pos in &end_jumps {
                    if let Opcode::Jmp(ref mut off) = self.code[*jmp_pos] {
                        *off = (end as i64 - *jmp_pos as i64 - 1) as i16;
                    }
                }
            }

            // ── 显式覆盖所有剩余 AST 节点 ──
            Expr::CCall { lib_path, func_name, args } => {
                let argc = args.len();
                for a in args {
                    self.compile_expr(a);
                    if self.has_error() { return; }
                }
                let lib_idx = lib_path.as_deref().map(|p| self.add_constant(p)).unwrap_or(0);
                let func_idx = self.add_constant(func_name);
                self.emit(Opcode::CallC(lib_idx, func_idx, argc));
            }
            Expr::Interpolate { parts } => {
                // 编译各部分后用 string_concat (builtin 4) 拼接
                // 栈布局: [part0, part1, ..., partN-1, count(N)]
                for part in parts {
                    match part {
                        InterpolatePart::Literal(s) => {
                            let idx = self.add_constant(s);
                            self.emit(Opcode::LoadStr(idx));
                        }
                        InterpolatePart::Expr(e) => {
                            self.compile_expr(e);
                            if self.has_error() { return; }
                        }
                    }
                }
                let total = parts.len();
                self.emit(Opcode::LoadInt(total as i64));
                self.emit(Opcode::Builtin(4)); // string_concat
            }
            Expr::IsCheck(expr, _) => {
                self.compile_expr(expr);
                if self.has_error() { return; }
                self.emit(Opcode::LoadBool(false));
            }
            Expr::Cast(expr, _) => {
                self.compile_expr(expr);
            }
            Expr::NamedArg(_, value) => {
                self.compile_expr(value);
            }

            // Fallback for any remaining AST nodes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dalin_compiler::ast::Program;

    fn compile_expr(expr: Expr) -> Vec<Opcode> {
        let mut prog = Program::new();
        prog.add(Stmt::Expr(Box::new(expr)));
        let mut compiler = BytecodeCompiler::new();
        let funcs = compiler.compile(&prog);
        if funcs.is_empty() {
            return vec![];
        }
        funcs[0].code.clone()
    }

    #[test]
    fn compile_int_literal() {
        let code = compile_expr(Expr::IntLiteral(42));
        assert_eq!(code.len(), 2); // LoadInt + Return
        assert_eq!(code[0], Opcode::LoadInt(42));
    }

    #[test]
    fn compile_addition() {
        let code = compile_expr(Expr::BinaryOp {
            left: Box::new(Expr::IntLiteral(3)),
            op: "+".into(),
            right: Box::new(Expr::IntLiteral(4)),
        });
        assert_eq!(code[0], Opcode::LoadInt(3));
        assert_eq!(code[1], Opcode::LoadInt(4));
        assert_eq!(code[2], Opcode::Add);
    }

    #[test]
    fn compile_string() {
        let code = compile_expr(Expr::StringLiteral("hello".into()));
        assert_eq!(code.len(), 2); // LoadStr + Return
        assert!(matches!(code[0], Opcode::LoadStr(0)));
    }

    #[test]
    fn compile_if_expr() {
        let expr = Expr::IfExpr(
            Box::new(Expr::BoolLiteral(true)),
            Box::new(Expr::IntLiteral(10)),
            Box::new(Expr::IntLiteral(20)),
        );
        let code = compile_expr(expr);
        assert!(code.len() >= 4);
        assert_eq!(code[0], Opcode::LoadBool(true));
    }

    #[test]
    fn compile_member_access() {
        let expr = Expr::MemberAccess {
            object: Box::new(Expr::Ident("obj".into())),
            member: "name".into(),
        };
        let code = compile_expr(expr);
        assert!(code.len() >= 2);
        // First should be LoadNone (for Ident), then Member(const_idx) where const is "name"
        assert_eq!(code[0], Opcode::LoadNone);
        assert!(matches!(code[1], Opcode::Member(0)));
    }

    #[test]
    fn compile_index_access() {
        let expr = Expr::Index {
            array: Box::new(Expr::Array(vec![Expr::IntLiteral(1)])),
            index: Box::new(Expr::IntLiteral(0)),
        };
        let code = compile_expr(expr);
        // MakeArray(1) + LoadInt(0) + Index + Return
        assert!(code.iter().any(|op| matches!(op, Opcode::Index)));
    }

    #[test]
    fn compile_array_creation() {
        let expr = Expr::Array(vec![
            Expr::IntLiteral(1),
            Expr::IntLiteral(2),
            Expr::IntLiteral(3),
        ]);
        let code = compile_expr(expr);
        assert!(code.iter().any(|op| matches!(op, Opcode::MakeArray(3))));
    }

    #[test]
    fn compile_fn_definition() {
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "add".into(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: Some("pure".into()),
            capability: Some("cpu".into()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![
                Stmt::Return(Some(Box::new(Expr::IntLiteral(42)))),
            ],
            async_: false,
            pub_: false,
        });
        let mut compiler = BytecodeCompiler::new();
        let funcs = compiler.compile(&prog);
        // Should have 2 functions: "add" first, then "__entry__"
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[1].name, "__entry__");
        assert_eq!(funcs[0].effect.as_deref(), Some("pure"));
        assert_eq!(funcs[0].capability.as_deref(), Some("cpu"));
        // "add" body should contain LoadInt(42) + Return
        assert!(funcs[0].code.iter().any(|op| matches!(op, Opcode::LoadInt(42))));
        assert!(funcs[0].code.iter().any(|op| matches!(op, Opcode::Return)));
    }

    #[test]
    fn compile_let_binding() {
        let mut prog = Program::new();
        prog.add(Stmt::Let {
            name: "x".into(),
            value: Some(Box::new(Expr::IntLiteral(100))),
            type_annotation: None,
            mutable: false,
        });
        let mut compiler = BytecodeCompiler::new();
        let funcs = compiler.compile(&prog);
        assert!(!funcs.is_empty());
        assert!(funcs[0].code.iter().any(|op| matches!(op, Opcode::LoadInt(100))));
    }

    #[test]
    fn compile_constant_dedup() {
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::StringLiteral("dup".into())),
            op: "+".into(),
            right: Box::new(Expr::StringLiteral("dup".into())),
        };
        let code = compile_expr(expr);
        // Both string literals should use same constant index
        let indices: Vec<u16> = code
            .iter()
            .filter_map(|op| {
                if let Opcode::LoadStr(idx) = op {
                    Some(*idx)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(indices.len(), 2);
        assert_eq!(indices[0], indices[1]); // dedup: same constant reused
    }

    #[test]
    fn compile_range_expression() {
        let expr = Expr::Range {
            start: Box::new(Expr::IntLiteral(0)),
            end: Box::new(Expr::IntLiteral(10)),
            inclusive: true,
        };
        let code = compile_expr(expr);
        assert!(code.iter().any(|op| matches!(op, Opcode::MakeArray(2))));
    }

    #[test]
    fn compile_option_some() {
        let expr = Expr::OptionValue {
            is_some: true,
            value: Some(Box::new(Expr::IntLiteral(42))),
        };
        let code = compile_expr(expr);
        assert!(code.iter().any(|op| matches!(op, Opcode::LoadInt(42))));
    }
}
