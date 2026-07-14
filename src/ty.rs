/// Dalin L — HM 类型推断引擎
/// 完整的 Robinson Unification + 函数签名推导 + 多态

use crate::ast::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

// ── 类型变量 ──

static TYPE_VAR_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct TypeVar {
    pub id: String,
}

impl TypeVar {
    pub fn new() -> Self {
        let n = TYPE_VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self { id: format!("α{}", n) }
    }
}

// ── 类型环境的类型 ──

#[derive(Debug, Clone)]
pub enum TypeOrVar {
    Concrete(TypeRef),
    Variable(TypeVar),
}

impl std::fmt::Display for TypeOrVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concrete(t) => write!(f, "{}", t),
            Self::Variable(v) => write!(f, "{}", v.id),
        }
    }
}

// ── 类型环境 ──

#[derive(Debug, Clone)]
pub struct TypeEnv {
    pub types: HashMap<String, TypeOrVar>,
    pub parent: Option<Box<TypeEnv>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self { types: HashMap::new(), parent: None }
    }

    pub fn child(&self) -> Self {
        Self { types: HashMap::new(), parent: Some(Box::new(self.clone())) }
    }

    pub fn declare(&mut self, name: &str, typ: TypeOrVar) {
        self.types.insert(name.to_string(), typ);
    }

    pub fn lookup(&self, name: &str) -> Option<TypeOrVar> {
        if let Some(t) = self.types.get(name) {
            return Some(t.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.lookup(name);
        }
        None
    }
}

// ── 类型错误 ──

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeError: {}", self.message)
    }
}

// ── 内置类型常量 ──

pub fn int_type() -> TypeRef { TypeRef::new(BaseType::Int) }
pub fn float_type() -> TypeRef { TypeRef::new(BaseType::Float) }
pub fn string_type() -> TypeRef { TypeRef::new(BaseType::String) }
pub fn bool_type() -> TypeRef { TypeRef::new(BaseType::Bool) }
pub fn none_type() -> TypeRef { TypeRef::new(BaseType::None) }
pub fn unknown_type() -> TypeRef { TypeRef::new(BaseType::Unknown) }

fn is_numeric(base: &BaseType) -> bool {
    matches!(base, BaseType::Int | BaseType::Float)
}

fn can_compare(a: &TypeRef, b: &TypeRef) -> bool {
    matches!(a.base, BaseType::Int | BaseType::Float | BaseType::String | BaseType::Bool | BaseType::Char) &&
        matches!(b.base, BaseType::Int | BaseType::Float | BaseType::String | BaseType::Bool | BaseType::Char)
}

// ── Unification ──

fn occurs_check(tv: &TypeVar, typ: &TypeOrVar) -> bool {
    match typ {
        TypeOrVar::Variable(other) => tv.id == other.id,
        TypeOrVar::Concrete(t) => {
            if let Some(arg) = &t.generic_arg {
                if occurs_check(tv, &TypeOrVar::Concrete(*arg.clone())) { return true; }
            }
            if let Some(err) = &t.result_err {
                if occurs_check(tv, &TypeOrVar::Concrete(*err.clone())) { return true; }
            }
            false
        }
    }
}

fn bind(tv: &TypeVar, typ: &TypeOrVar, subst: &mut HashMap<String, TypeOrVar>) -> Result<(), TypeError> {
    if occurs_check(tv, typ) {
        return Err(TypeError { message: format!("Infinite type: {} occurs in {}", tv.id, typ) });
    }
    subst.insert(tv.id.clone(), typ.clone());
    Ok(())
}

fn apply_subst_local(subst: &HashMap<String, TypeOrVar>, typ: &TypeOrVar) -> TypeOrVar {
    match typ {
        TypeOrVar::Variable(tv) => {
            if let Some(resolved) = subst.get(&tv.id) {
                apply_subst_local(subst, resolved)
            } else {
                typ.clone()
            }
        }
        TypeOrVar::Concrete(t) => {
            let new_generic = t.generic_arg.as_ref()
                .map(|a| apply_subst_local(subst, &TypeOrVar::Concrete(*a.clone())));
            let new_err = t.result_err.as_ref()
                .map(|e| apply_subst_local(subst, &TypeOrVar::Concrete(*e.clone())));
            match (new_generic, new_err) {
                (Some(TypeOrVar::Concrete(g)), Some(TypeOrVar::Concrete(e))) =>
                    TypeOrVar::Concrete(TypeRef { base: t.base.clone(), generic_arg: Some(Box::new(g)), result_err: Some(Box::new(e)) }),
                (Some(TypeOrVar::Concrete(g)), None) =>
                    TypeOrVar::Concrete(TypeRef { base: t.base.clone(), generic_arg: Some(Box::new(g)), result_err: None }),
                _ => typ.clone(),
            }
        }
    }
}

pub fn unify(t1: &TypeOrVar, t2: &TypeOrVar, subst: &mut HashMap<String, TypeOrVar>) -> Result<(), TypeError> {
    let t1 = apply_subst_local(subst, t1);
    let t2 = apply_subst_local(subst, t2);

    match (&t1, &t2) {
        (TypeOrVar::Variable(tv), _) => bind(tv, &t2, subst),
        (_, TypeOrVar::Variable(tv)) => bind(tv, &t1, subst),
        (TypeOrVar::Concrete(a), TypeOrVar::Concrete(b)) => {
            if a.base != b.base {
                return Err(TypeError { message: format!("Type mismatch: expected {}, got {}", a, b) });
            }
            match (&a.generic_arg, &b.generic_arg) {
                (Some(ga), Some(gb)) => unify(&TypeOrVar::Concrete(*ga.clone()), &TypeOrVar::Concrete(*gb.clone()), subst)?,
                (Some(_), None) | (None, Some(_)) =>
                    return Err(TypeError { message: format!("Type mismatch: {} vs {}", a, b) }),
                _ => {}
            }
            match (&a.result_err, &b.result_err) {
                (Some(ea), Some(eb)) => unify(&TypeOrVar::Concrete(*ea.clone()), &TypeOrVar::Concrete(*eb.clone()), subst)?,
                _ => {}
            }
            Ok(())
        }
    }
}

// ── 类型推断器 ──

#[derive(Debug)]
pub struct TypeInferencer {
    pub env: TypeEnv,
    pub errors: Vec<TypeError>,
    pub inferred_types: HashMap<String, TypeRef>,
    pub fn_signatures: HashMap<String, (Vec<TypeOrVar>, TypeOrVar)>,
}

impl TypeInferencer {
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            errors: Vec::new(),
            inferred_types: HashMap::new(),
            fn_signatures: HashMap::new(),
        }
    }

    pub fn infer_program(&mut self, prog: &Program) -> &HashMap<String, TypeRef> {
        for stmt in &prog.statements {
            let _ = self.infer_stmt(stmt);
        }
        &self.inferred_types
    }

    fn infer_stmt(&mut self, stmt: &Stmt) -> TypeOrVar {
        match stmt {
            Stmt::Let { name, value, type_annotation, .. } => self.infer_let(name, value.as_deref(), type_annotation),
            Stmt::Const { name, value, .. } => {
                if let Some(v) = value {
                    let typ = self.infer_expr(v);
                    self.env.declare(name, typ.clone());
                    if let TypeOrVar::Concrete(t) = &typ {
                        self.inferred_types.insert(name.clone(), t.clone());
                    }
                }
                TypeOrVar::Concrete(none_type())
            }
            Stmt::Fn { name, params, return_type, body, .. } => self.infer_fn(name, params, return_type, body),
            Stmt::If { condition, then_body, else_body } => self.infer_if(condition, then_body, else_body),
            Stmt::For { target, iterable, body } => self.infer_for(target, iterable, body),
            Stmt::While { condition, body } => self.infer_while(condition, body),
            Stmt::Match { target, arms } => self.infer_match(target, arms),
            Stmt::Return(v) => {
                v.as_ref().map(|e| self.infer_expr(e)).unwrap_or(TypeOrVar::Concrete(none_type()))
            }
            Stmt::Expr(e) => self.infer_expr(e),
            _ => TypeOrVar::Concrete(none_type()),
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> TypeOrVar {
        match expr {
            Expr::IntLiteral(_) => TypeOrVar::Concrete(int_type()),
            Expr::FloatLiteral(_) => TypeOrVar::Concrete(float_type()),
            Expr::StringLiteral(_) => TypeOrVar::Concrete(string_type()),
            Expr::BoolLiteral(_) => TypeOrVar::Concrete(bool_type()),
            Expr::CharLiteral(_) => TypeOrVar::Concrete(TypeRef::new(BaseType::Char)),
            Expr::Ident(name) => self.infer_ident(name),
            Expr::BinaryOp { left, op, right } => self.infer_binary(left, op, right),
            Expr::UnaryOp { op, operand } => self.infer_unary(op, operand),
            Expr::Call { func, args } => self.infer_call(func, args),
            Expr::Pipe { input, ops } => self.infer_pipe(input, ops),
            Expr::Range { start, end, .. } => self.infer_range(start, end),
            Expr::Array(elems) => self.infer_array(elems),
            Expr::OptionValue { is_some, value } => self.infer_option(*is_some, value.as_deref()),
            Expr::ResultValue { is_ok, value, error } => self.infer_result(*is_ok, value.as_deref(), error.as_deref()),
            Expr::MemberAccess { .. } => TypeOrVar::Concrete(none_type()),
            Expr::Index { array, index } => self.infer_index(array, index),
            Expr::IfExpr(cond, then_expr, else_expr) => {
                let _cond_t = self.infer_expr(cond);
                let then_t = self.infer_expr(then_expr);
                let _else_t = self.infer_expr(else_expr);
                then_t
            }
            Expr::MatchExpr(target, arms) => {
                let _target_t = self.infer_expr(target);
                let mut arm_types = Vec::new();
                for arm in arms {
                    for s in &arm.body {
                        arm_types.push(self.infer_stmt(s));
                    }
                }
                arm_types.into_iter().next().unwrap_or(TypeOrVar::Concrete(none_type()))
            }
        }
    }

    fn infer_let(&mut self, name: &str, value: Option<&Expr>, type_annotation: &Option<TypeRef>) -> TypeOrVar {
        if let Some(ann) = type_annotation {
            let ann_tov = TypeOrVar::Concrete(ann.clone());
            self.env.declare(name, ann_tov.clone());
            self.inferred_types.insert(name.to_string(), ann.clone());
            if let Some(v) = value {
                let actual = self.infer_expr(v);
                if let Err(e) = unify(&ann_tov, &actual, &mut HashMap::new()) {
                    self.errors.push(e);
                }
            }
            ann_tov
        } else if let Some(v) = value {
            let typ = self.infer_expr(v);
            self.env.declare(name, typ.clone());
            if let TypeOrVar::Concrete(t) = &typ {
                self.inferred_types.insert(name.to_string(), t.clone());
            }
            typ
        } else {
            TypeOrVar::Concrete(none_type())
        }
    }

    fn infer_fn(&mut self, name: &str, params: &[FnParam], return_type: &Option<TypeRef>, body: &[Stmt]) -> TypeOrVar {
        let mut child_env = self.env.child();

        let param_types: Vec<TypeOrVar> = params.iter().map(|p| {
            if let Some(ann) = &p.type_annotation {
                let t = TypeOrVar::Concrete(ann.clone());
                child_env.declare(&p.name, t.clone());
                t
            } else {
                let tv = TypeOrVar::Variable(TypeVar::new());
                child_env.declare(&p.name, tv.clone());
                tv
            }
        }).collect();

        let ret_tv = TypeOrVar::Variable(TypeVar::new());

        // Pre-declare for recursive calls
        self.env.declare(name, TypeOrVar::Concrete(TypeRef::new(BaseType::Func)));
        self.fn_signatures.insert(name.to_string(), (param_types.clone(), ret_tv.clone()));

        // Infer body in child env
        let old_env = std::mem::replace(&mut self.env, child_env);
        let body_type = body.iter().last()
            .map(|s| self.infer_stmt(s))
            .unwrap_or(TypeOrVar::Concrete(none_type()));
        self.env = old_env;

        // Unify return type
        let final_ret = if let Some(ann) = return_type {
            let ann_tov = TypeOrVar::Concrete(ann.clone());
            let mut subst = HashMap::new();
            if let Err(e) = unify(&ann_tov, &body_type, &mut subst) {
                self.errors.push(e);
            }
            let resolved = apply_subst_local(&subst, &ann_tov);
            match &resolved {
                TypeOrVar::Concrete(t) => {
                    let _ = &self.fn_signatures.insert(name.to_string(), (param_types, resolved.clone()));
                }
                _ => { let _ = &self.fn_signatures.insert(name.to_string(), (param_types, body_type.clone())); }
            }
            resolved
        } else {
            let typ = body_type;
            self.fn_signatures.insert(name.to_string(), (param_types, typ.clone()));
            typ
        };

        self.env.declare(name, TypeOrVar::Concrete(TypeRef::new(BaseType::Func)));
        final_ret
    }

    fn infer_ident(&mut self, name: &str) -> TypeOrVar {
        match self.env.lookup(name) {
            Some(t) => t,
            None => {
                let tv = TypeOrVar::Variable(TypeVar::new());
                self.env.declare(name, tv.clone());
                tv
            }
        }
    }

    fn infer_binary(&mut self, left: &Expr, op: &str, right: &Expr) -> TypeOrVar {
        let l = self.infer_expr(left);
        let r = self.infer_expr(right);

        let mut subst = HashMap::new();
        let _ = unify(&l, &r, &mut subst);
        let unified = apply_subst_local(&subst, &l);

        match op {
            "+" | "-" | "*" | "/" | "%" => {
                match &unified {
                    TypeOrVar::Variable(_) => unified,
                    TypeOrVar::Concrete(t) if is_numeric(&t.base) => {
                        if op == "+" && (matches!(&l, TypeOrVar::Concrete(lt) if lt.base == BaseType::String)
                            || matches!(&r, TypeOrVar::Concrete(rt) if rt.base == BaseType::String)) {
                            return TypeOrVar::Concrete(string_type());
                        }
                        if matches!(&l, TypeOrVar::Concrete(lt) if lt.base == BaseType::Float)
                            || matches!(&r, TypeOrVar::Concrete(rt) if rt.base == BaseType::Float) {
                            return TypeOrVar::Concrete(float_type());
                        }
                        TypeOrVar::Concrete(int_type())
                    }
                    _ => TypeOrVar::Concrete(int_type()),
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => TypeOrVar::Concrete(bool_type()),
            "&&" | "||" => TypeOrVar::Concrete(bool_type()),
            "=" => r,
            _ => TypeOrVar::Concrete(unknown_type()),
        }
    }

    fn infer_unary(&mut self, op: &str, operand: &Expr) -> TypeOrVar {
        let t = self.infer_expr(operand);
        match op {
            "-" => {
                match &t {
                    TypeOrVar::Variable(_) => t,
                    TypeOrVar::Concrete(ct) if is_numeric(&ct.base) => t,
                    _ => TypeOrVar::Concrete(int_type()),
                }
            }
            "!" => TypeOrVar::Concrete(bool_type()),
            _ => TypeOrVar::Concrete(unknown_type()),
        }
    }

    fn infer_call(&mut self, func: &Expr, args: &[Expr]) -> TypeOrVar {
        // Try to get function name
        let func_name = match func {
            Expr::Ident(name) => name.clone(),
            _ => return {
                for a in args { self.infer_expr(a); }
                TypeOrVar::Concrete(none_type())
            },
        };

        if let Some((param_types, return_type)) = self.fn_signatures.get(&func_name) {
            // Fresh copy for polymorphic instantiation
            let fresh_map = |pt: &TypeOrVar| -> TypeOrVar {
                match pt {
                    TypeOrVar::Variable(tv) => TypeOrVar::Variable(TypeVar::new()),
                    TypeOrVar::Concrete(ct) => TypeOrVar::Concrete(ct.clone()),
                }
            };
            let fresh_params: Vec<TypeOrVar> = param_types.iter().map(fresh_map).collect();
            let fresh_return = fresh_map(return_type);

            let mut subst = HashMap::new();
            for (i, arg) in args.iter().enumerate() {
                if i < fresh_params.len() {
                    let arg_type = self.infer_expr(arg);
                    let _ = unify(&fresh_params[i], &arg_type, &mut subst);
                } else {
                    self.infer_expr(arg);
                }
            }

            return apply_subst_local(&subst, &fresh_return);
        }

        // Builtins
        if ["println", "print", "len", "push", "assert", "int", "float", "str", "abs", "range"].contains(&func_name.as_str()) {
            for a in args { self.infer_expr(a); }
            match func_name.as_str() {
                "int" | "float" | "str" | "abs" => TypeOrVar::Concrete(int_type()),
                "len" | "range" => TypeOrVar::Concrete(int_type()),
                _ => TypeOrVar::Concrete(none_type()),
            }
        } else {
            for a in args { self.infer_expr(a); }
            TypeOrVar::Concrete(none_type())
        }
    }

    fn infer_pipe(&mut self, input: &Expr, ops: &[(String, Expr)]) -> TypeOrVar {
        let mut current = self.infer_expr(input);
        for (name, _) in ops {
            if let Some((param_types, return_type)) = self.fn_signatures.get(name) {
                let fresh_ret = match return_type {
                    TypeOrVar::Variable(tv) => TypeOrVar::Variable(TypeVar::new()),
                    TypeOrVar::Concrete(ct) => TypeOrVar::Concrete(ct.clone()),
                };
                let mut subst = HashMap::new();
                if let Some(first_param) = param_types.first() {
                    let _ = unify(first_param, &current, &mut subst);
                }
                current = apply_subst_local(&subst, &fresh_ret);
            } else if ["len", "int", "float", "str", "abs"].contains(&name.as_str()) {
                // 内置函数：len 返回 int，其他返回输入类型
                match name.as_str() {
                    "len" => current = TypeOrVar::Concrete(int_type()),
                    "int" | "float" | "str" | "abs" => current = current.clone(),
                    _ => current = TypeOrVar::Concrete(unknown_type()),
                }
            } else {
                current = TypeOrVar::Concrete(unknown_type());
            }
        }
        current
    }

    fn infer_range(&mut self, start: &Expr, end: &Expr) -> TypeOrVar {
        let _s = self.infer_expr(start);
        let _e = self.infer_expr(end);
        TypeOrVar::Concrete(TypeRef::new(BaseType::Array))
    }

    fn infer_array(&mut self, elems: &[Expr]) -> TypeOrVar {
        if elems.is_empty() { return TypeOrVar::Concrete(TypeRef::new(BaseType::Array)); }
        let first = self.infer_expr(&elems[0]);
        for elem in &elems[1..] {
            let et = self.infer_expr(elem);
            let mut subst = HashMap::new();
            if let Err(e) = unify(&first, &et, &mut subst) {
                self.errors.push(e);
            }
        }
        match &first {
            TypeOrVar::Concrete(t) =>
                TypeOrVar::Concrete(TypeRef { base: BaseType::Array, generic_arg: Some(Box::new(t.clone())), result_err: None }),
            _ => TypeOrVar::Concrete(TypeRef::new(BaseType::Array)),
        }
    }

    fn infer_option(&mut self, is_some: bool, value: Option<&Expr>) -> TypeOrVar {
        if is_some {
            if let Some(v) = value {
                let inner = self.infer_expr(v);
                match &inner {
                    TypeOrVar::Concrete(t) =>
                        TypeOrVar::Concrete(TypeRef { base: BaseType::Option, generic_arg: Some(Box::new(t.clone())), result_err: None }),
                    _ => TypeOrVar::Concrete(TypeRef::new(BaseType::Option)),
                }
            } else {
                TypeOrVar::Concrete(TypeRef::new(BaseType::Option))
            }
        } else {
            TypeOrVar::Concrete(TypeRef::new(BaseType::Option))
        }
    }

    fn infer_result(&mut self, is_ok: bool, value: Option<&Expr>, error: Option<&Expr>) -> TypeOrVar {
        if is_ok {
            if let Some(v) = value {
                let inner = self.infer_expr(v);
                match &inner {
                    TypeOrVar::Concrete(t) =>
                        TypeOrVar::Concrete(TypeRef { base: BaseType::Result, generic_arg: Some(Box::new(t.clone())), result_err: Some(Box::new(string_type())) }),
                    _ => TypeOrVar::Concrete(TypeRef::new(BaseType::Result)),
                }
            } else {
                TypeOrVar::Concrete(TypeRef::new(BaseType::Result))
            }
        } else {
            TypeOrVar::Concrete(TypeRef::new(BaseType::Result))
        }
    }

    fn infer_index(&mut self, array: &Expr, index: &Expr) -> TypeOrVar {
        let arr_type = self.infer_expr(array);
        let _idx_type = self.infer_expr(index);
        match &arr_type {
            TypeOrVar::Concrete(t) => {
                if let Some(arg) = &t.generic_arg {
                    TypeOrVar::Concrete(*arg.clone())
                } else {
                    TypeOrVar::Concrete(none_type())
                }
            }
            _ => TypeOrVar::Concrete(none_type()),
        }
    }

    fn infer_if(&mut self, condition: &Expr, then_body: &[Stmt], else_body: &[Stmt]) -> TypeOrVar {
        let _cond = self.infer_expr(condition);
        let then_type = then_body.iter().last()
            .map(|s| self.infer_stmt(s))
            .unwrap_or(TypeOrVar::Concrete(none_type()));
        let else_type = else_body.iter().last()
            .map(|s| self.infer_stmt(s))
            .unwrap_or(TypeOrVar::Concrete(none_type()));

        match (&then_type, &else_type) {
            (TypeOrVar::Concrete(t), _) if t.base == BaseType::None => else_type,
            (_, TypeOrVar::Concrete(e)) if e.base == BaseType::None => then_type,
            _ => then_type,
        }
    }

    fn infer_for(&mut self, target: &str, iterable: &Expr, body: &[Stmt]) -> TypeOrVar {
        let _iter = self.infer_expr(iterable);
        self.env.declare(target, TypeOrVar::Concrete(int_type()));
        body.iter().last()
            .map(|s| self.infer_stmt(s))
            .unwrap_or(TypeOrVar::Concrete(none_type()))
    }

    fn infer_while(&mut self, condition: &Expr, body: &[Stmt]) -> TypeOrVar {
        let _cond = self.infer_expr(condition);
        body.iter().last()
            .map(|s| self.infer_stmt(s))
            .unwrap_or(TypeOrVar::Concrete(none_type()))
    }

    fn infer_match(&mut self, target: &Expr, arms: &[MatchArm]) -> TypeOrVar {
        let _target = self.infer_expr(target);
        let mut arm_types = Vec::new();
        for arm in arms {
            for s in &arm.body {
                arm_types.push(self.infer_stmt(s));
            }
        }
        arm_types.into_iter().next().unwrap_or(TypeOrVar::Concrete(none_type()))
    }

    pub fn print_report(&self) -> String {
        let mut lines = vec!["\n=== Type Inference Report ===".to_string()];
        lines.push("\nInferred Types:".into());
        let mut sorted: Vec<_> = self.inferred_types.iter().collect();
        sorted.sort_by_key(|(k, _)| (*k).clone());
        for (name, typ) in &sorted {
            lines.push(format!("  {}: {}", name, typ));
        }
        if !self.errors.is_empty() {
            lines.push("\nErrors:".into());
            for err in &self.errors {
                lines.push(format!("  ❌ {}", err));
            }
        } else {
            lines.push("\n✅ No type errors!".into());
        }
        lines.push(String::new());
        lines.join("\n")
    }
}