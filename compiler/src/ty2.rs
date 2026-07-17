/// Dalin L 2.0 — 七通道类型系统
///
/// 类型 = (值类型) × (效应类型) × (能力类型)
/// 七通道正交，各自独立做 unification

use crate::ast::{BaseType, Stmt, TypeRef};
use std::collections::HashMap;
use std::fmt;
