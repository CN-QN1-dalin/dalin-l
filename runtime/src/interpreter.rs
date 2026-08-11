use crate::env::{DALIN_TYPE_KEY, Environment, FnValue, Value};
/// Dalin L — 树遍历解释器
use dalin_compiler::ast::{Expr, FnParam, MatchArm, Pattern, Program, Stmt, TypeRef};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct RuntimeError(pub String);

// ── 控制流哨兵 ──
// 解释器用 Err(RuntimeError(哨兵)) 承载非局部跳转（return/break/continue）。
// 这些字符串以 `__` 包裹，用户代码无法构造同名错误，故不会误捕获。
/// 函数返回：由 `call_function` 拦截，取出 `self.return_value`。
const CTRL_RETURN: &str = "__return__";
/// 循环终止：由最内层 `eval_while`/`eval_for` 拦截。
const CTRL_BREAK: &str = "__break__";
/// 循环续跑：由最内层 `eval_while`/`eval_for` 拦截，跳过本轮剩余语句。
const CTRL_CONTINUE: &str = "__continue__";

/// 循环体一轮执行的结果信号。
enum LoopFlow {
    /// 正常执行完本轮
    Normal,
    /// 遇到 break，终止循环
    Break,
    /// 遇到 continue，进入下一轮
    Continue,
    /// 真实错误或 return，向上冒泡
    Err(RuntimeError),
}

/// 判断错误是否为控制流哨兵（而非真实运行时错误）。
fn ctrl_kind(err: &RuntimeError) -> Option<&'static str> {
    match err.0.as_str() {
        CTRL_RETURN => Some(CTRL_RETURN),
        CTRL_BREAK => Some(CTRL_BREAK),
        CTRL_CONTINUE => Some(CTRL_CONTINUE),
        _ => None,
    }
}

/// 任务树节点（持久化，存于跨线程共享注册表，供控制面视图）。
struct TaskNode {
    name: String,
    parent: Option<String>,
}

/// 全局任务序号，保证每次 spawn 获得唯一 id。
static TASK_SEQ: AtomicUsize = AtomicUsize::new(0);

fn next_task_id(name: &str) -> String {
    let seq = TASK_SEQ.fetch_add(1, Ordering::SeqCst);
    format!("{name}_{seq}")
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RuntimeError: {}", self.0)
    }
}

pub type EvalResult<T> = std::result::Result<T, RuntimeError>;

pub struct Interpreter {
    pub global_env: Environment,
    pub structs: HashMap<String, Vec<String>>,
    pub enums: HashMap<String, Vec<String>>,
    pub functions: HashMap<String, FnValue>,
    /// 命名空间隔离：裸函数名 → 唯一归属模块（同名冲突时为空，须显式 `module::func` 调用）
    bare_aliases: HashMap<String, String>,
    pub return_value: Option<Value>,
    // ── 并发原语运行时（跨线程共享注册表，本地模拟控制面任务树）──
    // 任务树：id -> 节点（含 parent 指针），持久保留供视图/调度用。
    task_tree: Arc<Mutex<HashMap<String, TaskNode>>>,
    // 任务结果通道：id -> Receiver，await 时取出消费（瞬态）。
    task_results: Arc<Mutex<HashMap<String, mpsc::Receiver<Value>>>>,
    // 通道接收端表：名称 -> Receiver（发送端随 Value 跨线程共享）
    #[allow(clippy::type_complexity)]
    channel_registry: Arc<Mutex<HashMap<String, Arc<Mutex<mpsc::Receiver<Value>>>>>>,
    // 当前任务 id（worker 线程内用于把子任务挂到正确父节点）
    current_task_id: Option<String>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    #[must_use]
    pub fn new() -> Self {
        let mut interp = Self {
            global_env: Environment::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
            bare_aliases: HashMap::new(),
            return_value: None,
            task_tree: Arc::new(Mutex::new(HashMap::new())),
            task_results: Arc::new(Mutex::new(HashMap::new())),
            channel_registry: Arc::new(Mutex::new(HashMap::new())),
            current_task_id: None,
        };
        interp.install_builtins();
        // 加载标准库：让 sqrt/str_len/vec_new 等 stdlib 函数在运行时可用
        if let Err(e) = interp.load_stdlib() {
            eprintln!("[runtime] stdlib load warning: {e}");
        }
        interp
    }

    pub fn interpret(&mut self, prog: &Program) -> Result<Vec<Value>, RuntimeError> {
        let mut results = Vec::new();
        let mut env = self.global_env.clone();
        for stmt in &prog.statements {
            let result = self.eval_stmt(stmt, &mut env)?;
            results.push(result);
        }
        self.global_env = env;
        Ok(results)
    }

    fn eval_stmt(&mut self, stmt: &Stmt, env: &mut Environment) -> Result<Value, RuntimeError> {
        match stmt {
            Stmt::Let { name, value, .. } => self.eval_let(name, value.as_deref(), env),
            Stmt::Const { name, value, .. } => {
                // const 绑定：求值后注册到环境（不可变，语义同 let 但仅绑定一次）
                let val = match value {
                    Some(e) => self.eval_expr(e, env)?,
                    None => Value::None,
                };
                env.define(name, val);
                Ok(Value::None)
            }
            Stmt::Fn {
                name,
                params,
                return_type,
                body,
                effect,
                capability,
                ..
            } => self.eval_fn_decl(name, params, return_type, body, effect, capability, env),
            Stmt::Return(v) => {
                let val = match v {
                    Some(e) => self.eval_expr(e, env)?,
                    None => Value::None,
                };
                self.return_value = Some(val);
                Err(RuntimeError(CTRL_RETURN.into()))
            }
            // 循环控制流：用哨兵错误向上冒泡，由最内层 eval_while/eval_for 拦截。
            // 未被循环拦截时会一路冒到 call_function，报为普通运行时错误（循环外使用）。
            Stmt::Break => Err(RuntimeError(CTRL_BREAK.into())),
            Stmt::Continue => Err(RuntimeError(CTRL_CONTINUE.into())),
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => self.eval_if(condition, then_body, else_body, env),
            Stmt::While { condition, body } => self.eval_while(condition, body, env),
            Stmt::For {
                target,
                iterable,
                body,
            } => self.eval_for(target, iterable, body, env),
            Stmt::Match { target, arms } => self.eval_match(target, arms, env),
            Stmt::StructDef { name, fields, .. } => {
                self.structs.insert(
                    name.clone(),
                    fields.iter().map(|f| f.name.clone()).collect(),
                );
                Ok(Value::None)
            }
            Stmt::EnumDef { name, variants, .. } => {
                self.enums.insert(
                    name.clone(),
                    variants.iter().map(|v| v.name.clone()).collect(),
                );
                Ok(Value::None)
            }
            Stmt::Spawn { fn_decl } => {
                // fn_decl 是 Stmt::Fn；spawn 要求效应标注为 spawn（效应格顶层，运行时强制）。
                if let Stmt::Fn {
                    name,
                    params,
                    return_type,
                    body,
                    effect,
                    capability,
                    ..
                } = fn_decl.as_ref()
                {
                    if effect.as_deref() != Some("spawn") {
                        return Err(RuntimeError(format!(
                            "spawn 要求被派生的函数标注 @ spawn（{name} 未标注效应）"
                        )));
                    }
                    if !params.is_empty() {
                        // 暂不支持参数传递（spawn fn `f(x)` 语法糖，用 `spawn_task` 传参即可）
                    }
                    let fnv = FnValue {
                        name: name.clone(),
                        params: params.clone(),
                        body: body.to_vec(),
                        closure: env.clone(),
                        return_type: return_type.clone(),
                        effect: effect.clone(),
                        capability: capability.clone(),
                    };
                    // 生成唯一任务 id，注册到跨线程共享的任务树（parent = 当前任务）。
                    let task_id = next_task_id(name);
                    let (tx, rx) = mpsc::channel();
                    {
                        let mut tree = self.task_tree.lock().unwrap();
                        tree.insert(
                            task_id.clone(),
                            TaskNode {
                                name: name.clone(),
                                parent: self.current_task_id.clone(),
                            },
                        );
                    }
                    {
                        let mut results = self.task_results.lock().unwrap();
                        results.insert(task_id.clone(), rx);
                    }
                    let child_functions = self.functions.clone();
                    let child_task_tree = self.task_tree.clone();
                    let child_task_results = self.task_results.clone();
                    let child_channel_registry = self.channel_registry.clone();
                    let child_task_id = task_id.clone();
                    std::thread::spawn(move || {
                        let mut child = Interpreter::new();
                        child.functions = child_functions;
                        child.task_tree = child_task_tree;
                        child.task_results = child_task_results;
                        child.channel_registry = child_channel_registry;
                        child.current_task_id = Some(child_task_id);
                        let res = child.call_function(&fnv, &[]);
                        let _ = tx.send(res.unwrap_or(Value::None));
                    });
                    // 任务句柄绑定到函数名，供 await 使用（Value 持有唯一 task id）。
                    let task = Value::Task(task_id);
                    env.define(name, task.clone());
                    Ok(task)
                } else {
                    Err(RuntimeError("spawn 必须后接函数定义".into()))
                }
            }
            Stmt::Channel {
                send_name,
                recv_name,
                ..
            } => {
                let (tx, rx) = mpsc::channel();
                env.define(send_name, Value::ChannelSender(Arc::new(tx)));
                // 接收端 Receiver 存共享注册表，Value 仅持有名称（保持 Value: Send）。
                self.channel_registry
                    .lock()
                    .unwrap()
                    .insert(recv_name.clone(), Arc::new(Mutex::new(rx)));
                env.define(recv_name, Value::ChannelReceiver(recv_name.clone()));
                Ok(Value::None)
            }
            Stmt::Assert { condition, message } => {
                let cond = self.eval_expr(condition, env)?;
                if !self.truthy(&cond) {
                    let msg = message
                        .as_ref()
                        .map(|m| {
                            self.eval_expr(m, env)
                                .map(|v| format!("{v}"))
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    return Err(RuntimeError(format!("Assertion failed: {msg}")));
                }
                Ok(Value::None)
            }
            Stmt::Expr(e) => self.eval_expr(e, env),
            _ => Ok(Value::None),
        }
    }

    fn eval_let(
        &mut self,
        name: &str,
        value: Option<&Expr>,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let val = match value {
            Some(v) => self.eval_expr(v, env)?,
            None => Value::None,
        };
        env.define(name, val.clone());
        Ok(val)
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_fn_decl(
        &mut self,
        name: &str,
        params: &[FnParam],
        return_type: &Option<TypeRef>,
        body: &[Stmt],
        effect: &Option<String>,
        capability: &Option<String>,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        #[allow(clippy::too_many_arguments)]
        let fn_val = FnValue {
            name: name.to_string(),
            params: params.to_vec(),
            body: body.to_vec(),
            closure: env.clone(),
            return_type: return_type.clone(),
            effect: effect.clone(),
            capability: capability.clone(),
        };
        // 存储在函数表 + 环境（环境供外部调用，函数表供递归调用）
        self.functions.insert(name.to_string(), fn_val.clone());
        env.define(name, Value::Function(fn_val));
        Ok(Value::None)
    }

    fn eval_if(
        &mut self,
        condition: &Expr,
        then_body: &[Stmt],
        else_body: &[Stmt],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let cond = self.eval_expr(condition, env)?;
        if self.truthy(&cond) {
            self.eval_block(then_body, env)
        } else {
            self.eval_block(else_body, env)
        }
    }

    fn eval_while(
        &mut self,
        condition: &Expr,
        body: &[Stmt],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        loop {
            let cond_val = self.eval_expr(condition, env)?;
            if !self.truthy(&cond_val) {
                break;
            }
            // 逐语句执行（不开子作用域：Dalin L while 体内 let 需跨迭代持久化）。
            // break/continue 哨兵在此拦截；return 与真实错误继续向上冒泡。
            match self.run_loop_body(body, env) {
                LoopFlow::Normal => {}
                LoopFlow::Break => break,
                LoopFlow::Continue => continue,
                LoopFlow::Err(e) => return Err(e),
            }
        }
        Ok(Value::None)
    }

    /// 执行一轮循环体，把 break/continue 哨兵翻译为结构化控制流信号。
    fn run_loop_body(&mut self, body: &[Stmt], env: &mut Environment) -> LoopFlow {
        for s in body {
            if let Err(e) = self.eval_stmt(s, env) {
                return match ctrl_kind(&e) {
                    Some(CTRL_BREAK) => LoopFlow::Break,
                    Some(CTRL_CONTINUE) => LoopFlow::Continue,
                    // CTRL_RETURN 与真实错误一律上抛。
                    _ => LoopFlow::Err(e),
                };
            }
        }
        LoopFlow::Normal
    }

    fn eval_for(
        &mut self,
        target: &str,
        iterable: &Expr,
        body: &[Stmt],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let iter = self.eval_expr(iterable, env)?;
        let items = self.as_iterable(&iter);
        for item in items {
            env.define(target, item.clone());
            match self.run_loop_body(body, env) {
                LoopFlow::Normal => {}
                LoopFlow::Break => break,
                LoopFlow::Continue => continue,
                LoopFlow::Err(e) => return Err(e),
            }
        }
        Ok(Value::None)
    }

    fn eval_match(
        &mut self,
        target: &Expr,
        arms: &[MatchArm],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let target_val = self.eval_expr(target, env)?;
        for arm in arms {
            let mut arm_env = env.child();
            if self.match_pattern(&arm.pattern, &target_val, &mut arm_env) {
                if let Some(guard) = &arm.guard {
                    let guard_val = self.eval_expr(guard, &mut arm_env)?;
                    if !self.truthy(&guard_val) {
                        continue;
                    }
                }
                return self.eval_block(&arm.body, &mut arm_env);
            }
        }
        Err(RuntimeError("Match failure: no arm matched".into()))
    }

    fn eval_block(&mut self, stmts: &[Stmt], env: &mut Environment) -> Result<Value, RuntimeError> {
        let mut last = Value::None;
        for s in stmts {
            last = self.eval_stmt(s, env)?;
        }
        Ok(last)
    }

    fn eval_expr(&mut self, expr: &Expr, env: &mut Environment) -> Result<Value, RuntimeError> {
        match expr {
            Expr::IntLiteral(v) => Ok(Value::Int(*v)),
            Expr::FloatLiteral(v) => Ok(Value::Float(*v)),
            Expr::StringLiteral(v) => Ok(Value::String(v.clone())),
            Expr::BoolLiteral(v) => Ok(Value::Bool(*v)),
            Expr::CharLiteral(v) => Ok(Value::Char(*v)),
            Expr::Ident(name) => self.eval_ident(name, env),
            Expr::StructLiteral { name, fields } => self.eval_struct_literal(name, fields, env),
            Expr::BinaryOp { left, op, right } => self.eval_binary(left, op, right, env),
            Expr::UnaryOp { op, operand } => self.eval_unary(op, operand, env),
            Expr::Call { func, args } => self.eval_call(func, args, env),
            Expr::MemberAccess { object, member } => self.eval_member_access(object, member, env),
            Expr::Index { array, index } => self.eval_index(array, index, env),
            Expr::Pipe { input, ops } => self.eval_pipe(input, ops, env),
            Expr::Range { start, end, .. } => self.eval_range(start, end, env),
            Expr::Array(elems) => self.eval_array(elems, env),
            Expr::OptionValue { is_some, value } => {
                if *is_some {
                    if let Some(v) = value {
                        Ok(Value::Option(true, Some(Box::new(self.eval_expr(v, env)?))))
                    } else {
                        Ok(Value::Option(true, None))
                    }
                } else {
                    Ok(Value::Option(false, None))
                }
            }
            Expr::ResultValue {
                is_ok,
                value,
                error,
            } => {
                if *is_ok {
                    if let Some(v) = value {
                        Ok(Value::Result(
                            true,
                            Some(Box::new(self.eval_expr(v, env)?)),
                            None,
                        ))
                    } else {
                        Ok(Value::Result(true, None, None))
                    }
                } else if let Some(e) = error {
                    Ok(Value::Result(
                        false,
                        None,
                        Some(Box::new(self.eval_expr(e, env)?)),
                    ))
                } else {
                    Ok(Value::Result(*is_ok, None, None))
                }
            }
            Expr::IfExpr(cond, then_expr, else_expr) => {
                let c = self.eval_expr(cond, env)?;
                if self.truthy(&c) {
                    self.eval_expr(then_expr, env)
                } else {
                    self.eval_expr(else_expr, env)
                }
            }
            Expr::MatchExpr(target, arms) => {
                let t = self.eval_expr(target, env)?;
                for arm in arms {
                    let mut arm_env = env.child();
                    if self.match_pattern(&arm.pattern, &t, &mut arm_env) {
                        let body_stmt = &arm.body;
                        return self.eval_block(body_stmt, &mut arm_env);
                    }
                }
                Err(RuntimeError("Match expression failure".into()))
            }
        }
    }

    /// 结构体字面量求值：`Point { x: 1, y: 2 }`
    ///
    /// 与位置构造器 `Point(1, 2)` 共用 `DALIN_TYPE_KEY` 约定，但增加字段校验：
    /// 未定义类型、未知字段、缺失字段一律 fail-fast，不构造半成品对象。
    fn eval_struct_literal(
        &mut self,
        name: &str,
        fields: &[(String, Expr)],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let Some(declared) = self.structs.get(name).cloned() else {
            return Err(RuntimeError(format!("Undefined struct type: '{name}'")));
        };

        let mut map = HashMap::new();
        map.insert(DALIN_TYPE_KEY.to_string(), Value::String(name.to_string()));

        for (field_name, field_expr) in fields {
            if !declared.contains(field_name) {
                return Err(RuntimeError(format!(
                    "Struct '{name}' has no field '{field_name}' (declared: {})",
                    declared.join(", ")
                )));
            }
            let value = self.eval_expr(field_expr, env)?;
            map.insert(field_name.clone(), value);
        }

        let missing: Vec<&String> = declared.iter().filter(|d| !map.contains_key(*d)).collect();
        if !missing.is_empty() {
            let names: Vec<String> = missing.into_iter().cloned().collect();
            return Err(RuntimeError(format!(
                "Struct '{name}' missing field(s): {}",
                names.join(", ")
            )));
        }

        Ok(Value::Struct(map))
    }

    fn eval_ident(&mut self, name: &str, env: &Environment) -> Result<Value, RuntimeError> {
        if let Some(v) = env.lookup(name) {
            return Ok(v);
        }
        // Check enum variants
        for (enum_name, variants) in &self.enums {
            if variants.contains(&name.to_string()) {
                return Ok(Value::EnumVariant(enum_name.clone(), name.to_string()));
            }
        }
        Err(RuntimeError(format!("Undefined variable: '{name}'")))
    }

    /// Binary operator evaluation.
    ///
    /// SAFETY INVARIANTS — see `docs/runtime-safety-invariants.md`:
    /// - INV-1: `&&` / `||` MUST short-circuit (right operand evaluated only when its
    ///   result can affect the outcome); never eager-evaluate both operands first.
    /// - INV-2: integer `+ - *` use `checked_*`; `/ %` guard against zero. Overflow and
    ///   division/modulo-by-zero return `RuntimeError`, never a Rust panic.
    fn eval_binary(
        &mut self,
        left: &Expr,
        op: &str,
        right: &Expr,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        // Assignment
        if op == "=" {
            let right_val = self.eval_expr(right, env)?;
            match left {
                Expr::Ident(name) => {
                    if !env.assign(name, right_val.clone()) {
                        return Err(RuntimeError(format!(
                            "Cannot assign to undefined variable: '{name}'"
                        )));
                    }
                    return Ok(right_val);
                }
                Expr::Index { array, index } => {
                    let arr = self.eval_expr(array, env)?;
                    let idx = self.eval_expr(index, env)?;
                    if let (Value::Array(mut a), Value::Int(i)) = (arr, idx) {
                        let i = i as usize;
                        if i < a.len() {
                            a[i] = right_val.clone();
                            return Ok(right_val);
                        }
                    }
                    return Err(RuntimeError("Invalid array assignment".into()));
                }
                _ => return Err(RuntimeError("Invalid assignment target".into())),
            }
        }

        // Short-circuit logical operators: the right operand is only
        // evaluated when its result can affect the outcome. This prevents
        // side effects in the right operand (e.g. division by zero, index
        // out of bounds) from firing when the left operand already decides
        // the result — the expected semantics of && / || .
        if op == "&&" {
            let left_val = self.eval_expr(left, env)?;
            if !self.truthy(&left_val) {
                return Ok(Value::Bool(false));
            }
            let right_val = self.eval_expr(right, env)?;
            return Ok(Value::Bool(self.truthy(&right_val)));
        }
        if op == "||" {
            let left_val = self.eval_expr(left, env)?;
            if self.truthy(&left_val) {
                return Ok(Value::Bool(true));
            }
            let right_val = self.eval_expr(right, env)?;
            return Ok(Value::Bool(self.truthy(&right_val)));
        }

        let left_val = self.eval_expr(left, env)?;
        let right_val = self.eval_expr(right, env)?;

        match op {
            "+" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => a
                    .checked_add(*b)
                    .map(Value::Int)
                    .ok_or_else(|| RuntimeError("integer overflow in addition".into())),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
                (Value::String(a), b) => Ok(Value::String(format!("{a}{b}"))),
                (a, Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
                // 数组拼接：a + [x] / a + b（元素级拼接）
                (Value::Array(a), Value::Array(b)) => {
                    let mut out = a.clone();
                    out.extend(b.iter().cloned());
                    Ok(Value::Array(out))
                }
                (Value::Array(a), b) => {
                    let mut out = a.clone();
                    out.push(b.clone());
                    Ok(Value::Array(out))
                }
                (a, Value::Array(b)) => {
                    let mut out = vec![a.clone()];
                    out.extend(b.iter().cloned());
                    Ok(Value::Array(out))
                }
                _ => Err(RuntimeError(format!(
                    "Cannot add {left_val:?} and {right_val:?}"
                ))),
            },
            "-" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => a
                    .checked_sub(*b)
                    .map(Value::Int)
                    .ok_or_else(|| RuntimeError("integer overflow in subtraction".into())),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
                _ => Err(RuntimeError(format!(
                    "Cannot subtract {left_val:?} and {right_val:?}"
                ))),
            },
            "*" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => a
                    .checked_mul(*b)
                    .map(Value::Int)
                    .ok_or_else(|| RuntimeError("integer overflow in multiplication".into())),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
                _ => Err(RuntimeError(format!(
                    "Cannot multiply {left_val:?} and {right_val:?}"
                ))),
            },
            "/" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => {
                    if *b == 0 {
                        return Err(RuntimeError("division by zero".into()));
                    }
                    a.checked_div(*b)
                        .map(Value::Int)
                        .ok_or_else(|| RuntimeError("integer overflow in division".into()))
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                _ => Err(RuntimeError(format!(
                    "Cannot divide {left_val:?} and {right_val:?}"
                ))),
            },
            "%" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => {
                    if *b == 0 {
                        return Err(RuntimeError("modulo by zero".into()));
                    }
                    Ok(Value::Int(a % b))
                }
                _ => Err(RuntimeError(format!(
                    "Cannot modulo {left_val:?} and {right_val:?}"
                ))),
            },
            "==" => Ok(Value::Bool(self.values_equal(&left_val, &right_val))),
            "!=" => Ok(Value::Bool(!self.values_equal(&left_val, &right_val))),
            "<" | ">" | "<=" | ">=" => self.compare(&left_val, &right_val, op),
            _ => Err(RuntimeError(format!("Unknown operator: {op}"))),
        }
    }

    fn eval_unary(
        &mut self,
        op: &str,
        operand: &Expr,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let val = self.eval_expr(operand, env)?;
        match op {
            "-" => match val {
                Value::Int(v) => Ok(Value::Int(-v)),
                Value::Float(v) => Ok(Value::Float(-v)),
                _ => Err(RuntimeError(format!("Cannot negate {val:?}"))),
            },
            "!" => Ok(Value::Bool(!self.truthy(&val))),
            _ => Err(RuntimeError(format!("Unknown unary op: {op}"))),
        }
    }

    fn eval_call(
        &mut self,
        func: &Expr,
        args: &[Expr],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        // 解析被调用名 + 可选的命名空间前缀（如 `strings::str_reverse`）
        let (callee_name, namespace) = match func {
            Expr::Ident(name) => (name.clone(), None),
            Expr::MemberAccess { object, member } => {
                let ns = match object.as_ref() {
                    Expr::Ident(n) => Some(n.clone()),
                    _ => None,
                };
                (member.clone(), ns)
            }
            _ => return Err(RuntimeError("Invalid call expression".into())),
        };

        // Evaluate all arguments upfront to avoid borrow conflicts
        let mut arg_vals = Vec::new();
        for a in args {
            arg_vals.push(self.eval_expr(a, env)?);
        }

        // Builtins
        let builtins: [&str; 37] = [
            "println",
            "println!",
            "print",
            "print!",
            "len",
            "push",
            "assert",
            "int",
            "float",
            "str",
            "abs",
            "range",
            "await",
            "send",
            "recv",
            "spawn_task",
            // 系统 / 密码学 / 文件系统 内建（stdlib 桩模块实质化依赖）
            "sys_now",
            "rand_int",
            "sha256",
            "hmac_sha256",
            "read_file",
            "write_file",
            // 字符 ↔ 整数 转换（hex/base64 等模块实质化依赖）
            "ord",
            "chr",
            // 类型判定内建（core_types.type_of 依赖；修复此前 is_* 与 type_of
            // 互相递归导致无限循环、以及 is_int 对 Char 使用 >=/<= 在 Dalin L 中
            // 不受支持而崩溃的问题）
            "is_int",
            "is_float",
            "is_bool",
            "is_string",
            "is_list",
            "is_map",
            "is_fn",
            "is_char",
            // 结构化类型自省（补齐 Option/None/Struct/Enum 的运行时判定）
            "is_none",
            "is_some",
            "is_option",
            "is_struct",
            "is_enum",
        ];

        // 1) 命名空间精确解析（module::func 优先，实现真正的命名空间隔离）
        if let Some(ns) = &namespace {
            let qualified = format!("{}::{}", ns, callee_name);
            if let Some(fnv) = self.functions.get(&qualified).cloned() {
                return self.call_function(&fnv, &arg_vals);
            }
            // 限定名未命中：若非内置名则继续回退；内置名仍允许（如 `strings::len` → 内置 len）
        }

        // 2) 内置函数（裸名或限定名回退）
        if builtins.contains(&callee_name.as_str()) {
            return self.call_builtin(&callee_name, &arg_vals);
        }

        // 3) 结构体构造器（裸名）
        if let Some(fields) = self.structs.get(&callee_name).cloned() {
            let mut map = HashMap::new();
            map.insert(
                DALIN_TYPE_KEY.to_string(),
                Value::String(callee_name.clone()),
            );
            for (fname, fval) in fields.iter().zip(arg_vals) {
                map.insert(fname.clone(), fval);
            }
            return Ok(Value::Struct(map));
        }

        // 4) 用户顶层函数（裸名注册，支持递归；优先级高于 stdlib 裸别名）
        if let Some(fnv) = self.functions.get(&callee_name).cloned() {
            return self.call_function(&fnv, &arg_vals);
        }

        // 5) stdlib 裸别名（模块唯一或确定性首选模块），向后兼容 `func(...)` 调用风格
        if let Some(module) = self.bare_aliases.get(&callee_name).cloned() {
            let qualified = format!("{}::{}", module, callee_name);
            if let Some(fnv) = self.functions.get(&qualified).cloned() {
                return self.call_function(&fnv, &arg_vals);
            }
        }

        // 6) 环境闭包查找（局部/用户函数）
        match env.lookup(&callee_name) {
            Some(Value::Function(fnv)) => self.call_function(&fnv, &arg_vals),
            Some(_) => Err(RuntimeError(format!("'{callee_name}' is not callable"))),
            None => {
                let display = match &namespace {
                    Some(ns) => format!("{ns}::{callee_name}"),
                    None => callee_name.clone(),
                };
                Err(RuntimeError(format!("Undefined function: '{display}'")))
            }
        }
    }

    fn call_function(&mut self, fnv: &FnValue, args: &[Value]) -> Result<Value, RuntimeError> {
        if args.len() != fnv.params.len() {
            return Err(RuntimeError(format!(
                "Function '{}' expects {} args, got {}",
                fnv.name,
                fnv.params.len(),
                args.len()
            )));
        }
        let mut call_env = fnv.closure.child();
        for (param, arg) in fnv.params.iter().zip(args.iter()) {
            call_env.define(&param.name, arg.clone());
        }
        self.return_value = None;
        let result = self.eval_block(&fnv.body, &mut call_env);
        match result {
            Err(RuntimeError(ref msg)) if msg == CTRL_RETURN => {
                Ok(self.return_value.take().unwrap_or(Value::None))
            }
            // 哨兵逃出函数边界 = 循环外使用 break/continue，翻译为可读的诊断信息。
            Err(RuntimeError(ref msg)) if msg == CTRL_BREAK || msg == CTRL_CONTINUE => {
                let kw = if msg == CTRL_BREAK {
                    "break"
                } else {
                    "continue"
                };
                Err(RuntimeError(format!(
                    "'{kw}' 只能用于 while/for 循环体内（函数 '{}' 中越界使用）",
                    fnv.name
                )))
            }
            Ok(_) => Ok(Value::None),
            Err(e) => Err(e),
        }
    }

    fn call_builtin(&mut self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        match name {
            "println" | "println!" => {
                let s: Vec<String> = args.iter().map(|a| format!("{a}")).collect();
                println!("{}", s.join(" "));
                Ok(Value::None)
            }
            "print" | "print!" => {
                let s: Vec<String> = args.iter().map(|a| format!("{a}")).collect();
                print!("{}", s.join(" "));
                // stdout 行缓冲：不换行的 print 必须显式 flush，
                // 否则输出滞留缓冲区，与 stderr/子进程输出交错错序。
                let _ = std::io::Write::flush(&mut std::io::stdout());
                Ok(Value::None)
            }
            "len" => match &args[0] {
                Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                // UTF-8 安全：字符数而非字节数，与 eval_index 的 chars().nth() 口径一致。
                // 若返回字节数，`while i < len(s) { s[i] }` 在任何非 ASCII 输入下必然越界。
                Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                _ => Ok(Value::Int(0)),
            },
            "push" => {
                if let Value::Array(ref mut arr) = args[0].clone() {
                    let mut arr = arr.clone();
                    arr.push(args[1].clone());
                    Ok(Value::Array(arr))
                } else {
                    Err(RuntimeError("push requires array".into()))
                }
            }
            "int" => match &args[0] {
                Value::String(s) => s.parse::<i64>().map(Value::Int).or(Ok(Value::Int(0))),
                Value::Float(f) => Ok(Value::Int(*f as i64)),
                Value::Int(i) => Ok(Value::Int(*i)),
                _ => Ok(Value::Int(0)),
            },
            "float" => match &args[0] {
                Value::String(s) => s.parse::<f64>().map(Value::Float).or(Ok(Value::Float(0.0))),
                Value::Int(i) => Ok(Value::Float(*i as f64)),
                Value::Float(f) => Ok(Value::Float(*f)),
                _ => Ok(Value::Float(0.0)),
            },
            "str" => Ok(Value::String(format!("{}", args[0]))),
            "abs" => match args[0] {
                Value::Int(i) => Ok(Value::Int(i.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                _ => Err(RuntimeError("abs requires number".into())),
            },
            "range" => {
                if let (Value::Int(a), Value::Int(b)) = (&args[0], &args[1]) {
                    let items: Vec<Value> = (*a..*b).map(Value::Int).collect();
                    Ok(Value::Array(items))
                } else {
                    Err(RuntimeError("range requires int args".into()))
                }
            }
            "assert" => {
                if args.len() > 1 && !self.truthy(&args[0]) {
                    return Err(RuntimeError(format!("Assertion failed: {}", args[1])));
                }
                Ok(Value::None)
            }
            "await" => {
                if args.is_empty() {
                    return Err(RuntimeError("await 需要 task 参数".into()));
                }
                if let Value::Task(id) = &args[0] {
                    let rx = self.task_results.lock().unwrap().remove(id);
                    match rx {
                        Some(r) => match r.recv() {
                            Ok(v) => Ok(v),
                            Err(_) => Ok(Value::None),
                        },
                        None => Err(RuntimeError(format!("未知 task: {id}"))),
                    }
                } else {
                    Err(RuntimeError("await 的参数必须是 task".into()))
                }
            }
            "send" => {
                if args.len() < 2 {
                    return Err(RuntimeError("send 需要 channel 和值两个参数".into()));
                }
                if let Value::ChannelSender(tx) = &args[0] {
                    match tx.send(args[1].clone()) {
                        Ok(()) => Ok(Value::None),
                        Err(_) => Err(RuntimeError("send 失败：通道已关闭".into())),
                    }
                } else {
                    Err(RuntimeError("send 的第一个参数必须是 channel".into()))
                }
            }
            "recv" => {
                if args.is_empty() {
                    return Err(RuntimeError("recv 需要 channel 参数".into()));
                }
                if let Value::ChannelReceiver(name) = &args[0] {
                    let rx_arc = self.channel_registry.lock().unwrap().get(name).cloned();
                    match rx_arc {
                        Some(rx_mutex) => {
                            let rx = rx_mutex.lock().unwrap();
                            match rx.recv() {
                                Ok(v) => Ok(v),
                                Err(_) => Ok(Value::None),
                            }
                        }
                        None => Err(RuntimeError(format!("未知 channel: {name}"))),
                    }
                } else {
                    Err(RuntimeError("recv 的参数必须是 channel".into()))
                }
            }
            "spawn_task" => {
                if args.is_empty() {
                    return Err(RuntimeError("spawn_task 需要函数名".into()));
                }
                let fname = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => {
                        return Err(RuntimeError(
                            "spawn_task 第一个参数必须是函数名（字符串）".into(),
                        ));
                    }
                };
                let fnv = match self.functions.get(&fname).cloned() {
                    Some(f) => f,
                    None => return Err(RuntimeError(format!("spawn_task: 未定义函数 {fname}"))),
                };
                if fnv.effect.as_deref() != Some("spawn") {
                    return Err(RuntimeError(format!(
                        "spawn_task: {fname} 必须标注 @ spawn 才能被派生"
                    )));
                }
                let call_args: Vec<Value> = args[1..].to_vec();
                let child_id = next_task_id(&fname);
                let (tx, rx) = mpsc::channel();
                {
                    let mut tree = self.task_tree.lock().unwrap();
                    tree.insert(
                        child_id.clone(),
                        TaskNode {
                            name: fname.clone(),
                            parent: self.current_task_id.clone(),
                        },
                    );
                }
                {
                    let mut results = self.task_results.lock().unwrap();
                    results.insert(child_id.clone(), rx);
                }
                let child_functions = self.functions.clone();
                let child_task_tree = self.task_tree.clone();
                let child_task_results = self.task_results.clone();
                let child_channel_registry = self.channel_registry.clone();
                let child_task_id = child_id.clone();
                std::thread::spawn(move || {
                    let mut child = Interpreter::new();
                    child.functions = child_functions;
                    child.task_tree = child_task_tree;
                    child.task_results = child_task_results;
                    child.channel_registry = child_channel_registry;
                    child.current_task_id = Some(child_task_id);
                    let res = child.call_function(&fnv, &call_args);
                    let _ = tx.send(res.unwrap_or(Value::None));
                });
                Ok(Value::Task(child_id))
            }
            "sys_now" => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let dur = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                Ok(Value::Int(dur.as_millis() as i64))
            }
            "rand_int" => {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                if args.is_empty() {
                    return Err(RuntimeError("rand_int 需要 max 或 min,max".into()));
                }
                if args.len() == 1 {
                    let max = match &args[0] {
                        Value::Int(i) => *i,
                        _ => return Err(RuntimeError("rand_int 的参数必须是整数".into())),
                    };
                    if max <= 0 {
                        return Ok(Value::Int(0));
                    }
                    Ok(Value::Int(rng.gen_range(0..max)))
                } else {
                    let min = match &args[0] {
                        Value::Int(i) => *i,
                        _ => return Err(RuntimeError("rand_int 的参数必须是整数".into())),
                    };
                    let max = match &args[1] {
                        Value::Int(i) => *i,
                        _ => return Err(RuntimeError("rand_int 的参数必须是整数".into())),
                    };
                    if max <= min {
                        return Ok(Value::Int(min));
                    }
                    Ok(Value::Int(rng.gen_range(min..max)))
                }
            }
            "sha256" => {
                use sha2::{Digest, Sha256};
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                let mut hasher = Sha256::new();
                hasher.update(s.as_bytes());
                let out = hasher.finalize();
                let hex: String = out.iter().map(|b| format!("{b:02x}")).collect();
                Ok(Value::String(hex))
            }
            "hmac_sha256" => {
                use sha2::{Digest, Sha256};
                let key = match &args[0] {
                    Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                let data = match &args[1] {
                    Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                let block = 64usize;
                let key_bytes = key.as_bytes();
                // 1) 规范 key 到 64 字节：超长先哈希，不足右侧补 0
                let mut k = vec![0u8; block];
                if key_bytes.len() > block {
                    let mut h = Sha256::new();
                    h.update(key_bytes);
                    let d = h.finalize();
                    k[..32].copy_from_slice(&d);
                } else {
                    k[..key_bytes.len()].copy_from_slice(key_bytes);
                }
                // 2) ipad / opad
                let mut inner = Vec::with_capacity(block + data.len());
                let mut outer = Vec::with_capacity(block + 32);
                for &kb in &k {
                    inner.push(kb ^ 0x36);
                    outer.push(kb ^ 0x5c);
                }
                inner.extend_from_slice(data.as_bytes());
                let mut h1 = Sha256::new();
                h1.update(&inner);
                let inner_hash = h1.finalize();
                outer.extend_from_slice(&inner_hash);
                let mut h2 = Sha256::new();
                h2.update(&outer);
                let out = h2.finalize();
                let hex: String = out.iter().map(|b| format!("{b:02x}")).collect();
                Ok(Value::String(hex))
            }
            "read_file" => {
                let path = match &args[0] {
                    Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                match std::fs::read_to_string(&path) {
                    Ok(content) => Ok(Value::String(content)),
                    Err(e) => Err(RuntimeError(format!("read_file 失败: {e}"))),
                }
            }
            "write_file" => {
                let path = match &args[0] {
                    Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                let content = match &args[1] {
                    Value::String(s) => s.clone(),
                    other => format!("{other}"),
                };
                match std::fs::write(&path, content) {
                    Ok(()) => Ok(Value::Bool(true)),
                    Err(_) => Ok(Value::Bool(false)),
                }
            }
            // 字符 ↔ 整数（hex/base64 模块实质化依赖）
            "ord" => match &args[0] {
                Value::Char(c) => Ok(Value::Int(*c as u32 as i64)),
                Value::String(s) => {
                    let mut chars = s.chars();
                    match chars.next() {
                        Some(c) => Ok(Value::Int(c as u32 as i64)),
                        None => Err(RuntimeError("ord 需要非空字符串".into())),
                    }
                }
                other => Err(RuntimeError(format!(
                    "ord 需要 char 或字符串，得到 {other}"
                ))),
            },
            "chr" => match &args[0] {
                Value::Int(i) => {
                    let code = *i as u32;
                    match char::from_u32(code) {
                        Some(c) => Ok(Value::Char(c)),
                        None => Err(RuntimeError(format!("chr 参数越界: {i}"))),
                    }
                }
                other => Err(RuntimeError(format!("chr 需要整数，得到 {other}"))),
            },
            // ── 类型判定内建（core_types.type_of 依赖）──
            // 解决此前 is_* 与 type_of 互相递归导致无限循环，以及 is_int 对 Char
            // 使用 >= / <=（Dalin L 不支持）而崩溃的问题。is_map 约定：Array 且
            // 为空或所有元素均为二元组 [k, v]（与 json.dal 的 map 表示一致）。
            "is_int" => Ok(Value::Bool(matches!(args[0], Value::Int(_)))),
            "is_float" => Ok(Value::Bool(matches!(args[0], Value::Float(_)))),
            "is_bool" => Ok(Value::Bool(matches!(args[0], Value::Bool(_)))),
            "is_string" => Ok(Value::Bool(matches!(args[0], Value::String(_)))),
            "is_list" => Ok(Value::Bool(matches!(args[0], Value::Array(_)))),
            "is_fn" => Ok(Value::Bool(matches!(args[0], Value::Function(_)))),
            "is_char" => Ok(Value::Bool(matches!(args[0], Value::Char(_)))),
            "is_none" => Ok(Value::Bool(matches!(args[0], Value::Option(false, _)))),
            "is_some" => Ok(Value::Bool(matches!(args[0], Value::Option(true, _)))),
            "is_option" => Ok(Value::Bool(matches!(args[0], Value::Option(_, _)))),
            "is_struct" => Ok(Value::Bool(matches!(args[0], Value::Struct(_)))),
            "is_enum" => Ok(Value::Bool(matches!(args[0], Value::EnumVariant(_, _)))),
            "is_map" => Ok(Value::Bool(match &args[0] {
                Value::Array(a) => {
                    // map 约定：数组且每个元素都是 [String, v] 二元组。
                    // 空数组也视作 map（与 json_parse("{}") → [] 的表示一致，
                    // 使 json_get 在空 map 上不会因 type_of != "map" 而误拒）。
                    // 非空时要求「全部为字符串键二元组」，这样 ["x", "y"] 这类
                    // 普通二元数组不会被误判成 map。
                    a.is_empty()
                        || a.iter().all(|e| match e {
                            Value::Array(p) => p.len() == 2 && matches!(p[0], Value::String(_)),
                            _ => false,
                        })
                }
                _ => false,
            })),
            _ => Err(RuntimeError(format!("Unknown builtin: {name}"))),
        }
    }

    fn eval_member_access(
        &mut self,
        object: &Expr,
        member: &str,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let obj = self.eval_expr(object, env)?;
        match obj {
            Value::Struct(ref map) => {
                if let Some(v) = map.get(member) {
                    Ok(v.clone())
                } else {
                    let ty = map
                        .get(DALIN_TYPE_KEY)
                        .map(|v| format!("{v}"))
                        .unwrap_or_default();
                    Err(RuntimeError(format!(
                        "Struct '{ty}' has no field '{member}'"
                    )))
                }
            }
            _ => Err(RuntimeError(format!("Cannot access member '{member}'"))),
        }
    }

    fn eval_index(
        &mut self,
        array: &Expr,
        index: &Expr,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let arr = self.eval_expr(array, env)?;
        let idx = self.eval_expr(index, env)?;
        match (&arr, &idx) {
            (Value::Array(a), Value::Int(i)) => {
                let i = *i as usize;
                if i < a.len() {
                    Ok(a[i].clone())
                } else {
                    Err(RuntimeError(format!("Index out of range: {i}")))
                }
            }
            // 字符串索引: s[i] → Char（UTF-8 边界内）
            (Value::String(s), Value::Int(i)) => {
                let chars: Vec<char> = s.chars().collect();
                let i = *i as usize;
                if i < chars.len() {
                    Ok(Value::Char(chars[i]))
                } else {
                    Err(RuntimeError(format!("String index out of range: {i}")))
                }
            }
            _ => Err(RuntimeError("Invalid index operation".into())),
        }
    }

    fn eval_pipe(
        &mut self,
        input: &Expr,
        ops: &[(String, Expr)],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let mut current = self.eval_expr(input, env)?;
        for (name, _) in ops {
            match env.lookup(name) {
                Some(Value::Function(fnv)) => {
                    current = self.call_function(&fnv, &[current])?;
                }
                _ => {
                    return Err(RuntimeError(format!(
                        "Pipe target '{name}' is not callable"
                    )));
                }
            }
        }
        Ok(current)
    }

    fn eval_range(
        &mut self,
        start: &Expr,
        end: &Expr,
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
        let s = self.eval_expr(start, env)?;
        let e = self.eval_expr(end, env)?;
        match (s, e) {
            (Value::Int(a), Value::Int(b)) => {
                let items: Vec<Value> = (a..b).map(Value::Int).collect();
                Ok(Value::Array(items))
            }
            _ => Err(RuntimeError("Range requires int bounds".into())),
        }
    }

    fn eval_array(&mut self, elems: &[Expr], env: &mut Environment) -> Result<Value, RuntimeError> {
        let items: Result<Vec<Value>, RuntimeError> =
            elems.iter().map(|e| self.eval_expr(e, env)).collect();
        Ok(Value::Array(items?))
    }

    // ── 模式匹配 ──

    fn match_pattern(&mut self, pat: &Pattern, value: &Value, env: &mut Environment) -> bool {
        match pat.kind.as_str() {
            "wild" => true,
            "ident" => {
                env.define(&pat.name, value.clone());
                true
            }
            "lit" => {
                if let Some(lit_val) = &pat.value {
                    let lit = self.literal_to_value(lit_val);
                    self.values_equal(&lit, value)
                } else {
                    false
                }
            }
            "ctor" => {
                match pat.name.as_str() {
                    "Some" => {
                        if let Value::Option(true, Some(v)) = value
                            && let Some(ref binding) = pat.binding
                        {
                            env.define(binding, *v.clone());
                            return true;
                        }
                        false
                    }
                    "None" => matches!(value, Value::Option(false, _)),
                    "Ok" => {
                        if let Value::Result(true, Some(v), _) = value
                            && let Some(ref binding) = pat.binding
                        {
                            env.define(binding, *v.clone());
                            return true;
                        }
                        false
                    }
                    "Err" => {
                        if let Value::Result(false, _, Some(e)) = value
                            && let Some(ref binding) = pat.binding
                        {
                            env.define(binding, *e.clone());
                            return true;
                        }
                        false
                    }
                    _ => {
                        // Enum variant
                        if let Value::EnumVariant(_, vn) = value {
                            vn == &pat.name
                        } else {
                            false
                        }
                    }
                }
            }
            _ => false,
        }
    }

    fn literal_to_value(&self, expr: &Expr) -> Value {
        match expr {
            Expr::IntLiteral(v) => Value::Int(*v),
            Expr::FloatLiteral(v) => Value::Float(*v),
            Expr::StringLiteral(v) => Value::String(v.clone()),
            Expr::BoolLiteral(v) => Value::Bool(*v),
            Expr::CharLiteral(v) => Value::Char(*v),
            _ => Value::None,
        }
    }

    // ── 辅助 ──

    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Option(false, _) => false,
            Value::Option(true, _) => true,
            Value::Result(false, ..) => false,
            Value::Result(true, ..) => true,
            Value::EnumVariant(_, _) => true,
            Value::Struct(_) => true,
            Value::Function(_) => true,
            Value::Char(_) => true,
            Value::Task(_) => true,
            Value::ChannelSender(_) => true,
            Value::ChannelReceiver(_) => true,
        }
    }

    fn values_equal(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Int(ai), Value::Int(bi)) => ai == bi,
            (Value::Float(af), Value::Float(bf)) => (af - bf).abs() < 1e-10,
            // 跨类型数值比较：必须与 `compare()` 的数值提升语义保持一致。
            // 缺这两个分支会导致 `0.0 == 0` 恒为 false，而 `0.0 < 0` 却正常，
            // 语义不一致会在 stdlib 的边界判断里造成隐蔽错误（如 sqrt(0.0)）。
            (Value::Int(ai), Value::Float(bf)) => ((*ai as f64) - bf).abs() < 1e-10,
            (Value::Float(af), Value::Int(bi)) => (af - (*bi as f64)).abs() < 1e-10,
            (Value::String(as_), Value::String(bs)) => as_ == bs,
            (Value::Bool(ab), Value::Bool(bb)) => ab == bb,
            (Value::Char(ac), Value::Char(bc)) => ac == bc,
            (Value::None, Value::None) => true,
            // 容器按元素递归比较：否则 `[1,2] == [1,2]` 恒为 false，
            // 这类静默假值比报错更难排查。
            (Value::Array(xs), Value::Array(ys)) => {
                xs.len() == ys.len()
                    && xs
                        .iter()
                        .zip(ys.iter())
                        .all(|(x, y)| self.values_equal(x, y))
            }
            (Value::Option(sa, va), Value::Option(sb, vb)) => {
                sa == sb
                    && match (va, vb) {
                        (Some(x), Some(y)) => self.values_equal(x, y),
                        (None, None) => true,
                        _ => false,
                    }
            }
            (Value::Result(oa, va, ea), Value::Result(ob, vb, eb)) => {
                oa == ob
                    && match (va, vb) {
                        (Some(x), Some(y)) => self.values_equal(x, y),
                        (None, None) => true,
                        _ => false,
                    }
                    && match (ea, eb) {
                        (Some(x), Some(y)) => self.values_equal(x, y),
                        (None, None) => true,
                        _ => false,
                    }
            }
            (Value::EnumVariant(ea, va), Value::EnumVariant(eb, vb)) => ea == eb && va == vb,
            (Value::Struct(ma), Value::Struct(mb)) => {
                ma.len() == mb.len()
                    && ma
                        .iter()
                        .all(|(k, v)| mb.get(k).is_some_and(|other| self.values_equal(v, other)))
            }
            _ => false,
        }
    }

    fn compare(&self, a: &Value, b: &Value, op: &str) -> Result<Value, RuntimeError> {
        let cmp = match (a, b) {
            (Value::Int(ai), Value::Int(bi)) => Some(ai.cmp(bi)),
            (Value::Float(af), Value::Float(bf)) => {
                Some(af.partial_cmp(bf).unwrap_or(std::cmp::Ordering::Equal))
            }
            (Value::Int(ai), Value::Float(bf)) => Some(
                (*ai as f64)
                    .partial_cmp(bf)
                    .unwrap_or(std::cmp::Ordering::Equal),
            ),
            (Value::Float(af), Value::Int(bi)) => Some(
                af.partial_cmp(&(*bi as f64))
                    .unwrap_or(std::cmp::Ordering::Equal),
            ),
            (Value::String(as_), Value::String(bs)) => Some(as_.cmp(bs)),
            _ => None,
        };
        match cmp {
            Some(ord) => Ok(Value::Bool(match op {
                "<" => ord.is_lt(),
                ">" => ord.is_gt(),
                "<=" => ord.is_le(),
                ">=" => ord.is_ge(),
                _ => false,
            })),
            None => Err(RuntimeError(format!("Cannot compare {a:?} and {b:?}"))),
        }
    }

    fn as_iterable(&self, value: &Value) -> Vec<Value> {
        match value {
            Value::Array(a) => a.clone(),
            Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
            _ => vec![value.clone()],
        }
    }

    /// Return a text view of the task tree (a local miniature of the control-plane registry).
    #[must_use]
    pub fn describe_task_tree(&self) -> String {
        let tree = self.task_tree.lock().unwrap();
        let mut lines = vec!["=== 任务树（控制面注册表缩影）===".to_string()];
        if tree.is_empty() {
            lines.push("  (空)".to_string());
        }
        for (id, node) in tree.iter() {
            let parent = node.parent.as_deref().unwrap_or("<root>");
            lines.push(format!("  {} : name={} parent={}", id, node.name, parent));
        }
        lines.join("\n")
    }

    fn install_builtins(&mut self) {
        // Builtins are handled in eval_call
    }

    /// Load the standard library: compile stdlib/*.dal and register all function declarations into the runtime function table.
    ///
    /// This is the key bridge for stdlib from "parseable" to "executable":
    /// The stdlib's .dal files are Dalin source and must be compiled by the parser;
    /// their Fn declarations are converted into FnValues and injected into the functions table so user programs can call them.
    ///
    /// Failure policy: a parse failure in one file does not affect others (recorded and skipped),
    /// but functions from all parseable files are registered.
    pub fn load_stdlib(&mut self) -> Result<usize, String> {
        use dalin_compiler::ast::Stmt;
        use dalin_compiler::lexer::Lexer;
        use dalin_compiler::parser::Parser;
        use std::fs;

        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stdlib");
        let mut loaded = 0usize;
        let mut failed_files = Vec::new();

        // 收集所有 .dal 文件，按文件名排序保证模块加载顺序确定性
        let mut entries: Vec<_> = match fs::read_dir(&base) {
            Ok(d) => d.filter_map(|e| e.ok()).collect(),
            Err(e) => return Err(format!("cannot read stdlib dir: {e}")),
        };
        entries.sort_by_key(|e| e.path());

        // 裸名 → 所属模块 映射；同名出现在多个模块时保留首个定义模块作为确定性默认
        // （向后兼容 `func(...)` 调用风格），qualified `module::func` 始终精确命中目标模块。
        let mut bare_owner: HashMap<String, String> = HashMap::new();

        for entry in entries {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "dal") {
                let src = match fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        failed_files.push(format!("{}: {e}", path.display()));
                        continue;
                    }
                };
                let mut lex = Lexer::new(&src);
                let tokens = match lex.tokenize() {
                    Ok(t) => t,
                    Err(e) => {
                        failed_files.push(format!("{}: lex {e}", path.display()));
                        continue;
                    }
                };
                let mut parser = Parser::new(tokens);
                let (prog, errs) = match parser.parse() {
                    Ok(x) => x,
                    Err(e) => {
                        failed_files.push(format!("{}: parse {e}", path.display()));
                        continue;
                    }
                };
                if !errs.is_empty() {
                    failed_files.push(format!("{}: {} parse errors", path.display(), errs.len()));
                    continue;
                }
                // 模块名取自文件名（strings.dal → "strings"）
                let module = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                // 注册所有顶层函数声明（命名空间隔离：始终以 module::func 注册）
                let mut file_loaded = 0usize;
                for stmt in &prog.statements {
                    if let Stmt::Fn {
                        name,
                        params,
                        return_type,
                        body,
                        effect,
                        capability,
                        ..
                    } = stmt
                    {
                        let fn_val = FnValue {
                            name: name.clone(),
                            params: params.clone(),
                            body: body.as_ref().clone(),
                            closure: Environment::new(),
                            return_type: return_type.clone(),
                            effect: effect.clone(),
                            capability: capability.clone(),
                        };
                        // 命名空间隔离：以 module::func 注册（无冲突）
                        self.functions
                            .insert(format!("{}::{}", module, name), fn_val);
                        // 裸名归属追踪（确定性首选 = 首个定义该名的模块，保持向后兼容）
                        match bare_owner.get(name) {
                            Some(prev) if prev != &module => {
                                // 同名冲突：保留首个定义模块作为默认裸名目标，不做更改
                            }
                            Some(_) => {}
                            None => {
                                bare_owner.insert(name.clone(), module.clone());
                            }
                        }
                        file_loaded += 1;
                    }
                }
                loaded += file_loaded;

                // 注册 stdlib 内 struct 定义，使 `module::fn` 返回的 struct 字面量可构造
                // （主程序通过 eval_stmt(Stmt::StructDef) 注册，stdlib 加载路径此前遗漏此分支）
                for stmt in &prog.statements {
                    if let Stmt::StructDef { name, fields, .. } = stmt {
                        self.structs.insert(
                            name.clone(),
                            fields.iter().map(|f| f.name.clone()).collect(),
                        );
                    }
                }
            }
        }
        // 仅保留唯一归属的裸名别名（冲突名必须显式 module::func）
        self.bare_aliases = bare_owner;

        if !failed_files.is_empty() {
            eprintln!(
                "[stdlib] {} files failed to load: {:?}",
                failed_files.len(),
                failed_files
            );
        }
        Ok(loaded)
    }
}

/// Convenience entry: return all top-level values
pub fn run_source(source: &str) -> Result<Vec<Value>, RuntimeError> {
    let mut lex = dalin_compiler::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let (prog, errs) = parser.parse().map_err(|e| RuntimeError(e.to_string()))?;
    // 错误恢复模式下收集的语法错误必须上报，不能静默放行
    if !errs.is_empty() {
        let detail = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(RuntimeError(format!("parse errors: {detail}")));
    }
    let mut interp = Interpreter::new();
    interp.interpret(&prog)
}

/// 程序入口：执行顶层语句后，若定义了零参 `main` 则自动调用它。
///
/// 与 `run_source` 的分工：
/// - `run_source` = 求值一段代码（REPL / bridge / bench 用，**不**隐式调 main）
/// - `run_program` = 把源码当一个程序跑（`dalib run` 用，遵循 main 入口约定）
///
/// 未定义 `main` 时行为与 `run_source` 完全一致，因此对纯顶层脚本向后兼容。
pub fn run_program(source: &str) -> Result<Vec<Value>, RuntimeError> {
    let mut lex = dalin_compiler::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let (prog, errs) = parser.parse().map_err(|e| RuntimeError(e.to_string()))?;
    if !errs.is_empty() {
        let detail = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(RuntimeError(format!("parse errors: {detail}")));
    }

    let mut interp = Interpreter::new();
    let mut results = interp.interpret(&prog)?;

    // main 入口约定：仅接受零参 main，避免与同名多参工具函数混淆
    if let Some(main_fn) = interp.functions.get("main").cloned()
        && main_fn.params.is_empty()
    {
        let ret = interp.call_function(&main_fn, &[])?;
        results.push(ret);
    }

    Ok(results)
}

/// Convenience entry: after execution, return the task tree view (a registry miniature of nested spawns).
/// Used by the `--tree` demo to show how `spawn_task` derives subtasks with parent pointers.
pub fn run_source_with_tree(source: &str) -> Result<String, RuntimeError> {
    let mut lex = dalin_compiler::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let (prog, errs) = parser.parse().map_err(|e| RuntimeError(e.to_string()))?;
    if !errs.is_empty() {
        let detail = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(RuntimeError(format!("parse errors: {detail}")));
    }
    let mut interp = Interpreter::new();
    interp.interpret(&prog)?;
    Ok(interp.describe_task_tree())
}

/// C FFI external function call entry
pub fn call_ffi(func_name: &str, _args: &[Value]) -> Result<Value, RuntimeError> {
    // Stub: 在当前阶段不支持真实 C 调用
    // Phase 2: 使用 libloading 或 cbindgen 对接
    Err(RuntimeError(format!(
        "C FFI not implemented for function: {func_name}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Vec<Value>, RuntimeError> {
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
        let (prog, _errs) = dalin_compiler::parser::Parser::new(toks)
            .parse()
            .map_err(|e| RuntimeError(e.to_string()))?;
        let mut interp = Interpreter::new();
        interp.interpret(&prog)
    }

    #[test]
    fn spawn_await_returns_result() {
        let src = r#"
            spawn fn w() @ spawn @ cpu {
                return 99
            }
            let r = await(w)
        "#;
        let results = run(src).expect("run ok");
        let last = results.last().cloned().unwrap_or(Value::None);
        match last {
            Value::Int(n) => assert_eq!(n, 99),
            other => panic!("expected Int(99), got {:?}", other),
        }
    }

    #[test]
    fn spawn_channel_delivers_value() {
        let src = r#"
            channel tx rx
            spawn fn worker() @ spawn @ cpu {
                send(tx, 7)
                return 0
            }
            let v = recv(rx)
        "#;
        let results = run(src).expect("run ok");
        let last = results.last().cloned().unwrap_or(Value::None);
        match last {
            Value::Int(n) => assert_eq!(n, 7),
            other => panic!("expected Int(7), got {:?}", other),
        }
    }

    #[test]
    fn spawn_requires_spawn_effect() {
        let src = r#"
            spawn fn bad() @ pure @ cpu {
                return 1
            }
        "#;
        assert!(run(src).is_err());
    }

    #[test]
    fn spawn_task_passes_args_and_nests() {
        let src = r#"
            fn leaf(a, b) @ spawn @ cpu {
                return a + b
            }
            spawn fn root_worker() @ spawn @ cpu {
                let child = spawn_task("leaf", 10, 20)
                let r = await(child)
                return r
            }
            let rt = await(root_worker)
        "#;
        let results = run(src).expect("run ok");
        let last = results.last().cloned().unwrap_or(Value::None);
        match last {
            Value::Int(n) => assert_eq!(n, 30),
            other => panic!("expected Int(30), got {:?}", other),
        }
    }

    #[test]
    fn spawn_task_requires_spawn_effect() {
        let src = r#"
            fn pure_fn() @ pure @ cpu {
                return 1
            }
            let _ = spawn_task("pure_fn")
        "#;
        assert!(run(src).is_err());
    }
}
