/// Dalin L — 树遍历解释器
use dalin_compiler::ast::*;
use crate::env::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Return 哨兵常量 — 用于表示函数返回的控制流信号
const RETURN_SENTINEL: &str = "\x00__dl_return__\x00";

#[derive(Debug)]
pub struct RuntimeError(pub String);

/// 任务树节点（持久化，存于跨线程共享注册表，供控制面视图）。
struct TaskNode {
    name: String,
    parent: Option<String>,
}

/// 全局任务序号，保证每次 spawn 获得唯一 id。
static TASK_SEQ: AtomicUsize = AtomicUsize::new(0);

fn next_task_id(name: &str) -> String {
    let seq = TASK_SEQ.fetch_add(1, Ordering::SeqCst);
    format!("{}_{}", name, seq)
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
    pub return_value: Option<Value>,
    // ── 并发原语运行时（跨线程共享注册表，本地模拟控制面任务树）──
    // 任务树：id -> 节点（含 parent 指针），持久保留供视图/调度用。
    task_tree: Arc<Mutex<HashMap<String, TaskNode>>>,
    // 任务结果通道：id -> Receiver，await 时取出消费（瞬态）。
    task_results: Arc<Mutex<HashMap<String, mpsc::Receiver<Value>>>>,
    // 通道接收端表：名称 -> Receiver（发送端随 Value 跨线程共享）
    channel_registry: Arc<Mutex<HashMap<String, Arc<Mutex<mpsc::Receiver<Value>>>>>>,
    // 当前任务 id（worker 线程内用于把子任务挂到正确父节点）
    current_task_id: Option<String>,
    // ── 步数限制 ──
    step_count: u64,
    max_steps: u64,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            global_env: Environment::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
            return_value: None,
            task_tree: Arc::new(Mutex::new(HashMap::new())),
            task_results: Arc::new(Mutex::new(HashMap::new())),
            channel_registry: Arc::new(Mutex::new(HashMap::new())),
            current_task_id: None,
            step_count: 0,
            max_steps: 1_000_000,
        };
        interp.install_builtins();
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
            Stmt::Fn { name, params, return_type, body, effect, capability, .. } => {
                self.eval_fn_decl(name, params, return_type, body, effect, capability, env)
            }
            Stmt::Return(v) => {
                let val = match v {
                    Some(e) => self.eval_expr(e, env)?,
                    None => Value::None,
                };
                self.return_value = Some(val);
                Err(RuntimeError(RETURN_SENTINEL.into()))
            }
            Stmt::If { condition, then_body, else_body } => self.eval_if(condition, then_body, else_body, env),
            Stmt::While { condition, body } => self.eval_while(condition, body, env),
            Stmt::For { target, iterable, body } => self.eval_for(target, iterable, body, env),
            Stmt::Match { target, arms } => self.eval_match(target, arms, env),
            Stmt::StructDef { name, fields, .. } => {
                self.structs.insert(name.clone(), fields.iter().map(|f| f.name.clone()).collect());
                Ok(Value::None)
            }
            Stmt::EnumDef { name, variants, .. } => {
                self.enums.insert(name.clone(), variants.iter().map(|v| v.name.clone()).collect());
                Ok(Value::None)
            }
            Stmt::Spawn { fn_decl } => {
                // fn_decl 是 Stmt::Fn；spawn 要求效应标注为 spawn（效应格顶层，运行时强制）。
                if let Stmt::Fn { name, params, return_type, body, effect, capability, .. } = fn_decl.as_ref() {
                    if effect.as_deref() != Some("spawn") {
                        return Err(RuntimeError(format!(
                            "spawn 要求被派生的函数标注 @ spawn（{} 未标注效应）",
                            name
                        )));
                    }
                    if !params.is_empty() {
                        // 暂不支持参数传递（spawn fn `f(x)` 语法糖，用 `spawn_task` 传参即可）
                    }
                    let fnv = FnValue {
                        name: name.clone(),
                        params: params.to_vec(),
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
                        tree.insert(task_id.clone(), TaskNode { name: name.clone(), parent: self.current_task_id.clone() });
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
            Stmt::Channel { send_name, recv_name, .. } => {
                let (tx, rx) = mpsc::channel();
                env.define(send_name, Value::ChannelSender(Arc::new(tx)));
                // 接收端 Receiver 存共享注册表，Value 仅持有名称（保持 Value: Send）。
                self.channel_registry.lock().unwrap().insert(recv_name.clone(), Arc::new(Mutex::new(rx)));
                env.define(recv_name, Value::ChannelReceiver(recv_name.clone()));
                Ok(Value::None)
            }
            Stmt::Assert { condition, message } => {
                let cond = self.eval_expr(condition, env)?;
                if !self.truthy(&cond) {
                    let msg = message.as_ref()
                        .map(|m| self.eval_expr(m, env).map(|v| format!("{}", v)).unwrap_or_default())
                        .unwrap_or_default();
                    return Err(RuntimeError(format!("Assertion failed: {}", msg)));
                }
                Ok(Value::None)
            }
            Stmt::Expr(e) => self.eval_expr(e, env),
            _ => Ok(Value::None),
        }
    }

    fn eval_let(&mut self, name: &str, value: Option<&Expr>, env: &mut Environment) -> Result<Value, RuntimeError> {
        let val = match value {
            Some(v) => self.eval_expr(v, env)?,
            None => Value::None,
        };
        env.define(name, val.clone());
        Ok(val)
    }

    fn eval_fn_decl(&mut self, name: &str, params: &[FnParam], return_type: &Option<TypeRef>, body: &[Stmt], effect: &Option<String>, capability: &Option<String>, env: &mut Environment) -> Result<Value, RuntimeError> {
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

    fn eval_if(&mut self, condition: &Expr, then_body: &[Stmt], else_body: &[Stmt], env: &mut Environment) -> Result<Value, RuntimeError> {
        let cond = self.eval_expr(condition, env)?;
        if self.truthy(&cond) {
            self.eval_block(then_body, env)
        } else {
            self.eval_block(else_body, env)
        }
    }

    fn eval_while(&mut self, condition: &Expr, body: &[Stmt], env: &mut Environment) -> Result<Value, RuntimeError> {
        loop {
            self.step_count += 1;
            if self.step_count >= self.max_steps {
                return Err(RuntimeError("Step budget exceeded".to_string()));
            }
            let cond_val = self.eval_expr(condition, env)?;
            if !self.truthy(&cond_val) { break; }
            let result = self.eval_block(body, &mut env.child());
            match result {
                Err(RuntimeError(ref msg)) if msg == RETURN_SENTINEL => return result,
                Err(_) | Ok(_) => {}
            }
        }
        Ok(Value::None)
    }

    fn eval_for(&mut self, target: &str, iterable: &Expr, body: &[Stmt], env: &mut Environment) -> Result<Value, RuntimeError> {
        let iter = self.eval_expr(iterable, env)?;
        let items = self.as_iterable(&iter);
        let mut result = Value::None;
        for item in items {
            self.step_count += 1;
            if self.step_count >= self.max_steps {
                return Err(RuntimeError("Step budget exceeded".to_string()));
            }
            let mut child_env = env.child();
            child_env.define(target, item.clone());
            result = self.eval_block(body, &mut child_env)?;
        }
        Ok(result)
    }

    fn eval_match(&mut self, target: &Expr, arms: &[MatchArm], env: &mut Environment) -> Result<Value, RuntimeError> {
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
            Expr::ResultValue { is_ok, value, error } => {
                if *is_ok {
                    if let Some(v) = value {
                        Ok(Value::Result(true, Some(Box::new(self.eval_expr(v, env)?)), None))
                    } else {
                        Ok(Value::Result(true, None, None))
                    }
                } else if let Some(e) = error {
                    Ok(Value::Result(false, None, Some(Box::new(self.eval_expr(e, env)?))))
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
        Err(RuntimeError(format!("Undefined variable: '{}'", name)))
    }

    fn eval_binary(&mut self, left: &Expr, op: &str, right: &Expr, env: &mut Environment) -> Result<Value, RuntimeError> {
        // Assignment
        if op == "=" {
            let right_val = self.eval_expr(right, env)?;
            match left {
                Expr::Ident(name) => {
                    if !env.assign(name, right_val.clone()) {
                        return Err(RuntimeError(format!("Cannot assign to undefined variable: '{}'", name)));
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

        let left_val = self.eval_expr(left, env)?;
        let right_val = self.eval_expr(right, env)?;

        match op {
            "+" => {
                match (&left_val, &right_val) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
                    (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                    (Value::String(a), b) => Ok(Value::String(format!("{}{}", a, b))),
                    (a, Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                    _ => Err(RuntimeError(format!("Cannot add {:?} and {:?}", left_val, right_val))),
                }
            }
            "-" => {
                match (&left_val, &right_val) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
                    _ => Err(RuntimeError(format!("Cannot subtract {:?} and {:?}", left_val, right_val))),
                }
            }
            "*" => {
                match (&left_val, &right_val) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
                    _ => Err(RuntimeError(format!("Cannot multiply {:?} and {:?}", left_val, right_val))),
                }
            }
            "/" => {
                match (&left_val, &right_val) {
                    (Value::Int(a), Value::Int(b)) => {
                        if *b == 0 {
                            Err(RuntimeError("division by zero".into()))
                        } else if *a == i64::MIN && *b == -1 {
                            Err(RuntimeError("integer overflow in division".into()))
                        } else {
                            Ok(Value::Int(a / b))
                        }
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if *b == 0.0 {
                            Err(RuntimeError("division by zero".into()))
                        } else {
                            Ok(Value::Float(a / b))
                        }
                    }
                    _ => Err(RuntimeError(format!("Cannot divide {:?} and {:?}", left_val, right_val))),
                }
            }
            "%" => {
                match (&left_val, &right_val) {
                    (Value::Int(a), Value::Int(b)) => {
                        if *b == 0 {
                            Err(RuntimeError("modulo by zero".into()))
                        } else if *b == -1 {
                            Err(RuntimeError("integer overflow in modulo".into()))
                        } else {
                            Ok(Value::Int(a % b))
                        }
                    }
                    _ => Err(RuntimeError(format!("Cannot modulo {:?} and {:?}", left_val, right_val))),
                }
            }
            "==" => Ok(Value::Bool(self.values_equal(&left_val, &right_val))),
            "!=" => Ok(Value::Bool(!self.values_equal(&left_val, &right_val))),
            "<" | ">" | "<=" | ">=" => self.compare(&left_val, &right_val, op),
            "&&" => Ok(Value::Bool(self.truthy(&left_val) && self.truthy(&right_val))),
            "||" => Ok(Value::Bool(self.truthy(&left_val) || self.truthy(&right_val))),
            _ => Err(RuntimeError(format!("Unknown operator: {}", op))),
        }
    }

    fn eval_unary(&mut self, op: &str, operand: &Expr, env: &mut Environment) -> Result<Value, RuntimeError> {
        let val = self.eval_expr(operand, env)?;
        match op {
            "-" => match val {
                Value::Int(v) => Ok(Value::Int(-v)),
                Value::Float(v) => Ok(Value::Float(-v)),
                _ => Err(RuntimeError(format!("Cannot negate {:?}", val))),
            },
            "!" => Ok(Value::Bool(!self.truthy(&val))),
            _ => Err(RuntimeError(format!("Unknown unary op: {}", op))),
        }
    }

    fn eval_call(&mut self, func: &Expr, args: &[Expr], env: &mut Environment) -> Result<Value, RuntimeError> {
        let callee_name = match func {
            Expr::Ident(name) => name.clone(),
            Expr::MemberAccess { member, .. } => member.clone(),
            _ => return Err(RuntimeError("Invalid call expression".into())),
        };

        // Evaluate all arguments upfront to avoid borrow conflicts
        let mut arg_vals = Vec::new();
        for a in args {
            arg_vals.push(self.eval_expr(a, env)?);
        }

        // Builtins
        let builtins: [&str; 16] = ["println", "println!", "print", "print!", "len", "push", "assert", "int", "float", "str", "abs", "range", "await", "send", "recv", "spawn_task"];
        if builtins.contains(&callee_name.as_str()) {
            return self.call_builtin(&callee_name, &arg_vals);
        }

        // Struct constructor
        if let Some(fields) = self.structs.get(&callee_name).cloned() {
            let mut map = HashMap::new();
            map.insert(DALIN_TYPE_KEY.to_string(), Value::String(callee_name.clone()));
            for (fname, fval) in fields.iter().zip(arg_vals) {
                map.insert(fname.clone(), fval);
            }
            return Ok(Value::Struct(map));
        }

        // User function
        // 先查函数表（支持递归），再查环境
        if let Some(fnv) = self.functions.get(&callee_name).cloned() {
            return self.call_function(&fnv, &arg_vals);
        }
        match env.lookup(&callee_name) {
            Some(Value::Function(fnv)) => {
                self.call_function(&fnv, &arg_vals)
            }
            Some(_) => Err(RuntimeError(format!("'{}' is not callable", callee_name))),
            None => Err(RuntimeError(format!("Undefined function: '{}'", callee_name))),
        }
    }

    fn call_function(&mut self, fnv: &FnValue, args: &[Value]) -> Result<Value, RuntimeError> {
        if args.len() != fnv.params.len() {
            return Err(RuntimeError(format!(
                "Function '{}' expects {} args, got {}", fnv.name, fnv.params.len(), args.len()
            )));
        }
        let mut call_env = fnv.closure.child();
        for (param, arg) in fnv.params.iter().zip(args.iter()) {
            call_env.define(&param.name, arg.clone());
        }
        self.return_value = None;
        let result = self.eval_block(&fnv.body, &mut call_env);
        match result {
            Err(RuntimeError(ref msg)) if msg == RETURN_SENTINEL => {
                Ok(self.return_value.take().unwrap_or(Value::None))
            }
            Ok(_) => Ok(Value::None),
            Err(e) => Err(e),
        }
    }

    fn call_builtin(&mut self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        match name {
            "println" | "println!" => {
                let s: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                println!("{}", s.join(" "));
                Ok(Value::None)
            }
            "print" | "print!" => {
                let s: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                print!("{}", s.join(" "));
                Ok(Value::None)
            }
            "len" => {
                match &args[0] {
                    Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                    Value::String(s) => Ok(Value::Int(s.len() as i64)),
                    _ => Ok(Value::Int(0)),
                }
            }
            "push" => {
                if let Value::Array(ref mut arr) = args[0].clone() {
                    let mut arr = arr.clone();
                    arr.push(args[1].clone());
                    Ok(Value::Array(arr))
                } else {
                    Err(RuntimeError("push requires array".into()))
                }
            }
            "int" => {
                match &args[0] {
                    Value::String(s) => s.parse::<i64>().map(Value::Int).or(Ok(Value::Int(0))),
                    Value::Float(f) => Ok(Value::Int(*f as i64)),
                    Value::Int(i) => Ok(Value::Int(*i)),
                    _ => Ok(Value::Int(0)),
                }
            }
            "float" => {
                match &args[0] {
                    Value::String(s) => s.parse::<f64>().map(Value::Float).or(Ok(Value::Float(0.0))),
                    Value::Int(i) => Ok(Value::Float(*i as f64)),
                    Value::Float(f) => Ok(Value::Float(*f)),
                    _ => Ok(Value::Float(0.0)),
                }
            }
            "str" => Ok(Value::String(format!("{}", args[0]))),
            "abs" => {
                match args[0] {
                    Value::Int(i) => Ok(Value::Int(i.abs())),
                    Value::Float(f) => Ok(Value::Float(f.abs())),
                    _ => Err(RuntimeError("abs requires number".into())),
                }
            }
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
                        None => Err(RuntimeError(format!("未知 task: {}", id))),
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
                        Ok(_) => Ok(Value::None),
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
                        None => Err(RuntimeError(format!("未知 channel: {}", name))),
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
                    _ => return Err(RuntimeError("spawn_task 第一个参数必须是函数名（字符串）".into())),
                };
                let fnv = match self.functions.get(&fname).cloned() {
                    Some(f) => f,
                    None => return Err(RuntimeError(format!("spawn_task: 未定义函数 {}", fname))),
                };
                if fnv.effect.as_deref() != Some("spawn") {
                    return Err(RuntimeError(format!(
                        "spawn_task: {} 必须标注 @ spawn 才能被派生",
                        fname
                    )));
                }
                let call_args: Vec<Value> = args[1..].to_vec();
                let child_id = next_task_id(&fname);
                let (tx, rx) = mpsc::channel();
                {
                    let mut tree = self.task_tree.lock().unwrap();
                    tree.insert(child_id.clone(), TaskNode { name: fname.clone(), parent: self.current_task_id.clone() });
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
            _ => Err(RuntimeError(format!("Unknown builtin: {}", name))),
        }
    }

    fn eval_member_access(&mut self, object: &Expr, member: &str, env: &mut Environment) -> Result<Value, RuntimeError> {
        let obj = self.eval_expr(object, env)?;
        match obj {
            Value::Struct(ref map) => {
                if let Some(v) = map.get(member) {
                    Ok(v.clone())
                } else {
                    let ty = map.get(DALIN_TYPE_KEY).map(|v| format!("{}", v)).unwrap_or_default();
                    Err(RuntimeError(format!("Struct '{}' has no field '{}'", ty, member)))
                }
            }
            _ => Err(RuntimeError(format!("Cannot access member '{}'", member))),
        }
    }

    fn eval_index(&mut self, array: &Expr, index: &Expr, env: &mut Environment) -> Result<Value, RuntimeError> {
        let arr = self.eval_expr(array, env)?;
        let idx = self.eval_expr(index, env)?;
        match (&arr, &idx) {
            (Value::Array(a), Value::Int(i)) => {
                let i = *i as usize;
                if i < a.len() {
                    Ok(a[i].clone())
                } else {
                    Err(RuntimeError(format!("Index out of range: {}", i)))
                }
            }
            _ => Err(RuntimeError("Invalid index operation".into())),
        }
    }

    fn eval_pipe(&mut self, input: &Expr, ops: &[(String, Expr)], env: &mut Environment) -> Result<Value, RuntimeError> {
        let mut current = self.eval_expr(input, env)?;
        for (name, _) in ops {
            match env.lookup(name) {
                Some(Value::Function(fnv)) => {
                    current = self.call_function(&fnv, &[current])?;
                }
                _ => return Err(RuntimeError(format!("Pipe target '{}' is not callable", name))),
            }
        }
        Ok(current)
    }

    fn eval_range(&mut self, start: &Expr, end: &Expr, env: &mut Environment) -> Result<Value, RuntimeError> {
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
        let items: Result<Vec<Value>, RuntimeError> = elems.iter().map(|e| self.eval_expr(e, env)).collect();
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
                            && let Some(ref binding) = pat.binding {
                                env.define(binding, *v.clone());
                                return true;
                            }
                        false
                    }
                    "None" => matches!(value, Value::Option(false, _)),
                    "Ok" => {
                        if let Value::Result(true, Some(v), _) = value
                            && let Some(ref binding) = pat.binding {
                                env.define(binding, *v.clone());
                                return true;
                            }
                        false
                    }
                    "Err" => {
                        if let Value::Result(false, _, Some(e)) = value
                            && let Some(ref binding) = pat.binding {
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
            (Value::String(as_), Value::String(bs)) => as_ == bs,
            (Value::Bool(ab), Value::Bool(bb)) => ab == bb,
            (Value::Char(ac), Value::Char(bc)) => ac == bc,
            _ => false,
        }
    }

    fn compare(&self, a: &Value, b: &Value, op: &str) -> Result<Value, RuntimeError> {
        let cmp = match (a, b) {
            (Value::Int(ai), Value::Int(bi)) => Some(ai.cmp(bi)),
            (Value::Float(af), Value::Float(bf)) => Some(af.partial_cmp(bf).unwrap_or(std::cmp::Ordering::Equal)),
            (Value::Int(ai), Value::Float(bf)) => Some((*ai as f64).partial_cmp(bf).unwrap_or(std::cmp::Ordering::Equal)),
            (Value::Float(af), Value::Int(bi)) => Some(af.partial_cmp(&(*bi as f64)).unwrap_or(std::cmp::Ordering::Equal)),
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
            None => Err(RuntimeError(format!("Cannot compare {:?} and {:?}", a, b))),
        }
    }

    fn as_iterable(&self, value: &Value) -> Vec<Value> {
        match value {
            Value::Array(a) => a.clone(),
            Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
            _ => vec![value.clone()],
        }
    }

    /// 返回任务树的文本视图（控制面注册表的本地缩影）。
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
}

/// 便捷入口：返回所有顶层值
pub fn run_source(source: &str) -> Result<Vec<Value>, RuntimeError> {
    let mut lex = dalin_compiler::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let prog = parser.parse().map_err(|e| RuntimeError(e.to_string()))?;
    let mut interp = Interpreter::new();
    interp.interpret(&prog)
}

/// 便捷入口：执行后返回任务树视图（嵌套 spawn 的注册表缩影）。
/// 用于 `--tree` demo，展示 spawn_task 如何派生出带 parent 指针的子任务。
pub fn run_source_with_tree(source: &str) -> Result<String, RuntimeError> {
    let mut lex = dalin_compiler::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let prog = parser.parse().map_err(|e| RuntimeError(e.to_string()))?;
    let mut interp = Interpreter::new();
    interp.interpret(&prog)?;
    Ok(interp.describe_task_tree())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Vec<Value>, RuntimeError> {
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
        let prog = dalin_compiler::parser::Parser::new(toks).parse().map_err(|e| RuntimeError(e.to_string()))?;
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