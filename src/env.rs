/// Dalin L — 运行时环境

use std::collections::HashMap;
use std::fmt;

/// 运行时值标记常量
pub const SOME_TAG: &str = "__some__";
pub const NONE_TAG: &str = "__none__";
pub const OK_TAG: &str = "__ok__";
pub const ERR_TAG: &str = "__err__";
pub const ENUM_TAG: &str = "__enum__";
pub const DALIN_TYPE_KEY: &str = "__dalin_type__";

/// 运行时值
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
    None,
    Array(Vec<Value>),
    Option(bool, Option<Box<Value>>),       // (is_some, value)
    Result(bool, Option<Box<Value>>, Option<Box<Value>>), // (is_ok, value, error)
    Function(FnValue),
    Struct(HashMap<String, Value>),
    EnumVariant(String, String),             // (enum_name, variant_name)
}

#[derive(Debug, Clone)]
pub struct FnValue {
    pub name: String,
    pub params: Vec<super::ast::FnParam>,
    pub body: Vec<super::ast::Stmt>,
    pub closure: Environment,
    pub return_type: Option<super::ast::TypeRef>,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Char(v) => write!(f, "{}", v),
            Value::None => write!(f, "none"),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| format!("{}", v)).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Option(true, Some(v)) => write!(f, "Some({})", v),
            Value::Option(true, None) => write!(f, "Some(None)"),
            Value::Option(false, _) => write!(f, "None"),
            Value::Result(true, Some(v), _) => write!(f, "Ok({})", v),
            Value::Result(false, _, Some(e)) => write!(f, "Err({})", e),
            Value::Result(_, _, _) => write!(f, "Result"),
            Value::Function(fv) => write!(f, "<fn {}>", fv.name),
            Value::Struct(map) => {
                let ty = map.get(DALIN_TYPE_KEY).and_then(|v| {
                    if let Value::String(s) = v { Some(s.clone()) } else { None }
                }).unwrap_or_default();
                let inner: Vec<String> = map.iter()
                    .filter(|(k, _)| k.as_str() != DALIN_TYPE_KEY)
                    .map(|(k, v)| format!("{} = {}", k, v))
                    .collect();
                write!(f, "{} {{ {} }}", ty, inner.join(", "))
            }
            Value::EnumVariant(en, vn) => write!(f, "{}::{}", en, vn),
        }
    }
}

/// 作用域环境（带父链）
#[derive(Debug, Clone)]
pub struct Environment {
    pub vars: HashMap<String, Value>,
    pub parent: Option<Box<Environment>>,
}

impl Environment {
    pub fn new() -> Self {
        Self { vars: HashMap::new(), parent: None }
    }

    pub fn child(&self) -> Self {
        Self { vars: HashMap::new(), parent: Some(Box::new(self.clone())) }
    }

    pub fn define(&mut self, name: &str, value: Value) {
        self.vars.insert(name.to_string(), value);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.lookup(name);
        }
        None
    }

    pub fn assign(&mut self, name: &str, value: Value) -> bool {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), value);
            return true;
        }
        if let Some(parent) = &mut self.parent {
            return parent.assign(name, value);
        }
        false
    }
}