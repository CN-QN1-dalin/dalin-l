//! Dalin L 字节码虚拟机（DLVM）
//!
//! 替换树遍历解释器，采用栈式字节码架构。
//! 核心组件：
//! - [Opcode]: 指令集（27 条指令）
//! - [BytecodeFunction]: 编译后的函数表示
//! - [Vm]: 栈式执行引擎
//! - [compiler]: AST → 字节码编译器

mod compiler;
pub use compiler::BytecodeCompiler;

/// 寄存器索引（虚拟寄存器，编译时分配）
pub type Reg = u8;

/// 字节码指令
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opcode {
    // ── 常量加载 ──
    LoadInt(i64),          // 加载整数常量
    LoadFloat(f64),        // 加载浮点常量
    LoadStr(u16),          // 加载字符串常量（常量池索引）
    LoadBool(bool),        // 加载布尔常量
    LoadNone,              // 加载 None

    // ── 算术运算 ──
    Add,                   // 栈顶两个值相加
    Sub,
    Mul,
    Div,
    Neg,                   // 一元负号

    // ── 比较运算 ──
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,

    // ── 控制流 ──
    Jmp(i16),              // 无条件跳转（相对偏移）
    JmpIf(i16),            // 条件跳转（栈顶为 true 时跳转）
    JmpIfNot(i16),         // 条件跳转（栈顶为 false 时跳转）
    Halt,                  // 停止执行

    // ── 函数与调用 ──
    Call(u16),             // 调用函数（参数个数已在栈上）
    Return,                // 从函数返回
    MakeClosure(u16),      // 创建闭包（函数索引，环境大小）

    // ── 数据结构 ──
    MakeArray(u16),        // 创建数组（从栈上弹出 n 个元素）
    Index,                 // 索引访问
    Member(u16),           // 成员访问（字符串常量池索引）

    // ── 内置函数 ──
    Builtin(u8),           // 调用内置函数（索引 0=print,1=len,2=push,3=assert...）

    // ── Agent 原语 ──
    Spawn(u16),            // spawn 任务（函数索引，参数个数）
    Send,                  // 发送到通道
    Recv,                  // 从通道接收
}

/// 编译后的函数
#[derive(Debug, Clone)]
pub struct BytecodeFunction {
    /// 函数名
    pub name: String,
    /// 字节码指令序列
    pub code: Vec<Opcode>,
    /// 常量池（字符串）
    pub constants: Vec<String>,
    /// 参数个数
    pub arity: u8,
    /// 局部变量数量
    pub locals: u8,
    /// 三通道标注（用于控制面调度）
    pub effect: Option<String>,
    pub capability: Option<String>,
}

/// DLVM 运行时值
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    None,
    Array(Vec<Value>),
    /// 闭包：(函数索引, 捕获的环境值)
    Closure(u16, Vec<Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => (a - b).abs() < f64::EPSILON,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::None, Value::None) => true,
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::Str(a), Value::Str(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(fl) => write!(f, "{fl}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::None => write!(f, "none"),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Closure(idx, _) => write!(f, "<closure fn#{idx}>"),
        }
    }
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        if let Value::Int(n) = self { Some(*n) } else { None }
    }
    pub fn as_float(&self) -> Option<f64> {
        if let Value::Float(n) = self { Some(*n) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
}

/// DLVM 执行引擎
pub struct Vm {
    /// 值栈
    stack: Vec<Value>,
    /// 调用栈（返回地址 + 栈基址）
    call_stack: Vec<(usize, usize)>,
    /// 已加载的函数表
    functions: Vec<BytecodeFunction>,
    /// 当前执行指令指针
    ip: usize,
    /// 当前函数索引
    current_fn: usize,
    /// 是否正在执行
    running: bool,
}

/// DLVM 错误
#[derive(Debug)]
pub enum VmError {
    StackUnderflow,
    InvalidOpcode(Opcode),
    FunctionNotFound(u16),
    TypeError(String),
    DivisionByZero,
    Halt,
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::StackUnderflow => write!(f, "stack underflow"),
            VmError::InvalidOpcode(op) => write!(f, "invalid opcode: {op:?}"),
            VmError::FunctionNotFound(idx) => write!(f, "function #{idx} not found"),
            VmError::TypeError(msg) => write!(f, "type error: {msg}"),
            VmError::DivisionByZero => write!(f, "division by zero"),
            VmError::Halt => write!(f, "halt"),
        }
    }
}

impl Vm {
    /// 创建新的 VM 实例，加载函数表。
    pub fn new(functions: Vec<BytecodeFunction>) -> Self {
        let entry = functions.first().map(|_| 0).unwrap_or(0);
        Self {
            stack: Vec::with_capacity(1024),
            call_stack: Vec::with_capacity(64),
            functions,
            ip: 0,
            current_fn: entry,
            running: false,
        }
    }

    /// 运行虚拟机，从第一个函数开始执行。
    pub fn run(&mut self) -> Result<Value, VmError> {
        self.running = true;
        self.ip = 0;
        self.current_fn = 0;

        while self.running {
            let func = &self.functions[self.current_fn].clone();
            if self.ip >= func.code.len() {
                break;
            }
            let op = func.code[self.ip];
            self.ip += 1;
            self.execute_op(op)?;
        }

        Ok(self.stack.pop().unwrap_or(Value::None))
    }

    /// 执行单条指令
    fn execute_op(&mut self, op: Opcode) -> Result<(), VmError> {
        match op {
            Opcode::LoadInt(n) => self.stack.push(Value::Int(n)),
            Opcode::LoadFloat(f) => self.stack.push(Value::Float(f)),
            Opcode::LoadStr(idx) => {
                let func = &self.functions[self.current_fn];
                let s = func.constants.get(idx as usize).cloned().unwrap_or_default();
                self.stack.push(Value::Str(s));
            }
            Opcode::LoadBool(b) => self.stack.push(Value::Bool(b)),
            Opcode::LoadNone => self.stack.push(Value::None),

            Opcode::Add => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                    (Value::Str(x), Value::Str(y)) => Value::Str(format!("{x}{y}")),
                    _ => return Err(VmError::TypeError("+ requires int/float/str".into())),
                };
                self.stack.push(result);
            }
            Opcode::Sub => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(x - y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x - y),
                    _ => return Err(VmError::TypeError("- requires int/float".into())),
                };
                self.stack.push(result);
            }
            Opcode::Mul => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => Value::Int(x * y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x * y),
                    _ => return Err(VmError::TypeError("* requires int/float".into())),
                };
                self.stack.push(result);
            }
            Opcode::Div => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (_, Value::Int(0)) | (_, Value::Float(0.0)) => return Err(VmError::DivisionByZero),
                    (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                    (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                    _ => return Err(VmError::TypeError("/ requires int/float".into())),
                };
                self.stack.push(result);
            }
            Opcode::Neg => {
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match v {
                    Value::Int(x) => self.stack.push(Value::Int(-x)),
                    Value::Float(x) => self.stack.push(Value::Float(-x)),
                    _ => return Err(VmError::TypeError("-x requires numeric".into())),
                }
            }

            // 比较运算
            Opcode::Eq => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.stack.push(Value::Bool(a == b));
            }
            Opcode::Ne => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.stack.push(Value::Bool(a != b));
            }
            Opcode::Lt => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x < y,
                    (Value::Float(x), Value::Float(y)) => x < y,
                    (Value::Str(x), Value::Str(y)) => x < y,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
            }
            Opcode::Gt => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x > y,
                    (Value::Float(x), Value::Float(y)) => x > y,
                    (Value::Str(x), Value::Str(y)) => x > y,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
            }
            Opcode::Le => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x <= y,
                    (Value::Float(x), Value::Float(y)) => x <= y,
                    (Value::Str(x), Value::Str(y)) => x <= y,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
            }
            Opcode::Ge => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let result = match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x >= y,
                    (Value::Float(x), Value::Float(y)) => x >= y,
                    (Value::Str(x), Value::Str(y)) => x >= y,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
            }

            // 控制流
            Opcode::Jmp(offset) => {
                let new_ip = if offset >= 0 {
                    self.ip + offset as usize
                } else {
                    self.ip.saturating_sub((-offset) as usize)
                };
                self.ip = new_ip;
            }
            Opcode::JmpIf(offset) => {
                let cond = self.pop_bool()?;
                if cond {
                    let new_ip = if offset >= 0 {
                        self.ip + offset as usize
                    } else {
                        self.ip.saturating_sub((-offset) as usize)
                    };
                    self.ip = new_ip;
                }
            }
            Opcode::JmpIfNot(offset) => {
                let cond = self.pop_bool()?;
                if !cond {
                    let new_ip = if offset >= 0 {
                        self.ip + offset as usize
                    } else {
                        self.ip.saturating_sub((-offset) as usize)
                    };
                    self.ip = new_ip;
                }
            }

            Opcode::Return => {
                let result = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                // 弹到调用栈帧的基址
                if let Some((ret_ip, base)) = self.call_stack.pop() {
                    self.stack.truncate(base);
                    self.stack.push(result);
                    self.ip = ret_ip;
                    self.current_fn = self.find_fn_by_ip(ret_ip).unwrap_or(0);
                } else {
                    // 顶层返回
                    self.stack.push(result);
                    self.running = false;
                }
            }

            Opcode::Call(argc) => {
                // 保存返回地址 + 栈基址
                let base = self.stack.len().saturating_sub(argc as usize);
                self.call_stack.push((self.ip, base));
                // 目标函数索引在栈上（参数之上）
                // 目前简单实现：调用函数 0（入口）
                // TODO: 从 MakeClosure/函数名查表
            }

            Opcode::Builtin(idx) => {
                self.execute_builtin(idx)?;
            }

            // 未实现的指令 —— 直接 panic（Phase 2 迭代补齐）
            other => return Err(VmError::InvalidOpcode(other)),
        }
        Ok(())
    }

    fn _unused_binary_op<F>(&mut self, f: F) -> Result<(), VmError>
    where
        F: FnOnce(Value, Value) -> Result<Value, VmError>,
    {
        let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        let result = f(a, b)?;
        self.stack.push(result);
        Ok(())
    }

    fn _unused_compare_op<F>(&mut self, f: F) -> Result<(), VmError>
    where
        F: FnOnce(&Value, &Value) -> bool,
    {
        let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
        self.stack.push(Value::Bool(f(&a, &b)));
        Ok(())
    }

    fn pop_bool(&mut self) -> Result<bool, VmError> {
        match self.stack.pop().ok_or(VmError::StackUnderflow)? {
            Value::Bool(b) => Ok(b),
            Value::None => Ok(false),
            Value::Int(0) => Ok(false),
            Value::Int(_) => Ok(true),
            _ => Ok(true), // 任意非空值 = true
        }
    }

    fn execute_builtin(&mut self, idx: u8) -> Result<(), VmError> {
        match idx {
            0 => {
                // print: 弹出栈顶值并打印
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                print!("{v}");
                self.stack.push(Value::None);
            }
            1 => {
                // println: 弹出栈顶值并打印换行
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                println!("{v}");
                self.stack.push(Value::None);
            }
            2 => {
                // len: 弹出数组/字符串，返回长度
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match v {
                    Value::Array(arr) => self.stack.push(Value::Int(arr.len() as i64)),
                    Value::Str(s) => self.stack.push(Value::Int(s.len() as i64)),
                    _ => return Err(VmError::TypeError("len requires array/str".into())),
                }
            }
            3 => {
                // assert: 弹出条件值，false 则 panic
                let cond = self.pop_bool()?;
                if !cond {
                    return Err(VmError::TypeError("assertion failed".into()));
                }
                self.stack.push(Value::None);
            }
            _ => {
                return Err(VmError::InvalidOpcode(Opcode::Builtin(idx)));
            }
        }
        Ok(())
    }

    fn find_fn_by_ip(&self, ip: usize) -> Option<usize> {
        // 粗略查找：遍历函数表找包含此 IP 的函数
        let mut offset = 0usize;
        for (i, f) in self.functions.iter().enumerate() {
            if ip >= offset && ip < offset + f.code.len() {
                return Some(i);
            }
            offset += f.code.len();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fn(code: Vec<Opcode>, constants: Vec<String>) -> BytecodeFunction {
        BytecodeFunction {
            name: "test".into(),
            code,
            constants,
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        }
    }

    #[test]
    fn arithmetic() {
        let f = make_fn(vec![
            Opcode::LoadInt(3),
            Opcode::LoadInt(4),
            Opcode::Add,
            Opcode::Return,
        ], vec![]);
        let mut vm = Vm::new(vec![f]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn string_concat() {
        let f = make_fn(vec![
            Opcode::LoadStr(0),
            Opcode::LoadStr(1),
            Opcode::Add,
            Opcode::Return,
        ], vec!["hello ".into(), "world".into()]);
        let mut vm = Vm::new(vec![f]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Str("hello world".into()));
    }

    #[test]
    fn comparison() {
        let f = make_fn(vec![
            Opcode::LoadInt(5),
            Opcode::LoadInt(3),
            Opcode::Gt,
            Opcode::Return,
        ], vec![]);
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Bool(true));
    }

    #[test]
    fn builtin_print() {
        let f = make_fn(vec![
            Opcode::LoadStr(0),
            Opcode::Builtin(1),  // println
            Opcode::Return,
        ], vec!["hello".into()]);
        let mut vm = Vm::new(vec![f]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::None);
    }

    #[test]
    fn conditional_jump() {
        // if true { 10 } else { 20 }
        let f = make_fn(vec![
            Opcode::LoadBool(true),
            Opcode::JmpIfNot(4),    // false → 跳到 else 分支
            Opcode::LoadInt(10),
            Opcode::Jmp(2),         // 跳过 else
            Opcode::LoadInt(20),
            Opcode::Return,
        ], vec![]);
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Int(10));
    }
}
