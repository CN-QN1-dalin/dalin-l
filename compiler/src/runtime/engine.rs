/// Dalin L 3.0 — Runtime Engine
///
/// The main execution engine: Runtime struct, call/exec/eval logic, and run_compiled helper.
use std::time::Instant;

use crate::ast::{BaseType, Expr, InterpolatePart, Program, Pattern, Stmt};
use crate::cffi::DFFIEnv;
use crate::ty2::{
    Capability, Effect, GovernanceLevel, TimeConstraint,
    parse_capability, parse_cognitive_loop, parse_confidence, parse_effect, parse_governance,
};

use super::env::{Environment, FnDef};
use super::scheduler::{cognitive_loop_to_phase, CognitiveLoopMachine, CognitiveLoopPhase, GovernanceChecker, TimeMonitor};
use super::value::{RuntimeResult, RuntimeError, RuntimeValue};

// ═══════════════════════════════════════════
//  RuntimeEvent — 执行事件日志
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    FnCall {
        name: String,
        cognitive_phase: CognitiveLoopPhase,
        governance: Option<GovernanceLevel>,
        elapsed_us: u64,
    },
    FnReturn {
        name: String,
        value: RuntimeValue,
        elapsed_us: u64,
    },
    CognitivePhaseAdvance {
        from: CognitiveLoopPhase,
        to: CognitiveLoopPhase,
    },
    GovernanceCheck {
        fn_name: String,
        level: GovernanceLevel,
        permitted: bool,
    },
    TimeWarning {
        fn_name: String,
        warning: String,
    },
}

// ═══════════════════════════════════════════
//  Runtime — 主执行引擎
// ═══════════════════════════════════════════

/// Dalin L Runtime 执行引擎
///
/// 用法：
/// ```ignore
/// let mut rt = Runtime::new(GovernanceLevel::Execute);
/// rt.load_program(&program);  // 注册所有函数
/// let result = rt.call("main", &[])?;
/// ```
pub struct Runtime {
    pub env: Environment,
    pub cognitive: CognitiveLoopMachine,
    pub governance: GovernanceChecker,
    pub time_monitor: TimeMonitor,
    pub events: Vec<RuntimeEvent>,
    /// C FFI dispatcher
    pub cffi: DFFIEnv,
    /// max call depth to prevent stack overflow
    pub max_depth: usize,
    pub current_depth: usize,
    /// Return 语句标志：当 exec_stmt 遇到 Return 时设置
    returned: bool,
    /// Return 语句的值
    return_value: RuntimeValue,
    /// 当前已执行步数
    step_count: u64,
    /// 最大允许步数
    max_steps: u64,
}

impl Runtime {
    pub fn new(session_governance: GovernanceLevel) -> Self {
        Self {
            env: Environment::new(),
            cognitive: CognitiveLoopMachine::new(),
            governance: GovernanceChecker::new(session_governance),
            time_monitor: TimeMonitor::new(),
            cffi: DFFIEnv::new(),
            events: Vec::new(),
            max_depth: 64,
            current_depth: 0,
            returned: false,
            return_value: RuntimeValue::None,
            step_count: 0,
            max_steps: 1_000_000,
        }
    }

    /// 将编译后的 Program 加载到运行时（注册所有函数）
    pub fn load_program(&mut self, prog: &Program) {
        for stmt in &prog.statements {
            self.load_stmt(stmt);
        }
    }

    fn load_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Fn {
                name,
                params,
                body,
                effect,
                capability,
                confidence,
                cognitive_loop,
                governance,
                latency,
                timeout,
                throughput,
                ..
            } => {
                let eff = effect.as_deref().map(parse_effect).unwrap_or(Effect::Pure);
                let cap = capability
                    .as_deref()
                    .map(parse_capability)
                    .unwrap_or(Capability::Cpu);
                let conf = confidence.as_deref().map(parse_confidence);
                let cl = cognitive_loop.as_deref().map(parse_cognitive_loop);
                let gov = governance.as_deref().map(parse_governance);
                let tc = {
                    let mut t = TimeConstraint::new();
                    if let Some(l) = latency {
                        t.latency_ms = l.trim_end_matches("ms").parse::<u64>().ok();
                    }
                    if let Some(to) = timeout {
                        t.timeout_ms = if to.ends_with("ms") {
                            to.trim_end_matches("ms").parse::<u64>().ok()
                        } else {
                            to.trim_end_matches("s")
                                .parse::<u64>()
                                .ok()
                                .map(|x| x * 1000)
                        };
                    }
                    if let Some(tp) = throughput {
                        t.throughput = tp.trim_end_matches("/s").parse::<u64>().ok();
                    }
                    if t.latency_ms.is_some() || t.timeout_ms.is_some() || t.throughput.is_some() {
                        Some(t)
                    } else {
                        None
                    }
                };
                self.env.register_fn(FnDef {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    effect: eff,
                    capability: cap,
                    confidence: conf,
                    cognitive_loop: cl,
                    governance: gov,
                    time_constraint: tc,
                });
            }
            Stmt::StructDef { name, .. } => {
                self.env.define(name.as_str(), RuntimeValue::String(format!("struct@{}", name)));
            }
            Stmt::EnumDef { name, .. } => {
                self.env.define(name.as_str(), RuntimeValue::String(format!("enum@{}", name)));
            }
            Stmt::TypeAlias { name, .. } => {
                self.env.define(name.as_str(), RuntimeValue::String(format!("alias@{}", name)));
            }
            Stmt::TraitDef { name, .. } => {
                self.env.define(name.as_str(), RuntimeValue::String(format!("trait@{}", name)));
            }
            Stmt::ImplBlock { type_name, .. } => {
                self.env
                    .define(type_name.as_str(), RuntimeValue::String(format!("impl@{}", type_name)));
            }
            Stmt::Channel {
                send_name,
                recv_name,
                elem_type,
                capacity,
            } => {
                let info = format!(
                    "channel<{:?}>[cap={}] (send={}, recv={})",
                    elem_type, capacity, send_name, recv_name
                );
                self.env
                    .define(send_name.as_str(), RuntimeValue::String(format!("chan_send:{}", info)));
                self.env.define(
                    recv_name.as_str(),
                    RuntimeValue::String(format!("chan_recv:{}", info)),
                );
            }
            Stmt::Llm { prompt, target, .. } => {
                let msg = if let Some(t) = target {
                    format!("[@llm] prompt: {:?}, target: {}", prompt, t)
                } else {
                    format!("[@llm] prompt: {:?}", prompt)
                };
                eprintln!("{}", msg);
            }
            Stmt::Use(path) => {
                self.env.define(path.as_str(), RuntimeValue::String(format!("use@{}", path)));
            }
            Stmt::Export(path) => {
                self.env
                    .define(path.as_str(), RuntimeValue::String(format!("export@{}", path)));
            }
            _ => {}
        }
    }

    /// 主入口：调用指定函数
    pub fn call(&mut self, fn_name: &str, args: &[RuntimeValue]) -> RuntimeResult<RuntimeValue> {
        let fn_def = self.env.lookup_fn(fn_name)?.clone();
        self.call_fn(&fn_def, args)
    }

    /// 调用函数（内部实现，含认知循环 + 治理 + 时间监控）
    fn call_fn(&mut self, fn_def: &FnDef, args: &[RuntimeValue]) -> RuntimeResult<RuntimeValue> {
        if self.current_depth >= self.max_depth {
            return Err(RuntimeError::RuntimePanic(format!(
                "stack overflow: max call depth {} exceeded",
                self.max_depth
            )));
        }
        self.current_depth += 1;
        let fn_start = Instant::now();

        // 1. 认知循环阶段检查
        if let Some(cl) = &fn_def.cognitive_loop {
            self.cognitive.check_phase(cl, &fn_def.name)?;
        }

        // 2. 治理权限检查
        if let Some(gov) = &fn_def.governance {
            self.governance.check(gov, &fn_def.name)?;
        }

        // 3. 记录认知相位
        let phase = fn_def
            .cognitive_loop
            .as_ref()
            .map(cognitive_loop_to_phase)
            .unwrap_or(match fn_def.effect {
                Effect::Io | Effect::Async => CognitiveLoopPhase::Acting,
                _ => CognitiveLoopPhase::Reasoning,
            });

        self.cognitive.advance(
            phase.clone(),
            &fn_def.name,
            fn_start.elapsed().as_micros() as u64,
        );

        self.events.push(RuntimeEvent::FnCall {
            name: fn_def.name.clone(),
            cognitive_phase: phase.clone(),
            governance: fn_def.governance.clone(),
            elapsed_us: fn_start.elapsed().as_micros() as u64,
        });

        // 4. 创建函数作用域并绑定参数（带数量校验）
        self.env.push_scope();
        if fn_def.params.len() != args.len() {
            return Err(RuntimeError::RuntimePanic(format!(
                "function '{}' expects {} argument(s), got {}",
                fn_def.name,
                fn_def.params.len(),
                args.len()
            )));
        }
        for (param, arg) in fn_def.params.iter().zip(args.iter()) {
            self.env.define(&param.name, arg.clone());
        }

        // 5. 执行函数体
        let result = self.exec_block(&fn_def.body);

        // 6. 治理日志
        if let Some(gov) = &fn_def.governance {
            let permitted = true; // 通过了上面的检查
            self.governance
                .check_log
                .push((fn_def.name.clone(), gov.clone(), permitted));
        }

        self.env.pop_scope();

        // 7. 时间监控
        let elapsed_ms = fn_start.elapsed().as_millis() as u64;
        self.time_monitor.record(&fn_def.name, elapsed_ms);
        if let Some(tc) = &fn_def.time_constraint {
            let time_errors = self
                .time_monitor
                .check_constraint(tc, &fn_def.name, elapsed_ms);
            for err in &time_errors {
                self.events.push(RuntimeEvent::TimeWarning {
                    fn_name: fn_def.name.clone(),
                    warning: format!("{}", err),
                });
            }
        }

        self.events.push(RuntimeEvent::FnReturn {
            name: fn_def.name.clone(),
            value: result.clone().unwrap_or(RuntimeValue::None),
            elapsed_us: fn_start.elapsed().as_micros() as u64,
        });

        self.current_depth -= 1;
        result
    }

    /// 执行语句块（多条语句）
    fn exec_block(&mut self, stmts: &[Stmt]) -> RuntimeResult<RuntimeValue> {
        let mut last_val = RuntimeValue::None;
        self.returned = false;
        self.return_value = RuntimeValue::None;
        for stmt in stmts {
            let result = self.exec_stmt(stmt)?;
            last_val = result;
            if self.returned {
                return Ok(self.return_value.clone());
            }
        }
        Ok(last_val)
    }

    /// 执行单条语句
    fn exec_stmt(&mut self, stmt: &Stmt) -> RuntimeResult<RuntimeValue> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let val = if let Some(expr) = value {
                    self.eval_expr(expr)?
                } else {
                    RuntimeValue::None
                };
                self.env.define(name, val);
                Ok(RuntimeValue::None)
            }
            Stmt::Const { name, value, .. } => {
                let val = if let Some(expr) = value {
                    self.eval_expr(expr)?
                } else {
                    RuntimeValue::None
                };
                self.env.define(name, val);
                Ok(RuntimeValue::None)
            }
            Stmt::Return(expr) => {
                let val = if let Some(e) = expr {
                    self.eval_expr(e)?
                } else {
                    RuntimeValue::None
                };
                self.returned = true;
                self.return_value = val.clone();
                Ok(val)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond_val = self.eval_expr(condition)?;
                let cond_bool = Self::as_bool(&cond_val)?;
                if cond_bool {
                    self.env.push_scope();
                    let result = self.exec_block(then_body);
                    self.env.pop_scope();
                    result
                } else {
                    self.env.push_scope();
                    let result = self.exec_block(else_body);
                    self.env.pop_scope();
                    result
                }
            }
            Stmt::While { condition, body } => {
                let mut last_val = RuntimeValue::None;
                loop {
                    self.step_count += 1;
                    if self.step_count >= self.max_steps {
                        return Err(RuntimeError::StepBudgetExceeded {
                            step_count: self.step_count,
                            max_steps: self.max_steps,
                        });
                    }
                    let cond_val = self.eval_expr(condition)?;
                    let cond_bool = Self::as_bool(&cond_val)?;
                    if !cond_bool {
                        break;
                    }
                    self.env.push_scope();
                    self.exec_block(body)?;
                    self.env.pop_scope();
                    if self.returned {
                        last_val = self.return_value.clone();
                        break;
                    }
                }
                Ok(last_val)
            }
            Stmt::For {
                target,
                iterable,
                body,
            } => {
                let iter_val = self.eval_expr(iterable)?;
                let items = match &iter_val {
                    RuntimeValue::Array(arr) => arr.clone(),
                    RuntimeValue::String(s) => s
                        .chars()
                        .map(|c| RuntimeValue::String(c.to_string()))
                        .collect(),
                    other => {
                        return Err(RuntimeError::TypeError {
                            expected: "array or string".to_string(),
                            actual: format!("{:?}", other),
                            detail: "for loop requires iterable".to_string(),
                        });
                    }
                };
                let mut last_val = RuntimeValue::None;
                for item in items {
                    self.step_count += 1;
                    if self.step_count >= self.max_steps {
                        return Err(RuntimeError::StepBudgetExceeded {
                            step_count: self.step_count,
                            max_steps: self.max_steps,
                        });
                    }
                    self.env.push_scope();
                    self.env.define(target, item);
                    self.exec_block(body)?;
                    self.env.pop_scope();
                    if self.returned {
                        last_val = self.return_value.clone();
                        break;
                    }
                }
                Ok(last_val)
            }
            Stmt::Match { target, arms } => {
                let target_val = self.eval_expr(target)?;
                for arm in arms {
                    if Self::match_pattern(&arm.pattern, &target_val) {
                        if let Some(guard) = arm.guard.as_ref() {
                            let guard_val = self.eval_expr(guard)?;
                            if !Self::as_bool(&guard_val)? {
                                continue;
                            }
                        }
                        self.env.push_scope();
                        let result = self.exec_block(&arm.body);
                        self.env.pop_scope();
                        return result;
                    }
                }
                Ok(RuntimeValue::None)
            }
            Stmt::Expr(expr) => self.eval_expr(expr),
            Stmt::TryCatch {
                try_body,
                catch_param,
                catch_body,
            } => {
                self.env.push_scope();
                let result = self.exec_block(try_body);
                self.env.pop_scope();
                match result {
                    Ok(val) => Ok(val),
                    Err(err) => {
                        self.env.push_scope();
                        if let Some(param) = catch_param {
                            self.env
                                .define(param, RuntimeValue::String(format!("{}", err)));
                        }
                        let result = self.exec_block(catch_body);
                        self.env.pop_scope();
                        result
                    }
                }
            }
            Stmt::Assert { condition, message } => {
                let cond_val = self.eval_expr(condition)?;
                let cond_bool = Self::as_bool(&cond_val)?;
                if !cond_bool {
                    let msg = if let Some(msg_expr) = message {
                        format!("{}", self.eval_expr(msg_expr)?)
                    } else {
                        "assertion failed".to_string()
                    };
                    return Err(RuntimeError::AssertionFailed { message: msg });
                }
                Ok(RuntimeValue::None)
            }
            Stmt::Spawn { fn_decl } => {
                if let Stmt::Fn {
                    name,
                    params,
                    body,
                    effect,
                    capability,
                    ..
                } = fn_decl.as_ref()
                {
                    let eff = effect.as_deref().map(parse_effect).unwrap_or(Effect::Spawn);
                    let cap = capability
                        .as_deref()
                        .map(parse_capability)
                        .unwrap_or(Capability::Cpu);
                    self.env.register_fn(FnDef {
                        name: name.clone(),
                        params: params.clone(),
                        body: body.clone(),
                        effect: eff,
                        capability: cap,
                        confidence: None,
                        cognitive_loop: None,
                        governance: None,
                        time_constraint: None,
                    });
                }
                Ok(RuntimeValue::None)
            }
            Stmt::Fn { .. } => Ok(RuntimeValue::None),
            Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::TraitDef { .. }
            | Stmt::ImplBlock { .. }
            | Stmt::TypeAlias { .. } => Ok(RuntimeValue::None),
            Stmt::Channel { .. } => Ok(RuntimeValue::None),
            Stmt::Llm { .. } => Ok(RuntimeValue::None),
            Stmt::Use(_) | Stmt::Export(_) => Ok(RuntimeValue::None),
            Stmt::ExternBlock { lang, items } => {
                if lang == "C" || lang == "c" {
                    for item in items {
                        self.cffi.register_extern(lang.as_str(), item);
                    }
                }
                Ok(RuntimeValue::None)
            }
        }
    }

    /// 表达式求值
    pub fn eval_expr(&mut self, expr: &Expr) -> RuntimeResult<RuntimeValue> {
        match expr {
            Expr::IntLiteral(v) => Ok(RuntimeValue::Int(*v)),
            Expr::FloatLiteral(v) => Ok(RuntimeValue::Float(*v)),
            Expr::StringLiteral(s) => Ok(RuntimeValue::String(s.clone())),
            Expr::BoolLiteral(b) => Ok(RuntimeValue::Bool(*b)),
            Expr::CharLiteral(c) => Ok(RuntimeValue::Char(*c)),
            Expr::Ident(name) => {
                match self.env.lookup(name) {
                    Ok(val) => Ok(val),
                    Err(_) => {
                        if self.env.lookup_fn(name).is_ok() {
                            Ok(RuntimeValue::Func(name.clone()))
                        } else {
                            Err(RuntimeError::UndefinedVariable(name.clone()))
                        }
                    }
                }
            }
            Expr::BinaryOp { left, op, right } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_binary_op(&l, op, &r)
            }
            Expr::UnaryOp { op, operand } => {
                let val = self.eval_expr(operand)?;
                match op.as_str() {
                    "-" => match val {
                        RuntimeValue::Int(v) => Ok(RuntimeValue::Int(-v)),
                        RuntimeValue::Float(v) => Ok(RuntimeValue::Float(-v)),
                        _ => Err(RuntimeError::TypeError {
                            expected: "number".to_string(),
                            actual: format!("{}", val),
                            detail: "unary negation".to_string(),
                        }),
                    },
                    "!" => match val {
                        RuntimeValue::Bool(v) => Ok(RuntimeValue::Bool(!v)),
                        _ => Err(RuntimeError::TypeError {
                            expected: "bool".to_string(),
                            actual: format!("{}", val),
                            detail: "logical not".to_string(),
                        }),
                    },
                    _ => Err(RuntimeError::RuntimePanic(format!(
                        "unknown unary op: {}",
                        op
                    ))),
                }
            }
            Expr::Call { func, args } => {
                let func_val = self.eval_expr(func)?;
                let fn_name = match &func_val {
                    RuntimeValue::Func(name) => name.clone(),
                    RuntimeValue::String(name) => name.clone(),
                    _ => {
                        return Err(RuntimeError::TypeError {
                            expected: "function".to_string(),
                            actual: format!("{}", func_val),
                            detail: "call target is not a function".to_string(),
                        });
                    }
                };
                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.eval_expr(arg)?);
                }
                let fn_def = self.env.lookup_fn(&fn_name)?.clone();
                self.call_fn(&fn_def, &evaluated_args)
            }
            Expr::IfExpr(cond, then_expr, else_expr) => {
                let cond_val = self.eval_expr(cond)?;
                let cond_bool = Self::as_bool(&cond_val)?;
                if cond_bool {
                    self.eval_expr(then_expr)
                } else {
                    self.eval_expr(else_expr)
                }
            }
            Expr::MatchExpr(target, arms) => {
                let target_val = self.eval_expr(target)?;
                for arm in arms {
                    if Self::match_pattern(&arm.pattern, &target_val) {
                        if let Some(guard) = arm.guard.as_ref() {
                            let guard_val = self.eval_expr(guard)?;
                            if !Self::as_bool(&guard_val)? {
                                continue;
                            }
                        }
                        let mut last_val = RuntimeValue::None;
                        for s in &arm.body {
                            last_val = self.exec_stmt(s)?;
                        }
                        return Ok(last_val);
                    }
                }
                Ok(RuntimeValue::None)
            }
            Expr::CCall {
                lib_path,
                func_name,
                args,
            } => {
                let mut c_args = Vec::new();
                for a in args.iter() {
                    c_args.push(self.eval_expr(a)?);
                }
                let lib = lib_path.as_deref().unwrap_or("libc");
                let return_type = self
                    .cffi
                    .lookup_extern(lib, func_name)
                    .map(|item| item.return_type.base.clone());
                self.cffi
                    .call_c_function(lib, func_name, &c_args, return_type.as_ref())
            }
            Expr::Interpolate { parts } => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        InterpolatePart::Literal(s) => {
                            result.push_str(s);
                        }
                        InterpolatePart::Expr(e) => {
                            let val = self.eval_expr(e)?;
                            result.push_str(&val.to_string());
                        }
                    }
                }
                Ok(RuntimeValue::String(result))
            }
            Expr::Array(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(self.eval_expr(item)?);
                }
                Ok(RuntimeValue::Array(vals))
            }
            Expr::MemberAccess { object, member } => {
                let obj = self.eval_expr(object)?;
                match obj {
                    RuntimeValue::Struct(_, fields) => {
                        for (k, v) in &fields {
                            if k == member {
                                return Ok(v.clone());
                            }
                        }
                        Err(RuntimeError::UndefinedVariable(format!(
                            "struct field '{}'",
                            member
                        )))
                    }
                    _ => Err(RuntimeError::TypeError {
                        expected: "struct".to_string(),
                        actual: format!("{}", obj),
                        detail: "member access".to_string(),
                    }),
                }
            }
            Expr::Index { array, index } => {
                let arr = self.eval_expr(array)?;
                let idx = self.eval_expr(index)?;
                match (&arr, &idx) {
                    (RuntimeValue::Array(items), RuntimeValue::Int(i)) => {
                        let i = *i as usize;
                        if i >= items.len() {
                            Err(RuntimeError::RuntimePanic(format!(
                                "index out of bounds: len={} index={}",
                                items.len(), i
                            )))
                        } else {
                            Ok(items[i].clone())
                        }
                    }
                    _ => Err(RuntimeError::TypeError {
                        expected: "array[index]".to_string(),
                        actual: format!("{}[{}]", arr, idx),
                        detail: "indexing".to_string(),
                    }),
                }
            }
            Expr::Pipe { input, ops } => {
                let mut val = self.eval_expr(input)?;
                for (op_name, op_expr) in ops {
                    match op_name.as_str() {
                        "map" | "filter" | "fold" => {
                            let fn_name = match op_expr {
                                Expr::Ident(name) => name.clone(),
                                Expr::StringLiteral(s) => s.clone(),
                                _ => format!("{:?}", op_expr),
                            };
                            let fn_def = self.env.lookup_fn(&fn_name)?.clone();
                            val = self.call_fn(&fn_def, &[val])?;
                        }
                        _ => {
                            let fn_def = self.env.lookup_fn(op_name)?.clone();
                            val = self.call_fn(&fn_def, &[val])?;
                        }
                    }
                }
                Ok(val)
            }
            Expr::Range {
                start,
                end,
                inclusive,
            } => {
                let s = self.eval_expr(start)?;
                let e = self.eval_expr(end)?;
                match (&s, &e) {
                    (RuntimeValue::Int(si), RuntimeValue::Int(ei)) => {
                        let mut items = Vec::new();
                        if *inclusive {
                            for i in *si..=*ei {
                                items.push(RuntimeValue::Int(i));
                            }
                        } else {
                            for i in *si..*ei {
                                items.push(RuntimeValue::Int(i));
                            }
                        }
                        Ok(RuntimeValue::Array(items))
                    }
                    _ => Err(RuntimeError::TypeError {
                        expected: "int..int".to_string(),
                        actual: format!("{}..{}", s, e),
                        detail: "range requires integers".to_string(),
                    }),
                }
            }
            Expr::OptionValue { is_some, value } => {
                if *is_some {
                    let v = if let Some(expr) = value {
                        Some(Box::new(self.eval_expr(expr)?))
                    } else {
                        None
                    };
                    Ok(RuntimeValue::Option(true, v))
                } else {
                    Ok(RuntimeValue::Option(false, None))
                }
            }
            Expr::ResultValue {
                is_ok,
                value,
                error,
            } => {
                if *is_ok {
                    let v = if let Some(expr) = value {
                        Some(Box::new(self.eval_expr(expr)?))
                    } else {
                        None
                    };
                    Ok(RuntimeValue::Result(true, v, None))
                } else {
                    let e = if let Some(expr) = error {
                        Some(Box::new(self.eval_expr(expr)?))
                    } else {
                        None
                    };
                    Ok(RuntimeValue::Result(false, None, e))
                }
            }
            Expr::NamedArg(_, _) => Ok(RuntimeValue::None),
            Expr::IsCheck(expr, type_ref) => {
                let val = self.eval_expr(expr)?;
                let expected_base = &type_ref.base;
                let matches = matches!(
                    (&val, expected_base),
                    (RuntimeValue::Int(_), BaseType::Int)
                        | (RuntimeValue::Float(_), BaseType::Float)
                        | (RuntimeValue::String(_), BaseType::String)
                        | (RuntimeValue::Bool(_), BaseType::Bool)
                        | (RuntimeValue::Array(_), BaseType::Array)
                        | (RuntimeValue::None, BaseType::None)
                );
                Ok(RuntimeValue::Bool(matches))
            }
            Expr::Cast(expr, type_ref) => {
                let val = self.eval_expr(expr)?;
                let target_base = &type_ref.base;
                match (&val, target_base) {
                    (RuntimeValue::Int(i), BaseType::Float) => {
                        Ok(RuntimeValue::Float((*i) as f64))
                    }
                    (RuntimeValue::Float(f), BaseType::Int) => {
                        Ok(RuntimeValue::Int((*f) as i64))
                    }
                    (RuntimeValue::String(s), BaseType::Int) => s
                        .parse::<i64>()
                        .map(RuntimeValue::Int)
                        .map_err(|_| RuntimeError::TypeError {
                            expected: "int".to_string(),
                            actual: format!("{}", val),
                            detail: "cast from string to int failed".to_string(),
                        }),
                    (RuntimeValue::String(s), BaseType::Float) => s
                        .parse::<f64>()
                        .map(RuntimeValue::Float)
                        .map_err(|_| RuntimeError::TypeError {
                            expected: "float".to_string(),
                            actual: format!("{}", val),
                            detail: "cast from string to float failed".to_string(),
                        }),
                    _ => Ok(val),
                }
            }
        }
    }

    // ═══════════════════════════════════════════
    //  辅助方法
    // ═══════════════════════════════════════════

    fn eval_binary_op(
        &self,
        left: &RuntimeValue,
        op: &str,
        right: &RuntimeValue,
    ) -> RuntimeResult<RuntimeValue> {
        match (left, op, right) {
            // 算术
            (RuntimeValue::Int(a), "+", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Int(a + b))
            }
            (RuntimeValue::Int(a), "-", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Int(a - b))
            }
            (RuntimeValue::Int(a), "*", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Int(a * b))
            }
            (RuntimeValue::Int(a), "/", RuntimeValue::Int(b)) => {
                if *b == 0 {
                    Err(RuntimeError::DivisionByZero)
                } else if *a == i64::MIN && *b == -1 {
                    Err(RuntimeError::RuntimePanic(
                        "integer overflow in division".into(),
                    ))
                } else {
                    Ok(RuntimeValue::Int(a / b))
                }
            }
            (RuntimeValue::Int(a), "%", RuntimeValue::Int(b)) => {
                if *b == 0 {
                    Err(RuntimeError::DivisionByZero)
                } else if *b == -1 {
                    Err(RuntimeError::RuntimePanic(
                        "integer overflow in modulo".into(),
                    ))
                } else {
                    Ok(RuntimeValue::Int(a % b))
                }
            }
            (RuntimeValue::Float(a), "+", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Float(a + b))
            }
            (RuntimeValue::Float(a), "-", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Float(a - b))
            }
            (RuntimeValue::Float(a), "*", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Float(a * b))
            }
            (RuntimeValue::Float(a), "/", RuntimeValue::Float(b)) => {
                if *b == 0.0 {
                    Err(RuntimeError::DivisionByZero)
                } else {
                    Ok(RuntimeValue::Float(a / b))
                }
            }
            // 比较
            (RuntimeValue::Int(a), "==", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Bool(a == b))
            }
            (RuntimeValue::Int(a), "!=", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Bool(a != b))
            }
            (RuntimeValue::Int(a), "<", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Bool(a < b))
            }
            (RuntimeValue::Int(a), ">", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Bool(a > b))
            }
            (RuntimeValue::Int(a), "<=", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Bool(a <= b))
            }
            (RuntimeValue::Int(a), ">=", RuntimeValue::Int(b)) => {
                Ok(RuntimeValue::Bool(a >= b))
            }
            (RuntimeValue::Float(a), "<", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Bool(a < b))
            }
            (RuntimeValue::Float(a), ">", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Bool(a > b))
            }
            (RuntimeValue::Float(a), "<=", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Bool(a <= b))
            }
            (RuntimeValue::Float(a), ">=", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Bool(a >= b))
            }
            (RuntimeValue::Float(a), "==", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Bool((a - b).abs() < f64::EPSILON))
            }
            (RuntimeValue::Float(a), "!=", RuntimeValue::Float(b)) => {
                Ok(RuntimeValue::Bool((a - b).abs() >= f64::EPSILON))
            }
            (RuntimeValue::String(a), "==", RuntimeValue::String(b)) => {
                Ok(RuntimeValue::Bool(a == b))
            }
            (RuntimeValue::String(a), "!=", RuntimeValue::String(b)) => {
                Ok(RuntimeValue::Bool(a != b))
            }
            (RuntimeValue::Bool(a), "==", RuntimeValue::Bool(b)) => {
                Ok(RuntimeValue::Bool(a == b))
            }
            (RuntimeValue::Bool(a), "!=", RuntimeValue::Bool(b)) => {
                Ok(RuntimeValue::Bool(a != b))
            }
            // 字符串拼接
            (RuntimeValue::String(a), "+", RuntimeValue::String(b)) => {
                Ok(RuntimeValue::String(format!("{}{}", a, b)))
            }
            // 逻辑
            (RuntimeValue::Bool(a), "&&", RuntimeValue::Bool(b)) => {
                Ok(RuntimeValue::Bool(*a && *b))
            }
            (RuntimeValue::Bool(a), "||", RuntimeValue::Bool(b)) => {
                Ok(RuntimeValue::Bool(*a || *b))
            }
            // 类型不匹配
            _ => Err(RuntimeError::TypeError {
                expected: format!("{} {} {}", left, op, right),
                actual: format!("{} {} {}", left, op, right),
                detail: format!("incompatible types for operator '{}'", op),
            }),
        }
    }

    pub fn as_bool(val: &RuntimeValue) -> RuntimeResult<bool> {
        match val {
            RuntimeValue::Bool(b) => Ok(*b),
            RuntimeValue::Int(i) => Ok(*i != 0),
            RuntimeValue::Float(f) => Ok(*f != 0.0),
            RuntimeValue::None => Ok(false),
            RuntimeValue::String(s) => Ok(!s.is_empty()),
            RuntimeValue::Array(a) => Ok(!a.is_empty()),
            _ => Err(RuntimeError::TypeError {
                expected: "bool".to_string(),
                actual: format!("{}", val),
                detail: "expected boolean value".to_string(),
            }),
        }
    }

    /// 简易模式匹配
    fn match_pattern(pattern: &Pattern, value: &RuntimeValue) -> bool {
        match pattern.kind.as_str() {
            "wild" => true,
            "ident" => true,
            "lit" => {
                if let Some(lit_expr) = &pattern.value {
                    format!("{:?}", lit_expr) == format!("{:?}", value)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

// ═══════════════════════════════════════════
//  RuntimeBuilder — 便利构造器
// ═══════════════════════════════════════════

/// 从编译结果构建运行时并执行主函数
pub fn run_compiled(prog: &Program, entry: &str) -> RuntimeResult<Vec<RuntimeEvent>> {
    let mut rt = Runtime::new(GovernanceLevel::Execute);
    rt.load_program(prog);
    rt.call(entry, &[])?;
    Ok(rt.events)
}
