/// Dalin L 3.0 — Environment (variable scope + function registry)
///
/// FnDef: function definition with all metadata fields
/// Environment: stack-based variable scoping + global function table
use std::collections::HashMap;

use crate::ast::{FnParam, Stmt};
use crate::ty2::{
    Capability, CognitiveLoop, Confidence, Effect, GovernanceLevel, TimeConstraint,
};

use super::value::{RuntimeResult, RuntimeError, RuntimeValue};

// ═══════════════════════════════════════════
//  FnDef — 函数定义
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<FnParam>,
    pub body: Vec<Stmt>,
    pub effect: Effect,
    pub capability: Capability,
    pub confidence: Option<Confidence>,
    pub cognitive_loop: Option<CognitiveLoop>,
    pub governance: Option<GovernanceLevel>,
    pub time_constraint: Option<TimeConstraint>,
}

// ═══════════════════════════════════════════
//  Environment — 作用域变量 + 函数注册表
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Environment {
    /// 作用域栈（每层为一个变量表）
    frames: Vec<HashMap<String, RuntimeValue>>,
    /// 函数注册表（全局）
    functions: HashMap<String, FnDef>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    pub fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
            functions: HashMap::new(),
        }
    }

    /// 进入新作用域
    pub fn push_scope(&mut self) {
        self.frames.push(HashMap::new());
    }

    /// 退出作用域
    pub fn pop_scope(&mut self) {
        self.frames.pop();
    }

    /// 在当前作用域定义变量
    pub fn define(&mut self, name: &str, value: RuntimeValue) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_string(), value);
        }
    }

    /// 查找变量（从内向外）
    pub fn lookup(&self, name: &str) -> RuntimeResult<RuntimeValue> {
        for frame in self.frames.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Ok(val.clone());
            }
        }
        Err(RuntimeError::UndefinedVariable(name.to_string()))
    }

    /// 注册函数
    pub fn register_fn(&mut self, def: FnDef) {
        self.functions.insert(def.name.clone(), def);
    }

    /// 查找函数
    pub fn lookup_fn(&self, name: &str) -> RuntimeResult<&FnDef> {
        self.functions
            .get(name)
            .ok_or_else(|| RuntimeError::UndefinedFunction(name.to_string()))
    }
}
