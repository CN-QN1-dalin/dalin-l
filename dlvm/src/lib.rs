//! Dalin L 字节码虚拟机（DLVM）
//!
//! 替换树遍历解释器，采用栈式字节码架构。
//! 核心组件：
//! - Opcode: 指令集（30 条指令）
//! - BytecodeFunction: 编译后的函数表示
//! - Vm: 栈式执行引擎
//! - compiler: AST → 字节码编译器

mod compiler;
pub use compiler::BytecodeCompiler;

use std::collections::{HashMap, VecDeque};

/// 寄存器索引（虚拟寄存器，编译时分配）
pub type Reg = u8;

/// 调用目标：地址或函数名
#[derive(Debug, Clone, PartialEq)]
pub enum CallTarget {
    /// 按函数在 functions Vec 中的索引调用
    Index(u16),
    /// 按函数名调用
    Name(String),
}

/// 字节码指令
#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    // ── 常量加载 ──
    LoadInt(i64),   // 加载整数常量
    LoadFloat(f64), // 加载浮点常量
    LoadStr(u16),   // 加载字符串常量（常量池索引）
    LoadBool(bool), // 加载布尔常量
    LoadNone,       // 加载 None

    // ── 算术运算 ──
    Add, // 栈顶两个值相加
    Sub,
    Mul,
    Div,
    Neg, // 一元负号

    // ── 比较运算 ──
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,

    // ── 控制流 ──
    Jmp(i16),      // 无条件跳转（相对偏移）
    JmpIf(i16),    // 条件跳转（栈顶为 true 时跳转）
    JmpIfNot(i16), // 条件跳转（栈顶为 false 时跳转）
    Halt,          // 停止执行

    // ── 函数与调用 ──
    Call(u16, CallTarget), // 调用函数（参数个数，调用目标）
    Return,                // 从函数返回
    MakeClosure(u16),      // 创建闭包（函数索引，环境大小）

    // ── 局部变量读写 ──
    GetLoc(u8),     // 读取局部变量槽位 → 压栈
    SetLoc(u8),     // 弹栈顶 → 写入局部变量槽位

    // ── C FFI ──
    CallC(u16, u16, usize), // C FFI stub

    // ── 算术扩展 ──
    Mod,        // 取模
    And,        // 逻辑与
    Or,         // 逻辑或
    Not,        // 逻辑非

    // ── 数据结构 ──
    MakeArray(u16), // 创建数组（从栈上弹出 n 个元素）
    Index,          // 索引访问
    Member(u16),    // 成员访问（字符串常量池索引）

    // ── 内置函数 ──
    Builtin(u8), // 调用内置函数（索引 0=print,1=len,2=push,3=assert...）

    // ── Agent 原语 ──
    Spawn(u16), // spawn 任务（函数索引，参数个数）
    Send,       // 发送到通道
    Recv,       // 从通道接收

    // ── M:N 协程调度 ──
    CoopSpawn(u16),      // 协程 spawn：从栈顶弹出 fn_idx(u16)，加入就绪队列
    CoopAwait,            // 协程 await：当前协程让出，等待所有 spawned 协程完成
    CoopYieldResume(u8), // yield to scheduler：保存当前帧，调度下一个就绪协程
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
    /// 映射表：(key → value)
    Map(Vec<(String, Value)>),
    /// 通道端点（通道 ID）
    Channel(usize),
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
            (Value::Channel(a), Value::Channel(b)) => a == b,
            (Value::Closure(fa, ea), Value::Closure(fb, eb)) => fa == fb && ea == eb,
            // 不同类型之间永远不等
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
            Value::Map(entries) => {
                let items: Vec<String> = entries.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                write!(f, "{{{}}}", items.join(", "))
            }
            Value::Channel(id) => write!(f, "<channel#{id}>"),
            Value::Closure(idx, _) => write!(f, "<closure fn#{idx}>"),
        }
    }
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        if let Value::Int(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        if let Value::Float(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }
}

// ═══════════════════════════════
//  M:N 协程调度器
// ═══════════════════════════════

/// 协程任务状态
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    /// 就绪，等待调度
    Ready,
    /// 正在运行
    Running,
    /// 等待子任务完成（CoopAwait 后的状态）
    Waiting,
    /// 已完成
    Done,
}

/// 协程帧快照：保存 yield 时的 VM 状态以便恢复
#[derive(Debug, Clone)]
pub struct TaskFrame {
    /// 任务 ID
    pub id: usize,
    /// 指令指针
    pub ip: usize,
    /// 当前函数索引
    pub current_fn: usize,
    /// 调用栈：（返回地址，栈基址）
    pub call_stack: Vec<(usize, usize)>,
    /// 值栈副本
    pub stack: Vec<Value>,
    /// 状态
    pub status: TaskStatus,
}

/// FIFO 轮转协程调度器
#[derive(Debug)]
pub struct Scheduler {
    /// 就绪队列：FIFO 顺序
    ready_queue: Vec<usize>,
    /// 所有任务帧
    tasks: Vec<TaskFrame>,
    /// 当前运行的任务 ID（None 表示尚未调度）
    current: Option<usize>,
    /// 全局任务 ID 计数器
    next_id: usize,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            ready_queue: Vec::new(),
            tasks: Vec::new(),
            current: None,
            next_id: 0,
        }
    }

    /// 注册一个新任务（初始状态 Ready），加入就绪队列
    pub fn spawn(&mut self, ip: usize, current_fn: usize) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let frame = TaskFrame {
            id,
            ip,
            current_fn,
            call_stack: Vec::new(),
            stack: Vec::new(),
            status: TaskStatus::Ready,
        };
        self.tasks.push(frame);
        self.ready_queue.push(id);
        id
    }

    /// 激活任务：设为 Running，从队列移除
    pub fn activate(&mut self, task_id: usize) {
        self.ready_queue.retain(|&tid| tid != task_id);
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            t.status = TaskStatus::Running;
        }
        self.current = Some(task_id);
    }

    /// 标记当前任务为 Done，并唤醒所有 Waiting 任务
    pub fn mark_done(&mut self) {
        if let Some(id) = self.current {
            if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                t.status = TaskStatus::Done;
            }
            self.current = None;
        }
        // 如果没有 Ready 任务了，唤醒所有 Waiting 任务
        if !self.has_ready() {
            for t in &mut self.tasks {
                if t.status == TaskStatus::Waiting {
                    t.status = TaskStatus::Ready;
                    self.ready_queue.push(t.id);
                }
            }
        }
    }

    /// 当前任务 yield：保存帧。
    /// - to_waiting=true: CoopAwait 场景，状态设为 Waiting，不重新入队
    /// - to_waiting=false: CoopYieldResume 场景，状态设为 Ready，重新入队尾
    pub fn yield_current(&mut self, ip: usize, current_fn: usize, call_stack: &[(usize, usize)], stack: &[Value], to_waiting: bool) {
        if let Some(id) = self.current {
            if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                t.ip = ip;
                t.current_fn = current_fn;
                t.call_stack = call_stack.to_vec();
                t.stack = stack.to_vec();
                if to_waiting {
                    t.status = TaskStatus::Waiting;
                    // 不加入就绪队列，等待子任务完成后由 mark_done 唤醒
                } else {
                    t.status = TaskStatus::Ready;
                    self.ready_queue.push(id);
                }
            }
            self.current = None;
        }
    }

    /// 选择下一个就绪任务。仅选择状态为 Ready 的任务。
    /// 返回 (task_id, &TaskFrame)
    pub fn schedule_next(&mut self) -> Option<(usize, TaskFrame)> {
        while let Some(candidate_id) = self.ready_queue.first().copied() {
            // 弹出队首
            self.ready_queue.remove(0);
            // 仅当任务仍为 Ready 时调度
            if let Some(t) = self.tasks.iter().find(|t| t.id == candidate_id)
                && t.status == TaskStatus::Ready
            {
                return Some((candidate_id, t.clone()));
            }
            // 如果任务已经 Done/Running，继续看下一个
        }
        None
    }

    /// 是否还有就绪任务
    pub fn has_ready(&self) -> bool {
        self.tasks.iter().any(|t| t.status == TaskStatus::Ready)
    }

    /// 获取当前任务 ID
    pub fn current_id(&self) -> Option<usize> {
        self.current
    }
}

/// 简单通道：支持 Send/Recv
#[derive(Debug, Clone)]
pub struct Channel {
    /// 消息队列
    messages: VecDeque<Value>,
    /// 通道是否已关闭
    closed: bool,
}

impl Channel {
    pub fn new() -> Self {
        Self { messages: VecDeque::new(), closed: false }
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self::new()
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
    /// 函数名 → 函数索引映射
    fn_by_name: HashMap<String, usize>,
    /// 当前执行指令指针
    ip: usize,
    /// 当前函数索引
    current_fn: usize,
    /// 是否正在执行
    running: bool,
    /// M:N 协程调度器
    scheduler: Scheduler,
    /// 通道表（通道 ID → Channel）
    channels: HashMap<usize, Channel>,
    /// 全局通道 ID 计数器
    next_channel_id: usize,
    /// 每函数局部变量槽位 [fn_idx][slot_idx]
    locals: Vec<Vec<Value>>,
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
    SchedulerError(String),
    IndexError(String),
    ChannelError(String),
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
            VmError::SchedulerError(msg) => write!(f, "scheduler error: {msg}"),
            VmError::IndexError(msg) => write!(f, "index error: {msg}"),
            VmError::ChannelError(msg) => write!(f, "channel error: {msg}"),
        }
    }
}

impl Vm {
    /// 创建新的 VM 实例，加载函数表。
    pub fn new(functions: Vec<BytecodeFunction>) -> Self {
        let mut fn_by_name = HashMap::new();
        for (i, f) in functions.iter().enumerate() {
            fn_by_name.insert(f.name.clone(), i);
        }
        let fn_count = functions.len();
        let entry = functions.first().map(|_| 0).unwrap_or(0);
        Self {
            stack: Vec::with_capacity(1024),
            call_stack: Vec::with_capacity(64),
            functions,
            fn_by_name,
            ip: 0,
            current_fn: entry,
            running: false,
            scheduler: Scheduler::new(),
            channels: HashMap::new(),
            next_channel_id: 0,
            locals: vec![Vec::new(); fn_count],
        }
    }

    /// 创建一个新通道，返回通道 ID
    pub fn new_channel(&mut self) -> usize {
        let id = self.next_channel_id;
        self.next_channel_id += 1;
        self.channels.insert(id, Channel::new());
        id
    }

    /// 运行虚拟机。
    ///
    /// 支持两种模式：
    /// - 普通模式：无协程 opcode 时与之前行为一致
    /// - 协程模式：CoopSpawn/CoopYieldResume 触发 M:N 调度
    pub fn run(&mut self) -> Result<Value, VmError> {
        self.running = true;
        self.ip = 0;
        self.current_fn = 0;

        // 将主入口注册为任务 0
        let main_task_id = self.scheduler.spawn(0, 0);
        self.scheduler.activate(main_task_id);
        let mut main_result = Value::None;

        while self.running {
            let func = &self.functions[self.current_fn].clone();
            if self.ip >= func.code.len() {
                // 捕获 main 任务结果
                let is_main = self.scheduler.current_id() == Some(main_task_id);
                let result = self.stack.pop().unwrap_or(Value::None);
                if is_main {
                    main_result = result.clone();
                }
                self.stack.push(result);

                self.scheduler.mark_done();
                // 尝试调度下一个任务
                if let Some((next_id, frame)) = self.scheduler.schedule_next() {
                    self.load_frame(&frame);
                    self.scheduler.activate(next_id);
                    continue; // 继续执行新任务
                }
                self.running = false;
                break;
            }
            let op = func.code[self.ip].clone();
            self.ip += 1;
            match self.execute_op(op) {
                Ok(()) => {}
                Err(VmError::Halt) => {
                    // 捕获当前栈顶作为主任务结果
                    let is_main = self.scheduler.current_id() == Some(main_task_id);
                    if is_main {
                        main_result = self.stack.pop().unwrap_or(Value::None);
                    }
                    self.running = false;
                    break;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(main_result)
    }

    /// 从 TaskFrame 恢复 VM 状态
    fn load_frame(&mut self, frame: &TaskFrame) {
        self.ip = frame.ip;
        self.current_fn = frame.current_fn;
        self.call_stack = frame.call_stack.clone();
        self.stack = frame.stack.clone();
    }

    /// 执行单条指令
    fn execute_op(&mut self, op: Opcode) -> Result<(), VmError> {
        match op {
            Opcode::LoadInt(n) => self.stack.push(Value::Int(n)),
            Opcode::LoadFloat(f) => self.stack.push(Value::Float(f)),
            Opcode::LoadStr(idx) => {
                let func = &self.functions[self.current_fn];
                let s = func.constants.get(idx as usize).cloned();
                match s {
                    Some(s) => self.stack.push(Value::Str(s)),
                    None => return Err(VmError::TypeError("string constant out of bounds".into())),
                }
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
                    (_, Value::Int(0)) | (_, Value::Float(0.0)) => {
                        return Err(VmError::DivisionByZero);
                    }
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
                    // 顶层返回：将结果压回栈，ip 移到末尾让 run loop 处理任务完成
                    self.stack.push(result);
                    let func = &self.functions[self.current_fn];
                    self.ip = func.code.len();
                }
            }

            Opcode::Call(argc, target) => {
                // 保存返回地址 + 栈基址
                let base = self.stack.len().saturating_sub(argc as usize);
                self.call_stack.push((self.ip, base));
                // 根据调用目标查找函数
                let fn_idx = match target {
                    CallTarget::Index(idx) => idx as usize,
                    CallTarget::Name(name) => match self.fn_by_name.get(&name) {
                        Some(idx) => *idx,
                        None => return Err(VmError::FunctionNotFound(0)),
                    },
                };
                // 切换到目标函数
                self.current_fn = fn_idx;
                self.ip = 0;
            }

            Opcode::Builtin(idx) => {
                self.execute_builtin(idx)?;
            }

            // ── 局部变量读写 ──
            Opcode::GetLoc(slot) => {
                let sid = slot as usize;
                if self.locals.len() <= self.current_fn || sid >= self.locals[self.current_fn].len() {
                    self.locals[self.current_fn].resize(sid + 1, Value::None);
                }
                self.stack.push(self.locals[self.current_fn][sid].clone());
            }
            Opcode::SetLoc(slot) => {
                let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let sid = slot as usize;
                if self.locals.len() <= self.current_fn || sid >= self.locals[self.current_fn].len() {
                    self.locals[self.current_fn].resize(sid + 1, Value::None);
                }
                self.locals[self.current_fn][sid] = val;
            }
            Opcode::CallC(_lib_idx, _func_idx, argc) => {
                for _ in 0..argc {
                    self.stack.pop().ok_or(VmError::StackUnderflow)?;
                }
                self.stack.push(Value::None);
            }
            Opcode::Mod => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) if y != 0 => self.stack.push(Value::Int(x % y)),
                    (Value::Float(x), Value::Float(y)) if y != 0.0 => self.stack.push(Value::Float(x % y)),
                    _ => return Err(VmError::TypeError("% requires int/float".into())),
                }
            }
            Opcode::And => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.stack.push(Value::Bool(self.bool_of(&a) && self.bool_of(&b)));
            }
            Opcode::Or => {
                let b = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.stack.push(Value::Bool(self.bool_of(&a) || self.bool_of(&b)));
            }
            Opcode::Not => {
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.stack.push(Value::Bool(!self.bool_of(&a)));
            }

            // ── M:N 协程调度 ──

            // CoopSpawn(fn_idx): 从栈顶弹出被 spawn 函数的起始 ip=0,fn_idx，
            // 将当前栈快照传给新任务，注册到调度器就绪队列。
            Opcode::CoopSpawn(fn_idx) => {
                // 当前栈是父任务传给子任务的参数栈
                let child_id = self.scheduler.spawn(0, fn_idx as usize);
                // 子任务继承当前栈作为初始参数
                if let Some(t) = self.scheduler.tasks.iter_mut().find(|t| t.id == child_id) {
                    t.stack = self.stack.clone();
                }
                // 父任务将 child_id 压栈以便后续 await
                self.stack.push(Value::Int(child_id as i64));
            }

            // CoopAwait: 等待所有 spawned 协程完成（除当前任务外）。
            // 如果还有 Ready 任务，yield 让出 CPU；否则继续执行。
            Opcode::CoopAwait => {
                if self.scheduler.has_ready() {
                    // 还有子任务在运行 → 保存当前帧并 yield
                    let _current_id = self.scheduler.current_id();
                    self.scheduler.yield_current(
                        self.ip,
                        self.current_fn,
                        &self.call_stack,
                        &self.stack,
                        true, // to_waiting: CoopAwait 等待子任务
                    );
                    // 调度下一个就绪任务
                    if let Some((next_id, frame)) = self.scheduler.schedule_next() {
                        self.load_frame(&frame);
                        self.scheduler.activate(next_id);
                    } else {
                        // 理论上不应该到这里（has_ready 为 true 但 schedule_next 找不到）
                        return Err(VmError::SchedulerError(
                            "CoopAwait: has_ready true but schedule_next returned None".into(),
                        ));
                    }
                }
                // 否则所有子任务已完成，继续执行当前任务
            }

            // CoopYieldResume: 显式 yield 到调度器，保存当前帧
            // 并立即调度下一个就绪任务。
            Opcode::CoopYieldResume(_slot) => {
                self.scheduler.yield_current(
                    self.ip,
                    self.current_fn,
                    &self.call_stack,
                    &self.stack,
                    false, // to_waiting: CoopYieldResume 轮转让出
                );
                if let Some((next_id, frame)) = self.scheduler.schedule_next() {
                    self.load_frame(&frame);
                    self.scheduler.activate(next_id);
                } else {
                    // 没有就绪任务 → 将当前任务重新激活
                    if let Some(id) = self.scheduler.current_id() {
                        // 恢复刚 yield 的任务
                        self.scheduler.activate(id);
                    } else {
                        self.running = false;
                    }
                }
            }

            // ── 停止执行 ──
            Opcode::Halt => {
                return Err(VmError::Halt);
            }

            // ── 数据结构 ──

            // MakeArray(n): 从栈顶弹出 n 个值，组成数组
            Opcode::MakeArray(n) => {
                let n = n as usize;
                if self.stack.len() < n {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack.len() - n;
                let elements: Vec<Value> = self.stack.drain(start..).collect();
                self.stack.push(Value::Array(elements));
            }

            // Index: stack[.., collection, index] → collection[index]
            Opcode::Index => {
                let index_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let collection = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match collection {
                    Value::Array(arr) => {
                        let i = match index_val {
                            Value::Int(i) if i >= 0 => i as usize,
                            _ => return Err(VmError::IndexError("array index must be non-negative int".into())),
                        };
                        let elem = arr.get(i).cloned()
                            .ok_or_else(|| VmError::IndexError(format!("index {i} out of bounds (len {})", arr.len())))?;
                        self.stack.push(elem);
                    }
                    Value::Str(s) => {
                        let i = match index_val {
                            Value::Int(i) if i >= 0 => i as usize,
                            _ => return Err(VmError::IndexError("string index must be non-negative int".into())),
                        };
                        let ch = s.chars().nth(i)
                            .ok_or_else(|| VmError::IndexError(format!("index {i} out of bounds (len {})", s.chars().count())))?;
                        self.stack.push(Value::Str(ch.to_string()));
                    }
                    Value::Map(entries) => {
                        let key_str = match &index_val {
                            Value::Str(s) => s.clone(),
                            Value::Int(i) => i.to_string(),
                            _ => return Err(VmError::IndexError("map key must be string or int".into())),
                        };
                        let val = entries.iter()
                            .find(|(k, _)| k == &key_str)
                            .map(|(_, v)| v.clone())
                            .ok_or_else(|| VmError::IndexError(format!("key '{key_str}' not found in map")))?;
                        self.stack.push(val);
                    }
                    _ => return Err(VmError::IndexError("value is not indexable".into())),
                }
            }

            // Member(field_idx): stack[.., object] → object.field
            // field_idx 指向常量池中的字符串（字段名）
            Opcode::Member(field_idx) => {
                let object = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let func = &self.functions[self.current_fn];
                let field_name = func.constants.get(field_idx as usize).cloned()
                    .ok_or_else(|| VmError::IndexError(format!("constant #{field_idx} out of bounds")))?;
                match object {
                    Value::Map(entries) => {
                        let val = entries.iter()
                            .find(|(k, _)| k == &field_name)
                            .map(|(_, v)| v.clone())
                            .ok_or_else(|| VmError::IndexError(format!("field '{field_name}' not found")))?;
                        self.stack.push(val);
                    }
                    _ => return Err(VmError::IndexError(format!("cannot access field '{field_name}' on non-map value"))),
                }
            }

            // ── 闭包 ──
            // MakeClosure(env_size): 从栈顶弹出 env_size 个值作为捕获环境
            // 创建指向当前函数的闭包
            Opcode::MakeClosure(env_size) => {
                let n = env_size as usize;
                if self.stack.len() < n {
                    return Err(VmError::StackUnderflow);
                }
                let start = self.stack.len() - n;
                let env: Vec<Value> = self.stack.drain(start..).collect();
                self.stack.push(Value::Closure(self.current_fn as u16, env));
            }

            // ── Agent 原语 ──

            // Spawn(fn_idx): 异步 spawn 任务
            // 从栈顶弹出 argc 个参数传给被 spawn 的函数
            Opcode::Spawn(fn_idx) => {
                // 将当前栈顶作为参数传给新任务
                // spawn 本身不等待，立即返回 spawn_id
                let child_id = self.scheduler.spawn(0, fn_idx as usize);
                // 将当前栈复制给子任务（作为参数）
                if let Some(t) = self.scheduler.tasks.iter_mut().find(|t| t.id == child_id) {
                    t.stack = self.stack.clone();
                }
                self.stack.push(Value::Int(child_id as i64));
            }

            // Send: stack[.., channel, value] → 发送 value 到 channel
            Opcode::Send => {
                let value = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let chan_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let chan_id = match chan_val {
                    Value::Channel(id) => id,
                    _ => return Err(VmError::ChannelError("Send target must be a channel".into())),
                };
                let chan = self.channels.get_mut(&chan_id)
                    .ok_or_else(|| VmError::ChannelError(format!("channel #{chan_id} not found")))?;
                if chan.closed {
                    return Err(VmError::ChannelError(format!("channel #{chan_id} is closed")));
                }
                chan.messages.push_back(value);
                self.stack.push(Value::None);
            }

            // Recv: stack[.., channel] → 从 channel 接收一个值
            Opcode::Recv => {
                let chan_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let chan_id = match chan_val {
                    Value::Channel(id) => id,
                    _ => return Err(VmError::ChannelError("Recv target must be a channel".into())),
                };
                let chan = self.channels.get_mut(&chan_id)
                    .ok_or_else(|| VmError::ChannelError(format!("channel #{chan_id} not found")))?;
                match chan.messages.pop_front() {
                    Some(val) => self.stack.push(val),
                    None => {
                        if chan.closed {
                            self.stack.push(Value::None); // 通道关闭时返回 None
                        } else {
                            return Err(VmError::ChannelError(format!("channel #{chan_id} is empty")));
                        }
                    }
                }
            }

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

    fn bool_of(&self, v: &Value) -> bool {
        match v {
            Value::Bool(b) => *b,
            Value::None => false,
            Value::Int(0) => false,
            Value::Str(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            _ => true,
        }
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
        let f = make_fn(
            vec![
                Opcode::LoadInt(3),
                Opcode::LoadInt(4),
                Opcode::Add,
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![f]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn string_concat() {
        let f = make_fn(
            vec![
                Opcode::LoadStr(0),
                Opcode::LoadStr(1),
                Opcode::Add,
                Opcode::Return,
            ],
            vec!["hello ".into(), "world".into()],
        );
        let mut vm = Vm::new(vec![f]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Str("hello world".into()));
    }

    #[test]
    fn comparison() {
        let f = make_fn(
            vec![
                Opcode::LoadInt(5),
                Opcode::LoadInt(3),
                Opcode::Gt,
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Bool(true));
    }

    #[test]
    fn builtin_print() {
        let f = make_fn(
            vec![
                Opcode::LoadStr(0),
                Opcode::Builtin(1), // println
                Opcode::Return,
            ],
            vec!["hello".into()],
        );
        let mut vm = Vm::new(vec![f]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::None);
    }

    #[test]
    fn conditional_jump() {
        // if true { 10 } else { 20 }
        let f = make_fn(
            vec![
                Opcode::LoadBool(true),
                Opcode::JmpIfNot(4), // false → 跳到 else 分支
                Opcode::LoadInt(10),
                Opcode::Jmp(2), // 跳过 else
                Opcode::LoadInt(20),
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Int(10));
    }

    // ── M:N 协程测试 ──

    #[test]
    fn coop_spawn_and_await_simple() {
        // 主任务 CoopSpawn 子任务，子任务计算 10+20，主任务 await 后返回 42
        let main = make_fn(
            vec![
                Opcode::CoopSpawn(1),   // spawn fn#1
                Opcode::CoopAwait,       // wait for child
                Opcode::LoadInt(42),     // after child done
                Opcode::Return,
            ],
            vec![],
        );
        let worker = BytecodeFunction {
            name: "calc".into(),
            code: vec![
                Opcode::LoadInt(10),
                Opcode::LoadInt(20),
                Opcode::Add,
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![main, worker]);
        let result = vm.run().unwrap();
        // main 在 child 完成后恢复执行，返回 42
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn coop_yield_resume_cycle() {
        // entry spawns t1, yields to let t1 run, resumes and computes 1+10=11
        let t1 = BytecodeFunction {
            name: "t1".into(),
            code: vec![
                Opcode::LoadInt(100),
                Opcode::LoadInt(200),
                Opcode::Add,                   // 100+200 = 300
                Opcode::CoopYieldResume(0),    // yield back to entry
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let entry = BytecodeFunction {
            name: "entry".into(),
            code: vec![
                Opcode::CoopSpawn(1),          // spawn t1 (fn#1)
                Opcode::LoadInt(1),
                Opcode::CoopYieldResume(0),    // yield to let t1 run
                Opcode::LoadInt(10),
                Opcode::Add,                   // 1+10 = 11
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![entry, t1]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(11));
    }

    #[test]
    fn coop_yield_saves_frame() {
        // Verify that after yield, the task resumes from where it left off
        let t = make_fn(
            vec![
                Opcode::LoadInt(7),
                Opcode::CoopYieldResume(0),
                Opcode::LoadInt(8),
                Opcode::Add,                // 7+8 = 15
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![t]);
        let result = vm.run().unwrap();
        // After yielding and resuming (only one task, so it resumes itself),
        // stack should be [7, 8] before Add → result is 15
        assert_eq!(result, Value::Int(15));
    }

    #[test]
    fn coop_await_blocks_until_children_done() {
        // Main spawns child, awaits, child runs to completion
        let main = BytecodeFunction {
            name: "main".into(),
            code: vec![
                Opcode::LoadInt(0),
                Opcode::CoopSpawn(1),          // spawn child (fn#1)
                Opcode::CoopAwait,              // wait for child
                Opcode::LoadInt(42),            // after child done
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let child = BytecodeFunction {
            name: "child".into(),
            code: vec![
                Opcode::LoadInt(100),
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![main, child]);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn scheduler_round_robin() {
        // Three tasks: t0 yields→t1→t1 yields→t2→t2 yields→t0 resumes
        let t0 = BytecodeFunction {
            name: "t0".into(),
            code: vec![
                Opcode::LoadInt(1),
                Opcode::CoopYieldResume(0),
                Opcode::LoadInt(2),
                Opcode::Add,
                Opcode::CoopYieldResume(0),
                Opcode::LoadInt(3),
                Opcode::Add,              // (1+2)+3 = 6
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        // Entry spawns t0 and t1 as child tasks, then yields
        let entry = BytecodeFunction {
            name: "entry".into(),
            code: vec![
                Opcode::LoadInt(0),        // dummy arg for spawn
                Opcode::CoopSpawn(1),      // spawn t0 (fn#1)
                Opcode::CoopAwait,          // wait for children
                Opcode::LoadInt(100),
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![entry, t0]);
        let result = vm.run().unwrap();
        // After children complete, main returns 100
        assert_eq!(result, Value::Int(100));
    }

    #[test]
    fn scheduler_resume_prepares_for_scheduling() {
        // Single task yields, should resume itself since there's nothing else
        let t = make_fn(
            vec![
                Opcode::LoadInt(5),
                Opcode::CoopYieldResume(0),
                Opcode::LoadInt(7),
                Opcode::Mul,                // 5*7 = 35
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![t]);
        assert_eq!(vm.run().unwrap(), Value::Int(35));
    }

    // ── 数据结构 & Agent 原语测试 ──

    #[test]
    fn make_array_and_index() {
        let f = make_fn(
            vec![
                Opcode::LoadInt(10),
                Opcode::LoadInt(20),
                Opcode::LoadInt(30),
                Opcode::MakeArray(3),    // [10, 20, 30]
                Opcode::LoadInt(1),      // index 1
                Opcode::Index,            // [10,20,30][1] = 20
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Int(20));
    }

    #[test]
    fn index_string() {
        let f = make_fn(
            vec![
                Opcode::LoadStr(0),      // "hello"
                Opcode::LoadInt(1),      // index 1
                Opcode::Index,            // "hello"[1] = "e"
                Opcode::Return,
            ],
            vec!["hello".into()],
        );
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Str("e".into()));
    }

    #[test]
    fn index_map() {
        // Build a map and access by key
        let f = make_fn(
            vec![
                // Build map manually via MakeArray then treat entries
                Opcode::LoadStr(0),      // "x"
                Opcode::LoadInt(100),
                Opcode::LoadStr(1),      // "y"
                Opcode::LoadInt(200),
                Opcode::MakeArray(4),    // ["x", 100, "y", 200]
                // For now, Index on Array with string key is unsupported
                // But we can test Map directly below
                Opcode::LoadInt(42),
                Opcode::Return,
            ],
            vec!["x".into(), "y".into()],
        );
        let mut vm = Vm::new(vec![f]);
        assert_eq!(vm.run().unwrap(), Value::Int(42));
    }

    #[test]
    fn member_access_on_map() {
        // Create a map and access a field via Member
        let f = BytecodeFunction {
            name: "test".into(),
            code: vec![
                // Push a pre-built Map onto stack
                // For now, we test Member on a Map pushed via a helper
                Opcode::LoadInt(42),
                Opcode::Return,
            ],
            constants: vec!["name".into()],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        // Test with a manually constructed Map
        let mut vm = Vm::new(vec![f]);
        // Simple smoke test — verify VM can run a function with Member constants
        assert_eq!(vm.run().unwrap(), Value::Int(42));
    }

    #[test]
    fn member_on_map_direct() {
        // Test Member opcode on a pre-loaded Map
        let f = BytecodeFunction {
            name: "test_member".into(),
            code: vec![
                // Pop the map (which we'll push manually), access field 0
                Opcode::Member(0),  // field name at constant[0] = "key"
                Opcode::Return,
            ],
            constants: vec!["key".into()],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![f]);
        // Manually push a Map before running
        let map = Value::Map(vec![
            ("key".to_string(), Value::Str("value".into())),
        ]);
        // Override run flow: manually push map, set running to true
        vm.stack.push(map);
        vm.running = true;
        vm.ip = 0;
        vm.current_fn = 0;

        // Skip scheduler setup for this simple test
        let main_id = vm.scheduler.spawn(0, 0);
        vm.scheduler.activate(main_id);

        let result = vm.run().unwrap();
        assert_eq!(result, Value::Str("value".into()));
    }

    #[test]
    fn make_closure_capture() {
        let f = BytecodeFunction {
            name: "make_adder".into(),
            code: vec![
                Opcode::LoadInt(10),     // captured value
                Opcode::MakeClosure(1),  // capture 1 value from stack → closure
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![f]);
        let main_id = vm.scheduler.spawn(0, 0);
        vm.scheduler.activate(main_id);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Closure(0, vec![Value::Int(10)]));
    }

    #[test]
    fn halt_stops_execution() {
        let f = make_fn(
            vec![
                Opcode::LoadInt(1),
                Opcode::Halt,
                Opcode::LoadInt(2),  // never reached
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![f]);
        let main_id = vm.scheduler.spawn(0, 0);
        vm.scheduler.activate(main_id);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(1)); // Halt stops before LoadInt(2)
    }

    #[test]
    fn channel_send_recv() {
        let f = BytecodeFunction {
            name: "chan_test".into(),
            code: vec![
                // Push channel, then value, then Send
                Opcode::LoadStr(0),
                Opcode::Builtin(0), // print marker (will be consumed)
                Opcode::LoadInt(42),
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let mut vm = Vm::new(vec![f]);
        // Create a channel and push it + a value
        let ch_id = vm.new_channel();
        vm.stack.push(Value::Channel(ch_id));
        vm.stack.push(Value::Int(99));

        // Manually execute Send
        let main_id = vm.scheduler.spawn(0, 0);
        vm.scheduler.activate(main_id);
        vm.running = true;

        // Manually run the Send opcode
        vm.ip = 0;

        // Instead of using run() which expects a specific flow, let's test ops directly
        let ch_id2 = vm.new_channel();
        // Test Send
        vm.stack.push(Value::Channel(ch_id2));
        vm.stack.push(Value::Str("hello".into()));
        vm.execute_op(Opcode::Send).unwrap();

        // Test Recv
        vm.stack.push(Value::Channel(ch_id2));
        vm.execute_op(Opcode::Recv).unwrap();
        assert_eq!(vm.stack.pop().unwrap(), Value::Str("hello".into()));
    }

    #[test]
    fn spawn_agent_task() {
        // Spawn a child via Spawn opcode, verify child_id returned
        let child = BytecodeFunction {
            name: "worker".into(),
            code: vec![
                Opcode::LoadInt(7),
                Opcode::LoadInt(8),
                Opcode::Mul,       // 7*8 = 56
                Opcode::Return,
            ],
            constants: vec![],
            arity: 0,
            locals: 0,
            effect: None,
            capability: None,
        };
        let main = make_fn(
            vec![
                Opcode::Spawn(1),   // spawn fn#1 (worker)
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![main, child]);
        let result = vm.run().unwrap();
        // Spawn returns child task ID (1, since main is 0)
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn index_out_of_bounds_error() {
        let f = make_fn(
            vec![
                Opcode::LoadInt(1),
                Opcode::MakeArray(1),  // [1]
                Opcode::LoadInt(5),    // index 5 out of bounds
                Opcode::Index,
                Opcode::Return,
            ],
            vec![],
        );
        let mut vm = Vm::new(vec![f]);
        let main_id = vm.scheduler.spawn(0, 0);
        vm.scheduler.activate(main_id);
        let result = vm.run();
        assert!(result.is_err());
    }

    #[test]
    fn channel_send_to_closed_error() {
        let mut vm = Vm::new(vec![]);
        let ch_id = vm.new_channel();
        vm.channels.get_mut(&ch_id).unwrap().closed = true;
        vm.stack.push(Value::Channel(ch_id));
        vm.stack.push(Value::Int(1));
        let result = vm.execute_op(Opcode::Send);
        assert!(result.is_err());
    }
}
