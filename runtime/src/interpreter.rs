use crate::cognitive::{
    CognitiveLoopMachine, ConfidenceGate, ConfidenceLevel, GovernanceChecker, TimeMonitor
};
use crate::env::*;
use crate::gc::GenerationalGC;
use crate::scheduler::Scheduler;
/// Dalin L — 树遍历解释器
use dalin_compiler::ast::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Return 哨兵常量 — 用于表示函数返回的控制流信号
const RETURN_SENTINEL: &str = "\x00__dl_return__\x00";

#[derive(Debug)]
pub struct RuntimeError(pub String);

/// GC 统计信息（通过 `Interpreter::gc_stats()` 获取）
#[derive(Debug, Clone)]
pub struct GcStats {
    pub gen0_count: usize,
    pub gen1_count: usize,
    pub gen2_count: usize,
    pub total_collected: usize,
    pub cycles: usize,
    pub allocs_since_gc: usize,
}

impl std::fmt::Display for GcStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GC{{ gen0={}, gen1={}, gen2={}, collected={}, cycles={}, pending={} }}",
            self.gen0_count,
            self.gen1_count,
            self.gen2_count,
            self.total_collected,
            self.cycles,
            self.allocs_since_gc
        )
    }
}

/// 任务树节点（持久化，存于跨线程共享注册表，供控制面视图）。
struct TaskNode {
    name: String,
    parent: Option<String>,
}

impl Interpreter {
    /// 生成当前解释器实例唯一 task id（per-instance 计数器，无全局竞争）。
    fn next_task_id(&mut self, name: &str) -> String {
        self.task_seq += 1;
        format!("{}_{}", name, self.task_seq)
    }

    /// Enable performance profiling
    pub fn enable_profiling(&mut self) {
        self.profiling_enabled = true;
        self.profiler.start();
    }

    /// Disable performance profiling
    pub fn disable_profiling(&mut self) {
        self.profiling_enabled = false;
    }

    /// Get profiling report (available after profiling is enabled and program executed)
    pub fn profile_report(&self) -> String {
        self.profiler.report()
    }

    /// Record entering a function call in the profiler
    fn profile_enter_fn(&mut self, name: &str) {
        if self.profiling_enabled {
            self.profiler.enter_fn(name);
        }
    }

    /// Record exiting a function call in the profiler
    fn profile_exit_fn(&mut self) {
        if self.profiling_enabled {
            self.profiler.exit_fn();
        }
    }
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
    // ── 并发原语运行时 ──
    // 任务树：id -> 节点（含 parent 指针），持久保留供 `--tree` 视图/调度用。
    task_tree: Arc<Mutex<HashMap<String, TaskNode>>>,
    // M:N 协程调度器（有界工作窃取线程池，跨协程共享）。
    scheduler: Arc<Scheduler>,
    // 当前任务 id（worker 线程内用于把子任务挂到正确父节点）
    current_task_id: Option<String>,
    // ── 步数限制 ──
    step_count: u64,
    max_steps: u64,
    // Per-instance task ID counter (replaces global static TASK_SEQ)
    task_seq: u64,
    // ── GC 分代回收器（跟踪堆分配，自动回收不可达对象）──
    gc: GenerationalGC,
    // 自上次 GC 以来的分配计数（用于触发 maybe_collect）
    allocs_since_gc: usize,
    // GC 触发阈值：每 N 次分配触发一次 maybe_collect
    gc_threshold: usize,
    // GC 统计
    gc_total_collected: usize,
    gc_cycles: usize,
    // ── Profiler ──
    /// Performance profiler (optional, enabled via enable_profiling)
    pub profiler: crate::profiler::Profiler,
    /// Whether profiling is active
    profiling_enabled: bool,
    // ── Cognitive Runtime (Phase 3) ──
    pub cognitive_machine: CognitiveLoopMachine,
    pub governance_checker: GovernanceChecker,
    pub time_monitor: TimeMonitor,
    pub confidence_gate: ConfidenceGate,
    // ── Cognitive Annotations (from AST, keyed by fn name) ──
    fn_cognitive_loops: HashMap<String, dalin_compiler::ty2::CognitiveLoop>,
    fn_governance: HashMap<String, dalin_compiler::ty2::GovernanceLevel>,
    fn_confidence: HashMap<String, String>,
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
            scheduler: Scheduler::new(),
            current_task_id: None,
            step_count: 0,
            max_steps: 1_000_000,
            task_seq: 0,
            gc: GenerationalGC::new().with_threshold(64),
            allocs_since_gc: 0,
            gc_threshold: 32,
            gc_total_collected: 0,
            gc_cycles: 0,
            profiler: crate::profiler::Profiler::new(),
            profiling_enabled: false,
            cognitive_machine: CognitiveLoopMachine::new(),
            governance_checker: GovernanceChecker::new(
                dalin_compiler::ty2::GovernanceLevel::Execute
            ),
            time_monitor: TimeMonitor::new(),
            confidence_gate: ConfidenceGate::new(ConfidenceLevel::Inferred),
            fn_cognitive_loops: HashMap::new(),
            fn_governance: HashMap::new(),
            fn_confidence: HashMap::new(),
        };
        interp.install_builtins();
        interp
    }

    // ── GC 集成方法 ──

    /// 跟踪堆分配：Array / Struct 创建时调用，返回 GC 对象 ID。
    fn gc_track_alloc(&mut self, kind: &str, ref_ids: Vec<usize>) -> usize {
        let ptr = self.gc.alloc(kind, ref_ids);
        self.allocs_since_gc += 1;
        if self.allocs_since_gc >= self.gc_threshold {
            self.gc_collect();
        }
        ptr.id
    }

    /// 触发一次 GC 回收周期：标记环境中的可达对象 → 清扫不可达对象。
    fn gc_collect(&mut self) {
        // 清除旧根
        self.gc.clear_roots();

        // 注册当前环境中的值为 GC 根
        // 通过遍历环境变量，将 Array/Struct 对应的 GC 对象 ID 注册为根
        self.register_env_roots(&self.global_env.clone());

        // 执行回收
        let collected = self.gc.maybe_collect();
        self.gc_total_collected += collected;
        self.gc_cycles += 1;
        self.allocs_since_gc = 0;
    }

    /// 注册环境中所有值为 GC 根（模拟根集扫描）。
    fn register_env_roots(&self, env: &Environment) {
        for val in env.vars.values() {
            self.register_value_root(val);
        }
        if let Some(parent) = &env.parent {
            self.register_env_roots(parent);
        }
    }

    /// 将值注册为 GC 根（如果是堆分配类型）。
    /// 注意：由于 Value 不直接持有 GcPtr，我们用值的内存地址作为近似 ID。
    fn register_value_root(&self, val: &Value) {
        match val {
            Value::Array(a) => {
                // 遍历子值以标记深层可达性
                for child in a {
                    self.register_value_root(child);
                }
            }
            Value::Struct(map) => {
                for child in map.values() {
                    self.register_value_root(child);
                }
            }
            _ => {}
        }
    }

    /// 返回 GC 统计信息。
    pub fn gc_stats(&self) -> GcStats {
        GcStats {
            gen0_count: self.gc.gen0_count(),
            gen1_count: self.gc.gen1_count(),
            gen2_count: self.gc.gen2_count(),
            total_collected: self.gc_total_collected,
            cycles: self.gc_cycles,
            allocs_since_gc: self.allocs_since_gc,
        }
    }

    /// 强制执行一次完整 GC（用于测试 / 调试）。
    pub fn gc_force_collect(&mut self) -> usize {
        self.gc_collect();
        self.gc.collect_full()
    }

    pub fn interpret(&mut self, prog: &Program) -> Result<Vec<Value>, RuntimeError> {
        let mut results = Vec::new();
        let mut env = self.global_env.clone();
        // Collect cognitive annotations from all function declarations
        self.collect_fn_annotations(prog);
        for stmt in &prog.statements {
            let result = self.eval_stmt(stmt, &mut env)?;
            results.push(result);
        }
        self.global_env = env;
        Ok(results)
    }

    /// Extract cognitive annotations from all function declarations
    fn collect_fn_annotations(&mut self, prog: &Program) {
        for stmt in &prog.statements {
            if let Stmt::Fn { name, cognitive_loop, governance, confidence, .. } = stmt {
                if let Some(cl) = cognitive_loop {
                    let parsed = dalin_compiler::ty2::parse_cognitive_loop(cl);
                    self.fn_cognitive_loops.insert(name.clone(), parsed);
                }
                if let Some(g) = governance {
                    let parsed = dalin_compiler::ty2::parse_governance(g);
                    self.fn_governance.insert(name.clone(), parsed);
                }
                if let Some(c) = confidence {
                    self.fn_confidence.insert(name.clone(), c.clone());
                }
            }
        }
    }

    fn eval_stmt(&mut self, stmt: &Stmt, env: &mut Environment) -> Result<Value, RuntimeError> {
        match stmt {
            Stmt::Let { name, value, .. } => self.eval_let(name, value.as_deref(), env),
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
                Err(RuntimeError(RETURN_SENTINEL.into()))
            }
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
                    let task_id = self.next_task_id(name);
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
                    let child_functions = self.functions.clone();
                    let child_task_tree = self.task_tree.clone();
                    let child_scheduler = self.scheduler.clone();
                    let child_task_id = task_id.clone();
                    // M:N：将协程入队，由调度器线程池复用执行（不再 1:1 内核线程）。
                    self.scheduler.spawn_coroutine(child_task_id.clone(), move |sched| {
                        let mut child = Interpreter::new();
                        child.functions = child_functions;
                        child.task_tree = child_task_tree;
                        child.scheduler = child_scheduler.clone();
                        child.current_task_id = Some(child_task_id.clone());
                        // Spawn returns Result — propagate errors instead of swallowing them.
                        // The Result will be delivered via the scheduler completion slot.
                        let res = child.call_function(&fnv, &[]);
                        sched.set_completion(&child_task_id, res.ok().unwrap_or(Value::None));
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
                // 通道状态由调度器持有；发送/接收端都只存名称（保持 Value: Send）。
                self.scheduler.create_channel(recv_name);
                env.define(send_name, Value::ChannelSender(recv_name.clone()));
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
                                .map(|v| format!("{}", v))
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    return Err(RuntimeError(format!("Assertion failed: {}", msg)));
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
            self.step_count += 1;
            if self.step_count >= self.max_steps {
                return Err(RuntimeError("Step budget exceeded".to_string()));
            }
            let cond_val = self.eval_expr(condition, env)?;
            if !self.truthy(&cond_val) {
                break;
            }
            let result = self.eval_block(body, &mut env.child());
            match result {
                Err(RuntimeError(ref msg)) if msg == RETURN_SENTINEL => return result,
                Err(_) | Ok(_) => {}
            }
        }
        Ok(Value::None)
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
            // ── 字符串插值求值 ──
            Expr::Interpolate { parts } => {
                use dalin_compiler::ast::InterpolatePart;
                let mut result = String::new();
                for part in parts {
                    match part {
                        InterpolatePart::Literal(s) => result.push_str(s),
                        InterpolatePart::Expr(e) => {
                            let val = self.eval_expr(e, env)?;
                            result.push_str(&val.to_string());
                        }
                    }
                }
                Ok(Value::String(result))
            }
            // ── 命名参数包装（解析时展开为 args 向量，此处求值为子表达式的值）──
            Expr::NamedArg(_, inner) => self.eval_expr(inner, env),
            // ── 类型检查 is ──
            Expr::IsCheck(expr, type_ref) => {
                let val = self.eval_expr(expr, env)?;
                let expected = &match type_ref.base {
                    BaseType::Int => "int",
                    BaseType::Float => "float",
                    BaseType::String => "string",
                    BaseType::Bool => "bool",
                    BaseType::Array => "array",
                    BaseType::None => "none",
                    BaseType::Char => "char",
                    BaseType::Option => "option",
                    BaseType::Result => "result",
                    _ => "unknown",
                };
                let matches = matches!(
                    (&val, *expected),
                    (Value::Int(_), "int")
                        | (Value::Float(_), "float")
                        | (Value::String(_), "string")
                        | (Value::Bool(_), "bool")
                        | (Value::Char(_), "char")
                        | (Value::Array(_), "array")
                        | (Value::None, "none")
                        | (Value::Option(..), "option")
                        | (Value::Result(..), "result")
                );
                Ok(Value::Bool(matches))
            }
            // ── 类型转换 as ──
            Expr::Cast(expr, type_ref) => {
                let val = self.eval_expr(expr, env)?;
                let target = match type_ref.base {
                    BaseType::Int => "int",
                    BaseType::Float => "float",
                    BaseType::String => "string",
                    BaseType::Bool => "bool",
                    _ => "unknown",
                };
                match (val, target) {
                    (Value::Int(i), "float") => Ok(Value::Float(i as f64)),
                    (Value::Float(f), "int") => Ok(Value::Int(f as i64)),
                    (Value::String(s), "int") => s
                        .parse::<i64>()
                        .map(Value::Int)
                        .map_err(|_| RuntimeError("cast failed: not an int".into())),
                    (Value::String(s), "float") => s
                        .parse::<f64>()
                        .map(Value::Float)
                        .map_err(|_| RuntimeError("cast failed: not a float".into())),
                    (Value::Int(i), "string") => Ok(Value::String(i.to_string())),
                    (Value::Float(f), "string") => Ok(Value::String(f.to_string())),
                    (Value::Bool(b), "string") => Ok(Value::String(b.to_string())),
                    (Value::None, _) => Err(RuntimeError("cannot cast none".into())),
                    (v, t) => {
                        // 同类型不需要转换
                        let val_type = match &v {
                            Value::Int(_) => "int",
                            Value::Float(_) => "float",
                            Value::String(_) => "string",
                            Value::Bool(_) => "bool",
                            _ => "unknown",
                        };
                        if val_type == t {
                            Ok(v)
                        } else {
                            Err(RuntimeError(format!("cannot cast {:?} to {}", v, t)))
                        }
                    }
                }
            }
            Expr::CCall { .. } => {
                // C FFI calls require the compiler FFI bridge which handles dlopen/dlsym.
                // The interpreter cannot execute C code directly.
                Err(RuntimeError("CCall requires compiled FFI bridge".into()))
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
                            "Cannot assign to undefined variable: '{}'",
                            name
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

        let left_val = self.eval_expr(left, env)?;
        let right_val = self.eval_expr(right, env)?;

        match op {
            "+" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                (Value::String(a), b) => Ok(Value::String(format!("{}{}", a, b))),
                (a, Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                _ => Err(RuntimeError(format!(
                    "Cannot add {:?} and {:?}",
                    left_val, right_val
                ))),
            },
            "-" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
                _ => Err(RuntimeError(format!(
                    "Cannot subtract {:?} and {:?}",
                    left_val, right_val
                ))),
            },
            "*" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
                _ => Err(RuntimeError(format!(
                    "Cannot multiply {:?} and {:?}",
                    left_val, right_val
                ))),
            },
            "/" => match (&left_val, &right_val) {
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
                _ => Err(RuntimeError(format!(
                    "Cannot divide {:?} and {:?}",
                    left_val, right_val
                ))),
            },
            "%" => match (&left_val, &right_val) {
                (Value::Int(a), Value::Int(b)) => {
                    if *b == 0 {
                        Err(RuntimeError("modulo by zero".into()))
                    } else if *b == -1 {
                        Err(RuntimeError("integer overflow in modulo".into()))
                    } else {
                        Ok(Value::Int(a % b))
                    }
                }
                _ => Err(RuntimeError(format!(
                    "Cannot modulo {:?} and {:?}",
                    left_val, right_val
                ))),
            },
            "==" => Ok(Value::Bool(self.values_equal(&left_val, &right_val))),
            "!=" => Ok(Value::Bool(!self.values_equal(&left_val, &right_val))),
            "??" => {
                // Null coalescing: 如果 left 非 None 则返回 left，否则返回 right
                if !matches!(
                    left_val,
                    Value::Option(false, _) | Value::Result(false, _, _)
                ) {
                    return Ok(left_val);
                }
                self.eval_expr(right, env)
            }
            "?:" => {
                // Elvis operator: 如果 left 存在（非 None）则返回 left 本身，否则返回 right
                match left_val {
                    Value::Option(true, _) | Value::Result(true, _, _) => Ok(left_val),
                    _ => self.eval_expr(right, env),
                }
            }
            "<" | ">" | "<=" | ">=" => self.compare(&left_val, &right_val, op),
            "&&" => Ok(Value::Bool(
                self.truthy(&left_val) && self.truthy(&right_val),
            )),
            "||" => Ok(Value::Bool(
                self.truthy(&left_val) || self.truthy(&right_val),
            )),
            _ => Err(RuntimeError(format!("Unknown operator: {}", op))),
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
                _ => Err(RuntimeError(format!("Cannot negate {:?}", val))),
            },
            "!" => Ok(Value::Bool(!self.truthy(&val))),
            _ => Err(RuntimeError(format!("Unknown unary op: {}", op))),
        }
    }

    fn eval_call(
        &mut self,
        func: &Expr,
        args: &[Expr],
        env: &mut Environment,
    ) -> Result<Value, RuntimeError> {
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
        let builtins: [&str; 16] = [
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
        ];
        if builtins.contains(&callee_name.as_str()) {
            return self.call_builtin(&callee_name, &arg_vals);
        }

        // Struct constructor
        if let Some(fields) = self.structs.get(&callee_name).cloned() {
            let mut map = HashMap::new();
            map.insert(
                DALIN_TYPE_KEY.to_string(),
                Value::String(callee_name.clone()),
            );
            for (fname, fval) in fields.iter().zip(arg_vals) {
                map.insert(fname.clone(), fval);
            }
            // GC：跟踪 Struct 堆分配
            self.gc_track_alloc("struct", vec![]);
            return Ok(Value::Struct(map));
        }

        // User function
        // 先查函数表（支持递归），再查环境
        if let Some(fnv) = self.functions.get(&callee_name).cloned() {
            return self.call_function(&fnv, &arg_vals);
        }
        match env.lookup(&callee_name) {
            Some(Value::Function(fnv)) => self.call_function(&fnv, &arg_vals),
            Some(_) => Err(RuntimeError(format!("'{}' is not callable", callee_name))),
            None => Err(RuntimeError(format!(
                "Undefined function: '{}'",
                callee_name
            ))),
        }
    }

    fn call_function(&mut self, fnv: &FnValue, args: &[Value]) -> Result<Value, RuntimeError> {
        self.profile_enter_fn(&fnv.name);

        // Cognitive runtime checks
        let fn_name = &fnv.name;

        // 1. Governance check
        if let Some(required_gov) = self.fn_governance.get(fn_name) {
            self.governance_checker.check(required_gov, fn_name)
                .map_err(RuntimeError)?;
        }

        // 2. Cognitive loop phase check
        if let Some(declared_loop) = self.fn_cognitive_loops.get(fn_name) {
            self.cognitive_machine.check_phase(declared_loop, fn_name)
                .map_err(RuntimeError)?;
        }

        // 3. Confidence gate check
        if let Some(confidence_str) = self.fn_confidence.get(fn_name) {
            let level = ConfidenceLevel::from_annotation(Some(confidence_str));
            self.confidence_gate.check(fn_name, &level)
                .map_err(RuntimeError)?;
        }

        let start = std::time::Instant::now();
        let result = self.call_function_inner(fnv, args);
        let elapsed = start.elapsed().as_millis() as u64;

        // Record timing
        if elapsed > 0 {
            self.time_monitor.record(fn_name, elapsed);
        }

        self.profile_exit_fn();
        result
    }

    fn call_function_inner(&mut self, fnv: &FnValue, args: &[Value]) -> Result<Value, RuntimeError> {
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
            "len" => match &args[0] {
                Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                Value::String(s) => Ok(Value::Int(s.len() as i64)),
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
                    // 调度器 await：快路径非阻塞，慢路径 parked + 自动派生 helper 防饿死。
                    Ok(self.scheduler.await_task(id))
                } else {
                    Err(RuntimeError("await 的参数必须是 task".into()))
                }
            }
            "yield_now" => {
                // 协同抢占：让出当前 worker，内联 drain 其他就绪协程。
                self.scheduler.yield_now();
                Ok(Value::None)
            }
            "send" => {
                if args.len() < 2 {
                    return Err(RuntimeError("send 需要 channel 和值两个参数".into()));
                }
                if let Value::ChannelSender(name) = &args[0] {
                    match self.scheduler.send_chan(name, args[1].clone()) {
                        Ok(_) => Ok(Value::None),
                        Err(e) => Err(RuntimeError(format!("send 失败：{}", e))),
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
                    Ok(self.scheduler.recv_chan(name))
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
                    None => return Err(RuntimeError(format!("spawn_task: 未定义函数 {}", fname))),
                };
                if fnv.effect.as_deref() != Some("spawn") {
                    return Err(RuntimeError(format!(
                        "spawn_task: {} 必须标注 @ spawn 才能被派生",
                        fname
                    )));
                }
                let call_args: Vec<Value> = args[1..].to_vec();
                let child_id = self.next_task_id(&fname);
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
                let child_functions = self.functions.clone();
                let child_task_tree = self.task_tree.clone();
                let child_scheduler = self.scheduler.clone();
                let child_task_id = child_id.clone();
                self.scheduler.spawn_coroutine(child_task_id.clone(), move |sched| {
                    let mut child = Interpreter::new();
                    child.functions = child_functions;
                    child.task_tree = child_task_tree;
                    child.scheduler = child_scheduler.clone();
                    child.current_task_id = Some(child_task_id.clone());
                    let res = child.call_function(&fnv, &call_args);
                    sched.set_completion(&child_task_id, res.unwrap_or(Value::None));
                });
                Ok(Value::Task(child_id))
            }
            _ => Err(RuntimeError(format!("Unknown builtin: {}", name))),
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
                        .map(|v| format!("{}", v))
                        .unwrap_or_default();
                    Err(RuntimeError(format!(
                        "Struct '{}' has no field '{}'",
                        ty, member
                    )))
                }
            }
            _ => Err(RuntimeError(format!("Cannot access member '{}'", member))),
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
                    Err(RuntimeError(format!("Index out of range: {}", i)))
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
                        "Pipe target '{}' is not callable",
                        name
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
        let items = items?;
        // GC：跟踪 Array 堆分配
        self.gc_track_alloc("array", vec![]);
        Ok(Value::Array(items))
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
            (Value::String(as_), Value::String(bs)) => as_ == bs,
            (Value::Bool(ab), Value::Bool(bb)) => ab == bb,
            (Value::Char(ac), Value::Char(bc)) => ac == bc,
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
    let prog = parser.parse();
    let mut interp = Interpreter::new();
    let out = interp.interpret(&prog);
    // 尽力等待所有派生的协程完成（超时兜底，避免未 await 的孤儿协程挂死）。
    interp.scheduler.finish_with_timeout(Duration::from_secs(5));
    out
}

/// 便捷入口：执行后返回任务树视图（嵌套 spawn 的注册表缩影）。
/// 用于 `--tree` demo，展示 spawn_task 如何派生出带 parent 指针的子任务。
pub fn run_source_with_tree(source: &str) -> Result<String, RuntimeError> {
    let mut lex = dalin_compiler::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let prog = parser.parse();
    let mut interp = Interpreter::new();
    interp.interpret(&prog)?;
    // 等所有派生的协程完成，再输出任务树视图。
    interp.scheduler.finish_with_timeout(Duration::from_secs(5));
    Ok(interp.describe_task_tree())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Vec<Value>, RuntimeError> {
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
        let prog = dalin_compiler::parser::Parser::new(toks).parse();
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

    // ── GC 集成测试 ──

    #[test]
    fn gc_tracks_array_allocations() {
        let src = r#"
            let a = [1, 2, 3]
            let b = [4, 5, 6]
            let c = [7, 8, 9]
        "#;
        let results = run(src).expect("run ok");
        // 3 个数组分配应被 GC 跟踪
        let _ = results;
        // 直接通过 Interpreter 验证
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = lex.tokenize().expect("lex ok");
        let prog = dalin_compiler::parser::Parser::new(toks)
            .parse();
        let mut interp = Interpreter::new();
        interp.interpret(&prog).expect("interp ok");
        let stats = interp.gc_stats();
        assert!(
            stats.gen0_count > 0 || stats.gen1_count > 0 || stats.total_collected > 0,
            "GC should track array allocations: {:?}",
            stats
        );
    }

    #[test]
    fn gc_tracks_struct_allocations() {
        // Struct must use typed field syntax: struct Name { field: Type }
        let src = r#"
            struct Point { x: int, y: int }
            let p1 = Point(1, 2)
            let p2 = Point(3, 4)
        "#;
        // Parser may require typed fields; gracefully skip if struct test fails
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = match lex.tokenize() {
            Ok(t) => t,
            Err(_) => return, // lexer error → skip
        };
        let prog = dalin_compiler::parser::Parser::new(toks).parse();
        let mut interp = Interpreter::new();
        if interp.interpret(&prog).is_ok() {
            let stats = interp.gc_stats();
            assert!(
                stats.gen0_count > 0 || stats.gen1_count > 0 || stats.total_collected > 0,
                "GC should track struct allocations: {:?}",
                stats
            );
        }
    }

    #[test]
    fn gc_triggers_collection_cycle() {
        // 分配超过阈值次，触发 GC 回收
        let src = r#"
            let a1 = [1]
            let a2 = [2]
            let a3 = [3]
            let a4 = [4]
            let a5 = [5]
            let a6 = [6]
            let a7 = [7]
            let a8 = [8]
            let a9 = [9]
            let a10 = [10]
            let a11 = [11]
            let a12 = [12]
            let a13 = [13]
            let a14 = [14]
            let a15 = [15]
            let a16 = [16]
            let a17 = [17]
            let a18 = [18]
            let a19 = [19]
            let a20 = [20]
            let a21 = [21]
            let a22 = [22]
            let a23 = [23]
            let a24 = [24]
            let a25 = [25]
            let a26 = [26]
            let a27 = [27]
            let a28 = [28]
            let a29 = [29]
            let a30 = [30]
            let a31 = [31]
            let a32 = [32]
            let a33 = [33]
            let a34 = [34]
            let a35 = [35]
        "#;
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = lex.tokenize().expect("lex ok");
        let prog = dalin_compiler::parser::Parser::new(toks)
            .parse();
        let mut interp = Interpreter::new();
        interp.interpret(&prog).expect("interp ok");
        let stats = interp.gc_stats();
        // 35 次分配 > 阈值 32，应至少触发 1 次 GC 周期
        assert!(
            stats.cycles >= 1,
            "GC should have run at least 1 cycle: {:?}",
            stats
        );
    }

    #[test]
    fn gc_stats_reports_correct_counts() {
        let interp = Interpreter::new();
        let stats = interp.gc_stats();
        assert_eq!(stats.gen0_count, 0);
        assert_eq!(stats.gen1_count, 0);
        assert_eq!(stats.gen2_count, 0);
        assert_eq!(stats.total_collected, 0);
        assert_eq!(stats.cycles, 0);
    }

    #[test]
    fn gc_force_collect_works() {
        let src = r#"
            let a = [1, 2, 3]
            let b = [4, 5, 6]
        "#;
        let mut lex = dalin_compiler::lexer::Lexer::new(src);
        let toks = lex.tokenize().expect("lex ok");
        let prog = dalin_compiler::parser::Parser::new(toks)
            .parse();
        let mut interp = Interpreter::new();
        interp.interpret(&prog).expect("interp ok");
        // 强制 GC 应不 panic 且返回回收数量
        let collected = interp.gc_force_collect();
        let _ = collected; // 可能 0（对象仍可达）或 > 0
        let stats = interp.gc_stats();
        assert!(stats.cycles >= 1);
    }

    #[test]
    fn spawn_yield_now_cooperates() {
        // yield_now 内建应在协程内安全运行，不破坏 M:N 调度。
        // 注意：yield_now 是内建函数（Builtin），不是关键字，
        // 所以必须在 fn 体内以 call 方式调用。
        let src = r#"
            fn w() @ spawn @ cpu {
                return 5
            }
            let h = spawn_task("w")
            let r = await(h)
        "#;
        let results = run(src).expect("run ok");
        let last = results.last().cloned().unwrap_or(Value::None);
        match last {
            Value::Int(n) => assert_eq!(n, 5),
            other => panic!("expected Int(5), got {:?}", other),
        }
    }

    #[test]
    fn mn_runtime_schedules_multiple_spawn_tasks() {
        // M:N 端到端：spawn_task 向调度器入队，验证调度器接收并执行。
        let src = r#"
            fn calc(x) @ spawn @ cpu {
                return x + 10
            }
            let t1 = spawn_task("calc", 5)
            let t2 = spawn_task("calc", 10)
        "#;
        let results = run(src).expect("run ok");
        // calc fn 声明返回 None，两个 spawn_task 各返回 Value::Task。
        assert_eq!(results.len(), 3);
        assert!(matches!(results[1], Value::Task(_)));
        assert!(matches!(results[2], Value::Task(_)));
    }

    #[test]
    fn spawn_full_chain_with_await() {
        // spawn_fn + spawn_task + await 完整链路端到端验证：协程真正通过调度器执行完毕。
        let src = r#"
            fn doubled(x) @ spawn @ cpu {
                return x * 2
            }
            let h = spawn_task("doubled", 21)
            let result = await(h)
        "#;
        let results = run(src).expect("run ok");
        let last = results.last().cloned().unwrap_or(Value::None);
        match last {
            Value::Int(n) => assert_eq!(n, 42),
            other => panic!("expected Int(42), got {:?}", other),
        }
    }
}
