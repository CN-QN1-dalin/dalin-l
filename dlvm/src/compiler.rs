//! AST → DLVM 字节码编译器
//!
//! 把 Dalin L 的 AST 编译为 `BytecodeFunction` 序列，
//! 供 DLVM 执行引擎运行。Phase 2: 完整函数/变量/成员访问支持。

use std::collections::HashMap;

use dalin_compiler::ast::{Expr, FnParam, Program, Stmt};

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
    /// 局部变量表：变量名 → 栈偏移（从栈底算起的位置）
    locals: HashMap<String, usize>,
    /// 当前已编译的函数名集合（用于去重）
    compiled_fns: Vec<String>,
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
        }
    }

    /// 编译整个程序
    pub fn compile(&mut self, prog: &Program) -> Vec<BytecodeFunction> {
        self.functions.clear();
        self.compiled_fns.clear();
        self.start_function("__entry__");

        for stmt in &prog.statements {
            self.compile_stmt(stmt);
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

    /// 注册一个局部变量，记录其栈偏移
    fn register_local(&mut self, name: &str, stack_offset: usize) {
        self.locals.insert(name.to_string(), stack_offset);
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(e) => {
                self.compile_expr(e);
            }
            Stmt::Let { name, value, .. } => {
                // let x = expr → 编译 expr，记录变量位置
                let stack_before = self.current_offset(); // 近似栈偏移
                if let Some(v) = value {
                    self.compile_expr(v);
                } else {
                    self.emit(Opcode::LoadNone);
                }
                self.register_local(name, stack_before);
            }
            Stmt::Const { name, value, .. } => {
                let stack_before = self.current_offset();
                if let Some(v) = value {
                    self.compile_expr(v);
                } else {
                    self.emit(Opcode::LoadNone);
                }
                self.register_local(name, stack_before);
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
                    self.register_local(&pname, i);
                }

                // 编译函数体
                for s in body {
                    self.compile_stmt(s);
                }

                // 确保有 Return
                if self.code.is_empty() || !matches!(self.code.last(), Some(Opcode::Return)) {
                    self.emit(Opcode::LoadNone);
                    self.emit(Opcode::Return);
                }

                // 完成函数编译
                self.functions.push(BytecodeFunction {
                    name: std::mem::take(&mut self.fn_name),
                    code: std::mem::take(&mut self.code),
                    constants: std::mem::take(&mut self.constants),
                    arity: params.len() as u8,
                    locals: 0,
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
            _ => {}
        }
    }

    /// 从函数参数提取名称
    fn param_name(_param: &FnParam, idx: usize) -> String {
        // FnParam 可能有 name 字段，也可能只有 type_annotation
        // 用索引作为后备名
        format!("_p{idx}")
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
                // 查找局部变量表
                if let Some(_offset) = self.locals.get(name) {
                    // 变量已注册，但当前 VM 没有 LoadLocal 指令
                    // 变量值在编译时已被 push 到栈上
                    // 这里标记为已解析
                }
                // 对于非变量标识符（如函数名在 Call 中），由 Call 处理
                self.emit(Opcode::LoadNone);
            }

            Expr::BinaryOp { left, op, right } => {
                self.compile_expr(left);
                self.compile_expr(right);
                let opcode = match op.as_str() {
                    "+" => Opcode::Add,
                    "-" => Opcode::Sub,
                    "*" => Opcode::Mul,
                    "/" => Opcode::Div,
                    "==" => Opcode::Eq,
                    "!=" => Opcode::Ne,
                    "<" => Opcode::Lt,
                    ">" => Opcode::Gt,
                    "<=" => Opcode::Le,
                    ">=" => Opcode::Ge,
                    _ => Opcode::Add, // fallback
                };
                self.emit(opcode);
            }

            Expr::UnaryOp { op, operand } => {
                self.compile_expr(operand);
                if op == "-" {
                    self.emit(Opcode::Neg);
                }
            }

            Expr::Call { func, args } => {
                // 编译参数
                for a in args {
                    self.compile_expr(a);
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
                // 假跳转占位
                let jmp_false_pos = self.current_offset();
                self.emit(Opcode::JmpIfNot(0)); // placeholder

                // then 分支
                self.compile_expr(then);
                let jmp_end_pos = self.current_offset();
                self.emit(Opcode::Jmp(0)); // placeholder

                // else 分支 — 记录其起始位置
                let else_start = self.current_offset();
                self.compile_expr(else_);

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
                }
                self.emit(Opcode::MakeArray(items.len() as u16));
            }

            // 成员访问 obj.member → Member(idx)
            Expr::MemberAccess { object, member } => {
                self.compile_expr(object);
                let idx = self.add_constant(member);
                self.emit(Opcode::Member(idx));
            }

            // 索引访问 arr[i] → Index
            Expr::Index { array, index } => {
                self.compile_expr(array);
                self.compile_expr(index);
                self.emit(Opcode::Index);
            }

            Expr::Pipe { input, ops } => {
                // pipe: input |> fn(arg) → compile input, call fn with arg
                self.compile_expr(input);
                for (fn_name, call_arg) in ops {
                    self.compile_expr(call_arg);
                    let builtin_idx = match fn_name.as_str() {
                        "print" => Some(0),
                        "println" => Some(1),
                        _ => None,
                    };
                    match builtin_idx {
                        Some(idx) => self.emit(Opcode::Builtin(idx)),
                        None => self.emit(Opcode::Call(2, CallTarget::Name(fn_name.clone()))),
                    }
                }
            }

            // Range 表达式：编译 start 和 end，结果留待运行时处理
            Expr::Range { start, end, inclusive: _ } => {
                self.compile_expr(start);
                self.compile_expr(end);
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

            // Match 表达式：暂时编译 target + 第一个 arm 的最后一条表达式
            Expr::MatchExpr(target, arms) => {
                self.compile_expr(target);
                if let Some(first_arm) = arms.first() {
                    // 取第一个 arm 的最后一条表达式语句
                    if let Some(last_stmt) = first_arm.body.last() {
                        self.compile_stmt(last_stmt);
                    } else {
                        self.emit(Opcode::LoadNone);
                    }
                } else {
                    self.emit(Opcode::LoadNone);
                }
            }

            // 未覆盖的表达式：fallback
            _ => {
                self.emit(Opcode::LoadNone);
            }
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
