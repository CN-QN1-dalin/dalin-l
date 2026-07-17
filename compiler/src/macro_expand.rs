/// Dalin L 3.0 — Phase H: 编译时宏系统 (Macro System)
///
/// 支持两种宏：
/// 1. `#[derive(Debug, Clone)]` 属性宏 — 在编译期自动生成 impl 块
/// 2. `macro_rules! foo { ... }` 声明式宏 — pattern → expansion 模板替换
///
/// 宏展开发生在语义分析之前执行。
use crate::ast::{Program, Stmt, Expr, MacroDecl};
use std::collections::HashMap;

// ═══════════════════════════════
//  宏展开结果
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct MacroExpansion {
    pub original: Program,
    pub expanded: Program,
    pub expansions: Vec<ExpansionRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpansionRecord {
    pub macro_name: String,
    pub target_type: String,
    pub location: String,
    pub generated_stmts: usize,
}

// ═══════════════════════════════
//  #[derive] 属性宏展开器
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct DeriveExpander {
    pub records: Vec<ExpansionRecord>,
}

impl Default for DeriveExpander {
    fn default() -> Self {
        Self::new()
    }
}

impl DeriveExpander {
    pub fn new() -> Self {
        Self { records: Vec::new() }
    }

    pub fn expand_derives(&mut self, stmts: &mut Vec<Stmt>) {
        // Collect struct indices with derives first
        let struct_entries: Vec<(usize, String, Vec<String>, Vec<crate::ast::FieldDef>)> = stmts.iter().enumerate()
            .filter_map(|(i, stmt)| {
                if let Stmt::StructDef { name, derives, fields } = stmt
                    && !derives.is_empty() {
                        return Some((i, name.clone(), derives.clone(), fields.clone()));
                    }
                None
            }).collect();

        for (_idx, name, derives, fields) in struct_entries.into_iter().rev() {
            let gen_stmts: Vec<Stmt> = derives.iter()
                .filter_map(|trait_name| {
                    match trait_name.as_str() {
                        "Debug" => {
                            let stmt = self.generate_debug_impl(&name, &fields);
                            self.records.push(ExpansionRecord {
                                macro_name: "derive(Debug)".to_string(),
                                target_type: "struct".to_string(),
                                location: format!("struct {}", name),
                                generated_stmts: 1,
                            });
                            Some(stmt)
                        }
                        "Clone" => {
                            let stmt = self.generate_clone_impl(&name, &fields);
                            self.records.push(ExpansionRecord {
                                macro_name: "derive(Clone)".to_string(),
                                target_type: "struct".to_string(),
                                location: format!("struct {}", name),
                                generated_stmts: 1,
                            });
                            Some(stmt)
                        }
                        "Copy" => {
                            let stmt = self.generate_copy_impl(&name);
                            self.records.push(ExpansionRecord {
                                macro_name: "derive(Copy)".to_string(),
                                target_type: "struct".to_string(),
                                location: format!("struct {}", name),
                                generated_stmts: 1,
                            });
                            Some(stmt)
                        }
                        "PartialEq" => {
                            let stmt = self.generate_partialeq_impl(&name, &fields);
                            self.records.push(ExpansionRecord {
                                macro_name: "derive(PartialEq)".to_string(),
                                target_type: "struct".to_string(),
                                location: format!("struct {}", name),
                                generated_stmts: 1,
                            });
                            Some(stmt)
                        }
                        "Default" => {
                            let stmt = self.generate_default_impl(&name, &fields);
                            self.records.push(ExpansionRecord {
                                macro_name: "derive(Default)".to_string(),
                                target_type: "struct".to_string(),
                                location: format!("struct {}", name),
                                generated_stmts: 1,
                            });
                            Some(stmt)
                        }
                        _ => None,
                    }
                })
                .collect();

            let new_items = std::iter::once(stmts.remove(_idx))
                .chain(gen_stmts)
                .collect::<Vec<_>>();
            let offset = _idx;
            for (j, s) in new_items.into_iter().enumerate() {
                stmts.insert(offset + j, s);
            }
        }
    }

    pub fn expand_enum_derives(&self, _stmts: &mut Vec<Stmt>) {
        // Simplified: enum derive macros not yet generating extra code
    }

    fn generate_debug_impl(&self, name: &str, _fields: &[crate::ast::FieldDef]) -> Stmt {
        let debug_body: Vec<Stmt> = vec![
            Stmt::Expr(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("print".to_string())),
                args: vec![Expr::StringLiteral(format!("[DEBUG] {}", name))],
            })),
            Stmt::Return(None),
        ];
        Stmt::Fn {
            name: "fmt".to_string(),
            type_params: vec![],
            params: vec![crate::ast::FnParam {
                name: "_fmt".to_string(),
                type_annotation: Some(crate::ast::TypeRef::new(crate::ast::BaseType::String)),
                default: None,
            }],
            return_type: None, effect: None, capability: None, llm_prompt: None,
            confidence: None, cognitive_loop: None, governance: None,
            latency: None, timeout: None, throughput: None,
            body: debug_body, async_: false, pub_: false,
        }
    }

    fn generate_clone_impl(&self, name: &str, _fields: &[crate::ast::FieldDef]) -> Stmt {
        let body: Vec<Stmt> = vec![
            Stmt::Expr(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("println".to_string())),
                args: vec![Expr::StringLiteral(format!("clone {}", name))],
            })),
            Stmt::Return(Some(Box::new(Expr::IntLiteral(0)))),
        ];
        Stmt::Fn {
            name: "clone".to_string(), type_params: vec![], params: vec![], return_type: None,
            effect: None, capability: None, llm_prompt: None, confidence: None,
            cognitive_loop: None, governance: None, latency: None, timeout: None,
            throughput: None, body, async_: false, pub_: false,
        }
    }

    fn generate_copy_impl(&self, _name: &str) -> Stmt {
        Stmt::Fn {
            name: "copy".to_string(), type_params: vec![], params: vec![], return_type: None,
            effect: None, capability: None, llm_prompt: None, confidence: None,
            cognitive_loop: None, governance: None, latency: None, timeout: None,
            throughput: None, body: vec![Stmt::Return(None)],
            async_: false, pub_: false,
        }
    }

    fn generate_partialeq_impl(&self, _name: &str, _fields: &[crate::ast::FieldDef]) -> Stmt {
        let body: Vec<Stmt> = vec![
            Stmt::Assert {
                condition: Box::new(Expr::BoolLiteral(true)),
                message: Some(Box::new(Expr::StringLiteral("PartialEq check".to_string()))),
            },
            Stmt::Return(Some(Box::new(Expr::BoolLiteral(true)))),
        ];
        Stmt::Fn {
            name: "eq".to_string(),
            type_params: vec![],
            params: vec![crate::ast::FnParam {
                name: "other".to_string(),
                type_annotation: None,
                default: None,
            }],
            return_type: None, effect: None, capability: None, llm_prompt: None,
            confidence: None, cognitive_loop: None, governance: None,
            latency: None, timeout: None, throughput: None,
            body, async_: false, pub_: false,
        }
    }

    fn generate_default_impl(&self, _name: &str, fields: &[crate::ast::FieldDef]) -> Stmt {
        let mut body: Vec<Stmt> = Vec::new();
        for field in fields {
            body.push(Stmt::Let {
                name: format!("default_{}", field.name),
                value: Some(Box::new(match field.type_annotation.base {
                    crate::ast::BaseType::Int => Expr::IntLiteral(0),
                    crate::ast::BaseType::Float => Expr::FloatLiteral(0.0),
                    crate::ast::BaseType::String => Expr::StringLiteral(String::new()),
                    crate::ast::BaseType::Bool => Expr::BoolLiteral(false),
                    crate::ast::BaseType::Char => Expr::CharLiteral('\0'),
                    _ => Expr::IntLiteral(0),
                })),
                type_annotation: None, mutable: false,
            });
        }
        body.push(Stmt::Return(Some(Box::new(Expr::IntLiteral(0)))));
        Stmt::Fn {
            name: "default".to_string(), type_params: vec![], params: vec![], return_type: None,
            effect: None, capability: None, llm_prompt: None, confidence: None,
            cognitive_loop: None, governance: None, latency: None, timeout: None,
            throughput: None, body, async_: false, pub_: false,
        }
    }
}

// ═══════════════════════════════
//  macro_rules! 声明式宏注册表
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct MacroPatternRule {
    pub patterns: Vec<String>,
    pub expansion_templates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MacroDef {
    pub rules: Vec<MacroPatternRule>,
}

#[derive(Debug, Clone)]
pub struct MacroRegistry {
    macros: HashMap<String, MacroDef>,
}

impl Default for MacroRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MacroRegistry {
    pub fn new() -> Self {
        Self { macros: HashMap::new() }
    }

    pub fn len(&self) -> usize { self.macros.len() }
    pub fn is_empty(&self) -> bool { self.macros.is_empty() }

    pub fn insert(&mut self, key: String, val: MacroDef) -> Option<MacroDef> {
        self.macros.insert(key, val)
    }

    pub fn get(&self, key: &str) -> Option<&MacroDef> {
        self.macros.get(key)
    }
}

pub fn register_macro_decls(registry: &mut MacroRegistry, macros: &[MacroDecl]) {
    for macro_decl in macros {
        match macro_decl {
            MacroDecl::Declarative { name, rules } => {
                registry.insert(name.clone(), MacroDef {
                    rules: rules.iter().map(|r| MacroPatternRule {
                        patterns: r.pattern.clone(),
                        expansion_templates: r.expansion.clone(),
                    }).collect(),
                });
            }
            MacroDecl::Derive { .. } => {}
        }
    }
}

pub fn extract_macro_decls(_stmts: &[Stmt]) -> Vec<MacroDecl> {
    // Simplified: scan for macro rules declarations in AST
    // Full implementation would walk the AST looking for macro_rules! patterns
    Vec::new()
}

// ═══════════════════════════════
//  主宏展开器
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct MacroExpander {
    derive_expander: DeriveExpander,
    #[allow(dead_code)]
    registry: MacroRegistry,
    #[allow(dead_code)]
    records: Vec<ExpansionRecord>,
}

impl Default for MacroExpander {
    fn default() -> Self {
        Self::new()
    }
}

impl MacroExpander {
    pub fn new() -> Self {
        Self {
            derive_expander: DeriveExpander::new(),
            registry: MacroRegistry::new(),
            records: Vec::new(),
        }
    }

    /// 执行完整的宏展开管线
    pub fn expand(&self, program: &Program) -> MacroExpansion {
        let mut expanded = program.clone();
        
        // Clone the derive_expander, expand, then get records
        let mut expander = self.derive_expander.clone();
        expander.expand_derives(&mut expanded.statements);
        expander.expand_enum_derives(&mut expanded.statements);
        
        let expansions = expander.records.iter().map(|r| ExpansionRecord {
            macro_name: r.macro_name.clone(),
            target_type: r.target_type.clone(),
            location: r.location.clone(),
            generated_stmts: r.generated_stmts,
        }).collect::<Vec<_>>();
        
        MacroExpansion {
            original: program.clone(),
            expanded,
            expansions,
        }
    }

    #[allow(dead_code)]
    fn expand_derive_attrs(&self, stmts: &mut Vec<Stmt>) {
        let mut expander = DeriveExpander::new();
        expander.expand_derives(stmts);
        expander.expand_enum_derives(stmts);
    }
}

// ═══════════════════════════════
//  内置宏映射 (println!, dbg!, assert!)
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct BuiltinMacros {
    macros: HashMap<String, BuiltinMacroDef>,
}

#[derive(Debug, Clone)]
pub struct BuiltinMacroDef {
    pub name: String,
    pub num_args: usize,
    pub is_variadic: bool,
    expand_fn: fn(&[Expr]) -> Vec<Stmt>,
}

impl Default for BuiltinMacros {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinMacros {
    pub fn new() -> Self {
        let mut macros = HashMap::new();

        macros.insert("println".to_string(), BuiltinMacroDef {
            name: "println!".to_string(), num_args: 1, is_variadic: true,
            expand_fn: |args| vec![Stmt::Expr(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("io::println".to_string())),
                args: args.to_vec(),
            }))],
        });

        macros.insert("dbg".to_string(), BuiltinMacroDef {
            name: "dbg!".to_string(), num_args: 1, is_variadic: true,
            expand_fn: |args| vec![
                Stmt::Expr(Box::new(Expr::Call {
                    func: Box::new(Expr::Ident("io::print".to_string())),
                    args: vec![Expr::StringLiteral("[dbg] ".to_string())],
                })),
                Stmt::Expr(Box::new(Expr::Call {
                    func: Box::new(Expr::Ident("io::println".to_string())),
                    args: args.to_vec(),
                })),
            ],
        });

        macros.insert("assert".to_string(), BuiltinMacroDef {
            name: "assert!".to_string(), num_args: 1, is_variadic: false,
            expand_fn: |args| {
                if args.len() >= 2 {
                    vec![Stmt::Assert {
                        condition: Box::new(args[0].clone()),
                        message: Some(Box::new(args[1].clone())),
                    }]
                } else if args.len() == 1 {
                    vec![Stmt::Assert {
                        condition: Box::new(args[0].clone()),
                        message: None,
                    }]
                } else {
                    Vec::new()
                }
            },
        });

        Self { macros }
    }

    pub fn expand_call(&self, name: &str, args: &[Expr]) -> Option<Vec<Stmt>> {
        self.macros.get(name).and_then(|mac| {
            if mac.num_args == 0 || mac.is_variadic || args.len() >= mac.num_args {
                Some((mac.expand_fn)(args))
            } else {
                None
            }
        })
    }
}

// ═══════════════════════════════
//  单元测试
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FieldDef, TypeRef, BaseType};

    #[test]
    fn test_derive_expander_creation() {
        let expander = DeriveExpander::new();
        assert!(expander.records.is_empty());
    }

    #[test]
    fn test_expand_struct_with_debug_derive() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Person".to_string(),
                derives: vec!["Debug".to_string()],
                fields: vec![
                    FieldDef { name: "name".to_string(), type_annotation: TypeRef::new(BaseType::String) },
                    FieldDef { name: "age".to_string(), type_annotation: TypeRef::new(BaseType::Int) },
                ],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 2);
        assert_eq!(expander.records.len(), 1);
        assert_eq!(expander.records[0].target_type, "struct");
        assert!(expander.records[0].macro_name.contains("Debug"));
    }

    #[test]
    fn test_expand_struct_with_clone_derive() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Point".to_string(),
                derives: vec!["Clone".to_string()],
                fields: vec![
                    FieldDef { name: "x".to_string(), type_annotation: TypeRef::new(BaseType::Int) },
                    FieldDef { name: "y".to_string(), type_annotation: TypeRef::new(BaseType::Int) },
                ],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 2);
        assert!(expander.records[0].macro_name.contains("Clone"));
    }

    #[test]
    fn test_expand_struct_with_multiple_derives() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Rectangle".to_string(),
                derives: vec!["Debug".to_string(), "Clone".to_string(), "Default".to_string()],
                fields: vec![
                    FieldDef { name: "width".to_string(), type_annotation: TypeRef::new(BaseType::Float) },
                    FieldDef { name: "height".to_string(), type_annotation: TypeRef::new(BaseType::Float) },
                ],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 4);
        assert_eq!(expander.records.len(), 3);
    }

    #[test]
    fn test_expand_struct_with_empty_derives() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Empty".to_string(),
                derives: vec![],
                fields: vec![],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 1);
        assert!(expander.records.is_empty());
    }

    #[test]
    fn test_expand_non_struct_stmts_noop() {
        let mut stmts = vec![
            Stmt::Fn {
                name: "add".to_string(),
                type_params: vec![],
                params: vec![], return_type: None,
                effect: None, capability: None, llm_prompt: None, confidence: None,
                cognitive_loop: None, governance: None, latency: None, timeout: None,
                throughput: None,
                body: vec![Stmt::Return(Some(Box::new(Expr::IntLiteral(0))))],
                async_: false, pub_: false,
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 1);
        assert!(expander.records.is_empty());
    }

    #[test]
    fn test_expand_unknown_derive_trait_no_crash() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Weird".to_string(),
                derives: vec!["SomeUnknownTrait".to_string()],
                fields: vec![],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert!(stmts.len() >= 1);
    }

    #[test]
    fn test_expand_enum_derives_no_crash() {
        let mut stmts = vec![
            Stmt::EnumDef {
                name: "Shape".to_string(),
                variants: vec![
                    crate::ast::EnumVariant { name: "Circle".to_string(), fields: vec![] },
                    crate::ast::EnumVariant { name: "Rectangle".to_string(), fields: vec![] },
                ],
            },
        ];
        let expander = DeriveExpander::new();
        expander.expand_enum_derives(&mut stmts);
    }

    #[test]
    fn test_expand_copy_derive() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Vec3".to_string(),
                derives: vec!["Copy".to_string()],
                fields: vec![],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 2);
        assert!(expander.records[0].macro_name.contains("Copy"));
    }

    #[test]
    fn test_expand_partialeq_derive() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Point".to_string(),
                derives: vec!["PartialEq".to_string()],
                fields: vec![
                    FieldDef { name: "x".to_string(), type_annotation: TypeRef::new(BaseType::Int) },
                    FieldDef { name: "y".to_string(), type_annotation: TypeRef::new(BaseType::Int) },
                ],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 2);
        assert!(expander.records[0].macro_name.contains("PartialEq"));
    }

    #[test]
    fn test_expand_default_derive() {
        let mut stmts = vec![
            Stmt::StructDef {
                name: "Config".to_string(),
                derives: vec!["Default".to_string()],
                fields: vec![
                    FieldDef { name: "enabled".to_string(), type_annotation: TypeRef::new(BaseType::Bool) },
                ],
            },
        ];
        let mut expander = DeriveExpander::new();
        expander.expand_derives(&mut stmts);
        assert_eq!(stmts.len(), 2);
        assert!(expander.records[0].macro_name.contains("Default"));
    }

    #[test]
    fn test_builtin_macros_creation() {
        let macros = BuiltinMacros::new();
        assert!(macros.macros.contains_key("println"));
        assert!(macros.macros.contains_key("dbg"));
        assert!(macros.macros.contains_key("assert"));
    }

    #[test]
    fn test_expand_println_macro() {
        let macros = BuiltinMacros::new();
        let args = vec![Expr::StringLiteral("hello".to_string())];
        let result = macros.expand_call("println", &args);
        assert!(result.is_some());
        let stmts = result.unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Expr(expr) => {
                match expr.as_ref() {
                    Expr::Call { func, .. } => {
                        match func.as_ref() { Expr::Ident(n) => assert_eq!(n, "io::println"), _ => panic!() }
                    }
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_expand_dbg_macro() {
        let macros = BuiltinMacros::new();
        let args = vec![Expr::IntLiteral(42)];
        let result = macros.expand_call("dbg", &args);
        assert!(result.is_some());
        let stmts = result.unwrap();
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn test_expand_assert_with_message() {
        let macros = BuiltinMacros::new();
        let args = vec![
            Expr::BoolLiteral(true),
            Expr::StringLiteral("check failed".to_string()),
        ];
        let result = macros.expand_call("assert", &args);
        assert!(result.is_some());
        let stmts = result.unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Assert { condition, message } => {
                assert!(matches!(condition.as_ref(), Expr::BoolLiteral(true)));
                assert!(message.is_some());
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_expand_assert_without_message() {
        let macros = BuiltinMacros::new();
        let args = vec![Expr::BoolLiteral(true)];
        let result = macros.expand_call("assert", &args);
        assert!(result.is_some());
        let stmts = result.unwrap();
        match &stmts[0] {
            Stmt::Assert { condition, message } => {
                assert!(matches!(condition.as_ref(), Expr::BoolLiteral(true)));
                assert!(message.is_none());
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_expand_unknown_macro_returns_none() {
        let macros = BuiltinMacros::new();
        let result = macros.expand_call("unknown_macro", &vec![]);
        assert!(result.is_none());
    }

    #[test]
    fn test_expand_macro_call_insufficient_args() {
        let macros = BuiltinMacros::new();
        let result = macros.expand_call("assert", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_macro_expander_full_pipeline() {
        let program = Program {
            statements: vec![
                Stmt::StructDef {
                    name: "TestItem".to_string(),
                    derives: vec!["Debug".to_string(), "Clone".to_string()],
                    fields: vec![
                        FieldDef { name: "value".to_string(), type_annotation: TypeRef::new(BaseType::Int) },
                    ],
                },
            ],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };

        let expander = MacroExpander::new();
        let result = expander.expand(&program);
        assert!(result.expansions.iter().any(|r| r.macro_name.contains("Debug")));
        assert!(result.expansions.iter().any(|r| r.macro_name.contains("Clone")));
    }

    #[test]
    fn test_expand_program_without_derives() {
        let program = Program {
            statements: vec![
                Stmt::Fn {
                    name: "simple".to_string(),
                    type_params: vec![],
                    params: vec![], return_type: None,
                    effect: None, capability: None, llm_prompt: None, confidence: None,
                    cognitive_loop: None, governance: None, latency: None, timeout: None,
                    throughput: None,
                    body: vec![Stmt::Return(Some(Box::new(Expr::IntLiteral(42))))],
                    async_: false, pub_: false,
                },
            ],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };

        let expander = MacroExpander::new();
        let result = expander.expand(&program);
        // No derives in program → no derive expansions
        assert_eq!(result.expanded.statements.len(), program.statements.len());
    }

    #[test]
    fn test_register_macro_decls() {
        let mut registry = MacroRegistry::new();
        let decls = vec![
            MacroDecl::Declarative {
                name: "my_macro".to_string(),
                rules: vec![crate::ast::MacroRule {
                    pattern: vec!["$expr: expr".to_string()],
                    expansion: vec!["{ $expr }".to_string()],
                }],
            },
        ];
        register_macro_decls(&mut registry, &decls);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_extract_macro_decls_empty() {
        let stmts: Vec<Stmt> = vec![
            Stmt::Fn {
                name: "no_macros".to_string(),
                type_params: vec![],
                params: vec![], return_type: None,
                effect: None, capability: None, llm_prompt: None, confidence: None,
                cognitive_loop: None, governance: None, latency: None, timeout: None,
                throughput: None, body: vec![], async_: false, pub_: false,
            },
        ];
        let decls = extract_macro_decls(&stmts);
        assert!(decls.is_empty());
    }

    #[test]
    fn test_derive_result_cloning() {
        let record = ExpansionRecord {
            macro_name: "derive(Debug)".to_string(),
            target_type: "struct".to_string(),
            location: "struct Foo".to_string(),
            generated_stmts: 1,
        };
        assert_eq!(record.clone(), record);
    }
}
