//! AST → DLVM 字节码编译器
//!
//! 把 Dalin L 的 AST 编译为 `BytecodeFunction` 序列，
//! 供 DLVM 执行引擎运行。当前支持表达式求值 + 控制流。
//! 完整函数/闭包/模块支持在 Phase 2 迭代。

use dalin_compiler::ast::{Expr, Program, Stmt};

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
        }
    }

    /// 编译整个程序
    pub fn compile(&mut self, prog: &Program) -> Vec<BytecodeFunction> {
        self.functions.clear();
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
        }

        self.finish_function();
        std::mem::take(&mut self.functions)
    }

    fn start_function(&mut self, name: &str) {
        self.fn_name = name.to_string();
        self.code.clear();
        self.constants.clear();
    }

    fn finish_function(&mut self) {
        self.functions.push(BytecodeFunction {
            name: std::mem::take(&mut self.fn_name),
            code: std::mem::take(&mut self.code),
            constants: std::mem::take(&mut self.constants),
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        });
    }

    fn emit(&mut self, op: Opcode) {
        self.code.push(op);
    }

    fn add_constant(&mut self, s: &str) -> u16 {
        let idx = self.constants.len();
        self.constants.push(s.to_string());
        idx as u16
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(e) => {
                self.compile_expr(e);
            }
            Stmt::Let { name: _, value, .. } => {
                // let x = expr → 编译 expr，结果留在栈上
                if let Some(v) = value {
                    self.compile_expr(v);
                } else {
                    self.emit(Opcode::LoadNone);
                }
                // 目前简单模型：表达式的值留在栈上
                // 完整变量绑定在 Phase 2 实现（locals 表）
            }
            Stmt::Fn { .. } => {
                // 函数定义暂不支持在字节码中作为表达式
                // Phase 2: 编译为可调用的 BytecodeFunction
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
            Expr::Ident(_) => {
                // 标识符查找 — 暂未实现变量表
                // Phase 2: 查 locals 表
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
                // 如果函数是 Ident，用 Builtin
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
                        self.emit(Opcode::Call(args.len() as u16, CallTarget::Name(name.clone())));
                    }
                } else {
                    // 匿名函数/表达式调用，暂用索引 0
                    self.emit(Opcode::Call(args.len() as u16, CallTarget::Index(0)));
                }
            }

            Expr::IfExpr(cond, then, else_) => {
                // 编译条件
                self.compile_expr(cond);
                // 假跳转（占位，稍后 patch）
                let jmp_false_idx = self.code.len();
                self.emit(Opcode::JmpIfNot(0)); // placeholder

                // then 分支
                self.compile_expr(then);
                let jmp_end_idx = self.code.len();
                self.emit(Opcode::Jmp(0)); // placeholder

                // else 分支
                let false_offset = self.code.len() as i16 - jmp_false_idx as i16 - 1;
                // 因为 JmpIfNot 跳的是 false 分支，偏移是相对于 JmpIfNot 之后的指令
                // 但由于我们是追加方式，修正偏移需要更精密的计算
                // Phase 2: 精确 patch

                self.compile_expr(else_);
                let end_offset = self.code.len() as i16 - jmp_end_idx as i16 - 1;

                // Patch 跳转偏移（粗略）
                if jmp_false_idx < self.code.len()
                    && let Opcode::JmpIfNot(offset) = &mut self.code[jmp_false_idx] { *offset = false_offset }
                if jmp_end_idx < self.code.len()
                    && let Opcode::Jmp(offset) = &mut self.code[jmp_end_idx] { *offset = end_offset }
            }

            Expr::Array(items) => {
                for item in items {
                    self.compile_expr(item);
                }
                self.emit(Opcode::MakeArray(items.len() as u16));
            }

            Expr::Pipe { input, ops } => {
                // pipe: input |> fn(arg)  →  compile arg, call fn
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
                        None => self.emit(Opcode::Call(2, CallTarget::Name(fn_name.clone()))), // input + arg
                    }
                }
            }

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
}
