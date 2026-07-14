/// Dalin L — 树遍历解释器

use crate::ast::*;
use crate::env::*;
use std::collections::HashMap;

#[derive(Debug)]
pub struct RuntimeError(pub String);

#[derive(Debug)]
pub struct ReturnSignal(pub Value);

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
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            global_env: Environment::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
            return_value: None,
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
            Stmt::Fn { name, params, return_type, body, .. } => {
                self.eval_fn_decl(name, params, return_type, body, env)
            }
            Stmt::Return(v) => {
                let val = match v {
                    Some(e) => self.eval_expr(e, env)?,
                    None => Value::None,
                };
                self.return_value = Some(val);
                return Err(RuntimeError("__return__".into()));
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
            Stmt::Spawn { .. } => {
                println!("⚠️ spawn 在 Python 原型中未实现，将在 Rust 移植中支持");
                Ok(Value::None)
            }
            Stmt::Channel { .. } => {
                println!("⚠️ channel 在 Python 原型中未实现，将在 Rust 移植中支持");
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

    fn eval_fn_decl(&mut self, name: &str, params: &[FnParam], return_type: &Option<TypeRef>, body: &[Stmt], env: &mut Environment) -> Result<Value, RuntimeError> {
        let fn_val = FnValue {
            name: name.to_string(),
            params: params.to_vec(),
            body: body.to_vec(),
            closure: env.clone(),
            return_type: return_type.clone(),
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
            let cond_val = self.eval_expr(condition, env)?;
            if !self.truthy(&cond_val) { break; }
            let result = self.eval_block(body, &mut env.child());
            match result {
                Err(RuntimeError(ref msg)) if msg == "__return__" => return result,
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
            env.define(target, item.clone());
            result = self.eval_block(body, env)?;
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
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                    _ => Err(RuntimeError(format!("Cannot divide {:?} and {:?}", left_val, right_val))),
                }
            }
            "%" => {
                match (&left_val, &right_val) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
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
        let builtins: [&str; 12] = ["println", "println!", "print", "print!", "len", "push", "assert", "int", "float", "str", "abs", "range"];
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
            Err(RuntimeError(ref msg)) if msg == "__return__" => {
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
                        if let Value::Option(true, Some(v)) = value {
                            if let Some(ref binding) = pat.binding {
                                env.define(binding, *v.clone());
                                return true;
                            }
                        }
                        false
                    }
                    "None" => matches!(value, Value::Option(false, _)),
                    "Ok" => {
                        if let Value::Result(true, Some(v), _) = value {
                            if let Some(ref binding) = pat.binding {
                                env.define(binding, *v.clone());
                                return true;
                            }
                        }
                        false
                    }
                    "Err" => {
                        if let Value::Result(false, _, Some(e)) = value {
                            if let Some(ref binding) = pat.binding {
                                env.define(binding, *e.clone());
                                return true;
                            }
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

    fn install_builtins(&mut self) {
        // Builtins are handled in eval_call
    }
}

/// 便捷入口
pub fn run_source(source: &str) -> Result<Vec<Value>, RuntimeError> {
    let mut lex = crate::lexer::Lexer::new(source);
    let tokens = lex.tokenize().map_err(|e| RuntimeError(e.to_string()))?;
    let mut parser = crate::parser::Parser::new(tokens);
    let prog = parser.parse().map_err(|e| RuntimeError(e.to_string()))?;
    let mut interp = Interpreter::new();
    interp.interpret(&prog)
}