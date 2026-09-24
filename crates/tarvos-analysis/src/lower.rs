use crate::{module_supported, native_constant, native_function};
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use tarvos_ir::{BinaryOp, Module, Stmt, TypeContext, Value};
use tarvos_types::Type;

pub struct Lowerer {
    type_context: TypeContext,
    function_signatures: HashMap<String, (Vec<String>, Vec<Type>, Type)>,
    class_names: HashSet<String>,
    class_methods: HashMap<(String, String), String>,
    class_fields: HashMap<String, Vec<(String, Type)>>,
    object_classes: HashMap<String, String>,
    module_aliases: HashMap<String, String>,
    imported_functions: HashMap<String, String>,
    imported_constants: HashMap<String, Value>,
    loop_depth: usize,
    current_class: Option<String>,
}

impl Lowerer {
    pub fn new() -> Self {
        Self {
            type_context: TypeContext::new(),
            function_signatures: HashMap::new(),
            class_names: HashSet::new(),
            class_methods: HashMap::new(),
            class_fields: HashMap::new(),
            object_classes: HashMap::new(),
            module_aliases: HashMap::new(),
            imported_functions: HashMap::new(),
            imported_constants: HashMap::new(),
            loop_depth: 0,
            current_class: None,
        }
    }

    pub fn lower_module(&mut self, module: &tarvos_ast::Module) -> Result<Module> {
        let mut statements = Vec::new();

        for stmt in &module.body {
            if let tarvos_ast::Stmt::ClassDef { name, body, .. } = stmt {
                self.class_names.insert(name.clone());
                for member in body {
                    if let tarvos_ast::Stmt::FunctionDef { name: method, .. } = member {
                        self.class_methods.insert(
                            (name.clone(), method.clone()),
                            format!("{}_{}", rust_identifier(name), rust_identifier(method)),
                        );
                    }
                }
                self.class_fields
                    .insert(name.clone(), collect_class_fields(body));
            }
        }

        for stmt in &module.body {
            if let tarvos_ast::Stmt::FunctionDef {
                name,
                args,
                arg_annotations,
                returns,
                ..
            } = stmt
            {
                let param_names = args.clone();
                let param_types: Vec<Type> = args
                    .iter()
                    .zip(arg_annotations.iter())
                    .map(|(_, annotation)| {
                        annotation
                            .as_deref()
                            .and_then(|s| self.parse_type_annotation(s))
                            .unwrap_or(Type::Unknown)
                    })
                    .collect();
                let return_type = returns
                    .as_ref()
                    .and_then(|s| self.parse_type_annotation(s))
                    .unwrap_or(Type::None);
                self.function_signatures
                    .insert(name.clone(), (param_names, param_types, return_type));
            }
        }

        for stmt in &module.body {
            if let tarvos_ast::Stmt::ClassDef { name, body, .. } = stmt {
                statements.extend(self.lower_class(name, body)?);
                continue;
            }
            statements.push(self.lower_stmt(stmt)?);
        }

        Ok(Module { statements })
    }

    fn lower_stmt(&mut self, stmt: &tarvos_ast::Stmt) -> Result<Stmt> {
        match stmt {
            tarvos_ast::Stmt::Import { names } => {
                for name in names {
                    if !module_supported(&name.name) {
                        bail!(
                            "import '{}' is not supported by the native backend yet; \
                             supported native modules: math, time, os.path",
                            name.name
                        );
                    }
                    let binding = name
                        .asname
                        .as_deref()
                        .unwrap_or_else(|| name.name.split('.').next().unwrap_or(&name.name));
                    self.module_aliases
                        .insert(binding.to_string(), name.name.clone());
                    if name.asname.is_none() && name.name.contains('.') {
                        self.module_aliases
                            .insert(name.name.clone(), name.name.clone());
                    }
                }
                Ok(Stmt::Expr(Value::Bool(true)))
            }
            tarvos_ast::Stmt::ImportFrom { module, names } => {
                if !module_supported(module) {
                    bail!(
                        "from '{}' import ... is not supported by the native backend yet; \
                         supported native modules: math, time, os.path",
                        module
                    );
                }
                for name in names {
                    let alias = name.asname.as_ref().unwrap_or(&name.name);
                    if let Some(function) = native_function(module, &name.name) {
                        self.imported_functions
                            .insert(alias.clone(), function.rust_name.to_string());
                        self.type_context
                            .declare(alias.clone(), function.return_type);
                    } else if let Some(value) = native_constant(module, &name.name) {
                        self.imported_constants
                            .insert(alias.clone(), Value::Float(value));
                        self.type_context.declare(alias.clone(), Type::Float);
                    } else {
                        bail!(
                            "from '{}' import '{}' is not supported by the native backend yet",
                            module,
                            name.name
                        );
                    }
                }
                Ok(Stmt::Expr(Value::Bool(true)))
            }
            tarvos_ast::Stmt::Assign { target, value } => match target {
                tarvos_ast::Expr::Name { id } => {
                    let value_ir = self.lower_expr(value)?;
                    let ty = self.value_type(&value_ir)?;
                    if let Some(class_name) = self.constructor_class(&value_ir) {
                        self.object_classes.insert(id.clone(), class_name);
                    }

                    if self.type_context.lookup(id).is_some() {
                        Ok(Stmt::Assign {
                            name: id.clone(),
                            value: value_ir,
                        })
                    } else {
                        self.type_context.declare(id.clone(), ty.clone());
                        Ok(Stmt::Let {
                            name: id.clone(),
                            ty,
                            value: value_ir,
                        })
                    }
                }
                tarvos_ast::Expr::Subscript {
                    value: container,
                    index,
                } => {
                    let tarvos_ast::Expr::Name { id } = container.as_ref() else {
                        bail!("subscript assignment target container must be a variable name");
                    };
                    let index_ir = self.lower_expr(index)?;
                    let value_ir = self.lower_expr(value)?;
                    Ok(Stmt::IndexAssign {
                        target: id.clone(),
                        index: index_ir,
                        value: value_ir,
                    })
                }
                tarvos_ast::Expr::Attribute { value: obj, attr } => {
                    let obj_ir = self.lower_expr(obj)?;
                    let val_ir = self.lower_expr(value)?;
                    Ok(Stmt::FieldAssign {
                        object: obj_ir,
                        field: attr.clone(),
                        value: val_ir,
                    })
                }
                _ => bail!("assignment target must be a variable name or subscript"),
            },

            tarvos_ast::Stmt::AugAssign {
                target,
                operator,
                value,
            } => {
                let tarvos_ast::Expr::Name { id } = target else {
                    bail!("augmented assignment target must be a variable name");
                };
                // Desugar: x += e  →  x = x + e
                let left_expr = tarvos_ast::Expr::Name { id: id.clone() };
                let synthetic = tarvos_ast::Expr::Binary {
                    left: Box::new(left_expr),
                    operator: operator.clone(),
                    right: Box::new(value.clone()),
                };
                let value_ir = self.lower_expr(&synthetic)?;
                Ok(Stmt::Assign {
                    name: id.clone(),
                    value: value_ir,
                })
            }

            tarvos_ast::Stmt::AnnAssign {
                target,
                annotation,
                value,
            } => {
                let tarvos_ast::Expr::Name { id } = target else {
                    bail!("annotated assignment target must be a variable name");
                };
                let ty = self
                    .parse_type_annotation(annotation)
                    .unwrap_or(tarvos_types::Type::Unknown);
                if let Some(val) = value {
                    let value_ir = self.lower_expr(val)?;
                    self.type_context.declare(id.clone(), ty.clone());
                    Ok(Stmt::Let {
                        name: id.clone(),
                        ty,
                        value: value_ir,
                    })
                } else {
                    // Just a declaration hint — register the type, emit nothing meaningful
                    self.type_context.declare(id.clone(), ty.clone());
                    Ok(Stmt::Expr(Value::Bool(true)))
                }
            }

            tarvos_ast::Stmt::Global { .. } | tarvos_ast::Stmt::Nonlocal { .. } => {
                // Silently accept global/nonlocal declarations
                Ok(Stmt::Expr(Value::Bool(true)))
            }

            tarvos_ast::Stmt::Delete { .. } => {
                // del statement — silently ignore (Rust has no explicit free)
                Ok(Stmt::Expr(Value::Bool(true)))
            }

            tarvos_ast::Stmt::Assert { test, msg } => {
                let test_ir = self.lower_expr(test)?;
                let msg_ir = msg
                    .as_ref()
                    .map(|m| self.lower_expr(m))
                    .transpose()?
                    .unwrap_or(Value::String("assertion failed".to_string()));
                // Emit as: if !(test) { panic!(msg) }
                Ok(Stmt::If {
                    test: Value::Unary {
                        op: tarvos_ir::UnaryOp::Not,
                        operand: Box::new(test_ir),
                        ty: tarvos_types::Type::Bool,
                    },
                    body: vec![Stmt::Raise(Some(msg_ir))],
                    orelse: vec![],
                })
            }

            tarvos_ast::Stmt::ClassDef { .. } => {
                bail!("class definitions are lowered at module scope")
            }

            tarvos_ast::Stmt::Expr { value } => {
                match value {
                    tarvos_ast::Expr::Call {
                        function,
                        args,
                        keywords,
                    } => {
                        if let tarvos_ast::Expr::Name { id } = function.as_ref() {
                            if id == "print" {
                                if !keywords.is_empty() {
                                    // Allow sep/end kwargs but ignore them
                                    let values_ir: Result<Vec<_>> =
                                        args.iter().map(|a| self.lower_expr(a)).collect();
                                    return Ok(Stmt::Print(values_ir?));
                                }
                                let values_ir: Result<Vec<_>> =
                                    args.iter().map(|a| self.lower_expr(a)).collect();
                                return Ok(Stmt::Print(values_ir?));
                            }
                        }

                        let function_name = match function.as_ref() {
                            tarvos_ast::Expr::Name { id } => id.clone(),
                            tarvos_ast::Expr::Attribute { value: obj, attr } => {
                                // obj.method(...) call as statement
                                if let tarvos_ast::Expr::Name { id } = obj.as_ref() {
                                    return self.lower_method_call_stmt(id, attr, args);
                                }
                                bail!("method call on complex expression not yet supported as statement");
                            }
                            _ => bail!("only direct function calls are supported"),
                        };
                        let args_ir = self.lower_call_args(&function_name, args, keywords)?;
                        let return_type = self.infer_call_return_type(&function_name, &args_ir)?;
                        Ok(Stmt::Expr(Value::Call {
                            function: function_name,
                            args: args_ir,
                            return_type,
                        }))
                    }
                    tarvos_ast::Expr::MethodCall {
                        object,
                        method,
                        args,
                    } => {
                        if let Some(module_name) = module_reference(object) {
                            if let Some(module) = self.module_aliases.get(&module_name) {
                                if let Some(function) = native_function(module, method) {
                                    let args_ir = args
                                        .iter()
                                        .map(|arg| self.lower_expr(arg))
                                        .collect::<Result<Vec<_>>>()?;
                                    return Ok(Stmt::Expr(Value::Call {
                                        function: function.rust_name.to_string(),
                                        args: args_ir,
                                        return_type: function.return_type,
                                    }));
                                }
                            }
                        }
                        if let tarvos_ast::Expr::Name { id } = object.as_ref() {
                            return self.lower_method_call_stmt(id, method, args);
                        }
                        bail!("method call on complex expression not yet supported as statement");
                    }

                    _ => {
                        let val = self.lower_expr(value)?;
                        Ok(Stmt::Expr(val))
                    }
                }
            }

            tarvos_ast::Stmt::If { test, body, orelse } => {
                let test_ir = self.lower_expr(test)?;
                let body_ir: Result<Vec<_>> = body.iter().map(|s| self.lower_stmt(s)).collect();
                let orelse_ir: Result<Vec<_>> = orelse.iter().map(|s| self.lower_stmt(s)).collect();

                Ok(Stmt::If {
                    test: test_ir,
                    body: body_ir?,
                    orelse: orelse_ir?,
                })
            }

            tarvos_ast::Stmt::While { test, body } => {
                let test_ir = self.lower_expr(test)?;
                self.loop_depth += 1;
                let body_ir: Result<Vec<_>> = body.iter().map(|s| self.lower_stmt(s)).collect();
                self.loop_depth -= 1;

                Ok(Stmt::While {
                    test: test_ir,
                    body: body_ir?,
                })
            }

            tarvos_ast::Stmt::For { target, iter, body } => {
                let tarvos_ast::Expr::Name { id } = target else {
                    bail!("for loop target must be a variable name");
                };

                let iter_ir = self.lower_expr(iter)?;
                let iter_type = self.value_type(&iter_ir)?;

                let element_type = match &iter_type {
                    tarvos_types::Type::Array(elem_type) => (**elem_type).clone(),
                    tarvos_types::Type::Dict { key, .. } => (**key).clone(),
                    tarvos_types::Type::String => tarvos_types::Type::String,
                    tarvos_types::Type::Tuple(types) => types
                        .first()
                        .cloned()
                        .unwrap_or(tarvos_types::Type::Unknown),
                    _ => match &iter_ir {
                        Value::Call {
                            function,
                            return_type: tarvos_types::Type::Array(elem_type),
                            ..
                        } if function == "range" => (**elem_type).clone(),
                        Value::List { element_type, .. } => element_type.clone(),
                        Value::Name(name) => match self.type_context.lookup(name).cloned() {
                            Some(tarvos_types::Type::Array(inner)) => *inner,
                            _ => tarvos_types::Type::Unknown,
                        },
                        _ => tarvos_types::Type::Unknown,
                    },
                };
                self.type_context.declare(id.clone(), element_type.clone());

                self.loop_depth += 1;
                let body_ir: Result<Vec<_>> = body.iter().map(|s| self.lower_stmt(s)).collect();
                self.loop_depth -= 1;

                Ok(Stmt::For {
                    target: id.clone(),
                    iter: iter_ir,
                    iter_type,
                    body: body_ir?,
                })
            }

            tarvos_ast::Stmt::FunctionDef {
                name,
                args,
                arg_annotations,
                body,
                returns,
            } => {
                let parent_context = self.type_context.clone();
                let parent_loop_depth = self.loop_depth;
                let mut function_context = tarvos_ir::TypeContext::new();
                let mut params = Vec::new();

                for (arg_name, arg_annotation) in args.iter().zip(arg_annotations.iter()) {
                    let arg_type = arg_annotation
                        .as_deref()
                        .and_then(|s| self.parse_type_annotation(s))
                        .unwrap_or(tarvos_types::Type::Unknown);
                    function_context.declare(arg_name.clone(), arg_type.clone());
                    params.push((arg_name.clone(), arg_type));
                }

                self.type_context = function_context;
                self.loop_depth = 0;
                let body_ir: Result<Vec<_>> = body.iter().map(|s| self.lower_stmt(s)).collect();
                let return_type = returns
                    .as_ref()
                    .and_then(|s| self.parse_type_annotation(s))
                    .or_else(|| self.infer_return_type_from_body(body))
                    .unwrap_or(tarvos_types::Type::None);
                self.type_context = parent_context;
                self.loop_depth = parent_loop_depth;

                let param_types: Vec<tarvos_types::Type> =
                    params.iter().map(|(_, ty)| ty.clone()).collect();
                self.function_signatures.insert(
                    name.clone(),
                    (args.clone(), param_types.clone(), return_type.clone()),
                );

                Ok(Stmt::Function {
                    name: name.clone(),
                    params,
                    return_type,
                    body: body_ir?,
                })
            }

            tarvos_ast::Stmt::Return { value } => {
                let value_ir = value.as_ref().map(|v| self.lower_expr(v)).transpose()?;
                Ok(Stmt::Return(value_ir))
            }
            tarvos_ast::Stmt::Break => {
                if self.loop_depth == 0 {
                    bail!("break is only supported inside a loop");
                }
                Ok(Stmt::Break)
            }
            tarvos_ast::Stmt::Continue => {
                if self.loop_depth == 0 {
                    bail!("continue is only supported inside a loop");
                }
                Ok(Stmt::Continue)
            }
            tarvos_ast::Stmt::Raise { exc } => {
                let exc_ir = exc.as_ref().map(|e| self.lower_expr(e)).transpose()?;
                Ok(Stmt::Raise(exc_ir))
            }
            tarvos_ast::Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let body_ir = body
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                let mut handlers_ir = Vec::new();
                for h in handlers {
                    let exc_type = h.exc_type.as_ref().map(|e| match e {
                        tarvos_ast::Expr::Name { id } => id.clone(),
                        tarvos_ast::Expr::Call { function, .. } => match function.as_ref() {
                            tarvos_ast::Expr::Name { id } => id.clone(),
                            _ => "Exception".to_string(),
                        },
                        _ => "Exception".to_string(),
                    });
                    let saved_ctx = self.type_context.clone();
                    if let Some(ref var_name) = h.name {
                        self.type_context
                            .declare(var_name.clone(), tarvos_types::Type::String);
                    }
                    let handler_body = h
                        .body
                        .iter()
                        .map(|s| self.lower_stmt(s))
                        .collect::<Result<Vec<_>>>()?;
                    self.type_context = saved_ctx;
                    handlers_ir.push(tarvos_ir::ExceptHandler {
                        name: h.name.clone(),
                        exc_type,
                        body: handler_body,
                    });
                }
                let orelse_ir = orelse
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                let finalbody_ir = finalbody
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::Try {
                    body: body_ir,
                    handlers: handlers_ir,
                    orelse: orelse_ir,
                    finalbody: finalbody_ir,
                })
            }
            tarvos_ast::Stmt::With { items, body } => {
                let mut items_ir = Vec::new();
                let saved_ctx = self.type_context.clone();
                for item in items {
                    let ctx_val = self.lower_expr(&item.context_expr)?;
                    let target_name = item.optional_vars.as_ref().and_then(|v| match v {
                        tarvos_ast::Expr::Name { id } => Some(id.clone()),
                        _ => None,
                    });
                    if let Some(ref name) = target_name {
                        self.type_context
                            .declare(name.clone(), tarvos_types::Type::String);
                    }
                    items_ir.push(tarvos_ir::WithItem {
                        context_expr: ctx_val,
                        target: target_name,
                    });
                }
                let body_ir = body
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                self.type_context = saved_ctx;
                Ok(Stmt::With {
                    items: items_ir,
                    body: body_ir,
                })
            }
        }
    }

    /// Lower a method call as a statement (handles common list/dict/string methods)
    fn lower_method_call_stmt(
        &mut self,
        obj_name: &str,
        method: &str,
        args: &[tarvos_ast::Expr],
    ) -> Result<Stmt> {
        if let Some(class_name) = self.object_classes.get(obj_name) {
            if let Some(function_name) = self
                .class_methods
                .get(&(class_name.clone(), method.to_string()))
                .cloned()
            {
                let args_ir = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Result<Vec<_>>>()?;
                let return_type = self.infer_call_return_type(&function_name, &args_ir)?;
                return Ok(Stmt::Expr(Value::Call {
                    function: format!("__tarvos_mut_call_{}", function_name),
                    args: {
                        let mut receiver = vec![Value::Name(obj_name.to_string())];
                        receiver.extend(args_ir);
                        receiver
                    },
                    return_type,
                }));
            }
        }
        match method {
            "append" => {
                if args.len() != 1 {
                    bail!("list.append() takes exactly 1 argument");
                }
                let value_ir = self.lower_expr(&args[0])?;
                let obj_type = self
                    .type_context
                    .lookup(obj_name)
                    .cloned()
                    .unwrap_or(tarvos_types::Type::Unknown);
                if let tarvos_types::Type::Array(_) = obj_type {
                    return Ok(Stmt::ListAppend {
                        target: obj_name.to_string(),
                        value: value_ir,
                    });
                }
                // Fallback: generic append call
                Ok(Stmt::Expr(Value::Call {
                    function: format!("{}_append", obj_name),
                    args: vec![value_ir],
                    return_type: tarvos_types::Type::None,
                }))
            }
            "extend" | "insert" | "remove" | "pop" | "sort" | "reverse" | "clear" => {
                let args_ir: Result<Vec<_>> = args.iter().map(|a| self.lower_expr(a)).collect();
                Ok(Stmt::Expr(Value::Call {
                    function: format!("{}_{}", obj_name, method),
                    args: args_ir?,
                    return_type: tarvos_types::Type::None,
                }))
            }
            "update" | "setdefault" => {
                let args_ir: Result<Vec<_>> = args.iter().map(|a| self.lower_expr(a)).collect();
                Ok(Stmt::Expr(Value::Call {
                    function: format!("{}_{}", obj_name, method),
                    args: args_ir?,
                    return_type: tarvos_types::Type::None,
                }))
            }
            _ => {
                // Generic method call — emit as a function call with obj as first arg
                let mut args_ir = vec![Value::Name(obj_name.to_string())];
                for a in args {
                    args_ir.push(self.lower_expr(a)?);
                }
                Ok(Stmt::Expr(Value::Call {
                    function: format!("__method_{}_{}", obj_name, method),
                    args: args_ir,
                    return_type: tarvos_types::Type::None,
                }))
            }
        }
    }

    fn lower_class(&mut self, class_name: &str, body: &[tarvos_ast::Stmt]) -> Result<Vec<Stmt>> {
        let mut lowered = Vec::new();
        let previous_class = self.current_class.replace(class_name.to_string());
        for stmt in body {
            let tarvos_ast::Stmt::FunctionDef {
                name: method_name,
                args,
                arg_annotations,
                body: method_body,
                returns,
            } = stmt
            else {
                continue;
            };

            if method_name == "__init__" {
                let synthetic = tarvos_ast::Stmt::FunctionDef {
                    name: format!("__tarvos_ctor_{}", rust_identifier(class_name)),
                    args: args.iter().skip(1).cloned().collect(),
                    arg_annotations: arg_annotations.iter().skip(1).cloned().collect(),
                    body: method_body.clone(),
                    returns: Some(class_name.to_string()),
                };
                lowered.push(self.lower_stmt(&synthetic)?);
                continue;
            }

            let synthetic = tarvos_ast::Stmt::FunctionDef {
                name: format!(
                    "{}_{}",
                    rust_identifier(class_name),
                    rust_identifier(method_name)
                ),
                args: args.clone(),
                arg_annotations: {
                    let mut annotations = arg_annotations.clone();
                    if annotations.is_empty() {
                        annotations.push(Some(class_name.to_string()));
                    } else {
                        annotations[0] = Some(class_name.to_string());
                    }
                    annotations
                },
                body: method_body.clone(),
                returns: returns.clone(),
            };
            lowered.push(self.lower_stmt(&synthetic)?);
        }
        lowered.insert(
            0,
            Stmt::StructDef {
                name: class_name.to_string(),
                fields: self
                    .class_fields
                    .get(class_name)
                    .cloned()
                    .unwrap_or_default(),
            },
        );
        self.current_class = previous_class;
        Ok(lowered)
    }

    fn constructor_class(&self, value: &Value) -> Option<String> {
        let Value::Call {
            function,
            return_type,
            ..
        } = value
        else {
            return None;
        };
        if let Type::Object(class_name) = return_type {
            return Some(class_name.clone());
        }
        function
            .strip_prefix("__tarvos_ctor_")
            .map(ToOwned::to_owned)
    }

    fn object_class_for_value(&self, value: &Value) -> Option<String> {
        match value {
            Value::Name(name) if name == "self" => self.current_class.clone(),
            Value::Name(name) => self.object_classes.get(name).cloned(),
            Value::Call {
                return_type: Type::Object(name),
                ..
            } => Some(name.clone()),
            _ => None,
        }
    }

    fn lower_expr(&mut self, expr: &tarvos_ast::Expr) -> Result<Value> {
        match expr {
            tarvos_ast::Expr::Int { value } => Ok(Value::Int(*value)),
            tarvos_ast::Expr::BigInt { value } => {
                let value = value.parse::<u128>().map_err(|_| {
                    anyhow::anyhow!(
                        "integer literal `{value}` exceeds Tarvos native integer support; \
                         use --python-fallback for arbitrary-precision Python integers"
                    )
                })?;
                Ok(Value::Int128(value))
            }
            tarvos_ast::Expr::Float { value } => Ok(Value::Float(*value)),
            tarvos_ast::Expr::String { value } => Ok(Value::String(value.clone())),
            tarvos_ast::Expr::Bool { value } => Ok(Value::Bool(*value)),
            tarvos_ast::Expr::Name { id } => Ok(self
                .imported_constants
                .get(id)
                .cloned()
                .unwrap_or_else(|| Value::Name(id.clone()))),
            tarvos_ast::Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left_ir = self.lower_expr(left)?;
                let right_ir = self.lower_expr(right)?;
                let left_type = self.value_type(&left_ir)?;
                let right_type = self.value_type(&right_ir)?;

                let op = self.parse_binary_op(operator)?;
                let result_type = self.infer_binary_result_type(&left_type, &op, &right_type)?;

                Ok(Value::Binary {
                    left: Box::new(left_ir),
                    op,
                    right: Box::new(right_ir),
                    ty: result_type,
                })
            }
            tarvos_ast::Expr::Compare {
                left,
                operators,
                comparators,
            } => {
                // For now, only support single comparisons
                if operators.len() != 1 || comparators.len() != 1 {
                    bail!("chained comparisons not yet supported");
                }

                let left_ir = self.lower_expr(left)?;
                let right_ir = self.lower_expr(&comparators[0])?;

                let op = self.parse_compare_op(&operators[0])?;

                Ok(Value::Binary {
                    left: Box::new(left_ir),
                    op,
                    right: Box::new(right_ir),
                    ty: Type::Bool,
                })
            }
            tarvos_ast::Expr::Call {
                function,
                args,
                keywords,
            } => {
                if let tarvos_ast::Expr::Name { id } = function.as_ref() {
                    if self.class_names.contains(id) {
                        let args_ir = self.lower_call_args(id, args, keywords)?;
                        return Ok(Value::Call {
                            function: format!("__tarvos_ctor_{}", rust_identifier(id)),
                            args: args_ir,
                            return_type: Type::Object(id.clone()),
                        });
                    }
                    if id == "__import__"
                        && args.len() == 1
                        && matches!(&args[0], tarvos_ast::Expr::String { value } if value == "time")
                    {
                        return Ok(Value::String("time".into()));
                    }
                    let function_name = self
                        .imported_functions
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| id.clone());
                    let args_ir = self.lower_call_args(&function_name, args, keywords)?;
                    if function_name == "range"
                        && args_ir.iter().any(|arg| matches!(arg, Value::Int128(_)))
                    {
                        bail!(
                            "range() bounds above i64 are not executable as a native loop; \
                             use --python-fallback or reduce the workload before compiling"
                        );
                    }
                    let return_type = self.infer_call_return_type(&function_name, &args_ir)?;

                    Ok(Value::Call {
                        function: function_name,
                        args: args_ir,
                        return_type,
                    })
                } else {
                    bail!("only direct function calls are supported")
                }
            }
            tarvos_ast::Expr::MethodCall {
                object,
                method,
                args,
            } => {
                if let Some(module_name) = module_reference(object) {
                    if let Some(module) = self.module_aliases.get(&module_name) {
                        if let Some(function) = native_function(module, method) {
                            let args_ir = args
                                .iter()
                                .map(|arg| self.lower_expr(arg))
                                .collect::<Result<Vec<_>>>()?;
                            return Ok(Value::Call {
                                function: function.rust_name.to_string(),
                                args: args_ir,
                                return_type: function.return_type,
                            });
                        }
                    }
                }
                if let tarvos_ast::Expr::Name { id } = object.as_ref() {
                    if let Some(module) = self.module_aliases.get(id) {
                        if let Some(function) = native_function(module, method) {
                            let args_ir = args
                                .iter()
                                .map(|arg| self.lower_expr(arg))
                                .collect::<Result<Vec<_>>>()?;
                            return Ok(Value::Call {
                                function: function.rust_name.to_string(),
                                args: args_ir,
                                return_type: function.return_type,
                            });
                        }
                        bail!(
                            "module '{}.{}' is not supported by the native backend yet",
                            module,
                            method
                        );
                    }
                }
                let object_name = match object.as_ref() {
                    tarvos_ast::Expr::Name { id } => id,
                    _ => bail!("method calls on complex expressions are not supported"),
                };
                let class_name =
                    self.object_classes
                        .get(object_name)
                        .cloned()
                        .ok_or_else(|| {
                            anyhow::anyhow!("unknown object `{object_name}` for method `{method}`")
                        })?;
                let function_name = self
                    .class_methods
                    .get(&(class_name, method.clone()))
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("unknown method `{method}`"))?;
                let mut args_ir = vec![Value::Name(object_name.clone())];
                args_ir.extend(
                    args.iter()
                        .map(|arg| self.lower_expr(arg))
                        .collect::<Result<Vec<_>>>()?,
                );
                let return_type = self.infer_call_return_type(&function_name, &args_ir)?;
                Ok(Value::Call {
                    function: format!("__tarvos_mut_call_{}", function_name),
                    args: args_ir,
                    return_type,
                })
            }
            tarvos_ast::Expr::List { elements } => {
                let elements_ir: Result<Vec<_>> =
                    elements.iter().map(|e| self.lower_expr(e)).collect();
                let elements_ir = elements_ir?;

                let element_type = if let Some(first) = elements_ir.first() {
                    self.value_type(first)?
                } else {
                    Type::Unknown
                };

                Ok(Value::List {
                    elements: elements_ir,
                    element_type,
                })
            }
            tarvos_ast::Expr::Tuple { elements } => {
                let values: Vec<Value> = elements
                    .iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<_>>()?;
                let element_types = values
                    .iter()
                    .map(|value| self.value_type(value))
                    .collect::<Result<_>>()?;
                Ok(Value::Tuple {
                    elements: values,
                    element_types,
                })
            }
            tarvos_ast::Expr::FormatString { parts } => {
                let parts = parts
                    .iter()
                    .map(|part| match part {
                        tarvos_ast::FormatPart::Literal { value } => {
                            Ok(tarvos_ir::FormatPart::Literal(value.clone()))
                        }
                        tarvos_ast::FormatPart::Value {
                            value,
                            format_spec,
                            conversion,
                        } => Ok(tarvos_ir::FormatPart::Value {
                            value: Box::new(self.lower_expr(value)?),
                            format_spec: format_spec.clone(),
                            conversion: conversion.clone(),
                        }),
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::FormatString { parts })
            }
            tarvos_ast::Expr::Dict { keys, values } => {
                if keys.len() != values.len() {
                    bail!("dictionary literal has mismatched key/value counts");
                }
                let keys_ir: Vec<Value> = keys
                    .iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<_>>()?;
                let values_ir: Vec<Value> = values
                    .iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<_>>()?;
                let key_type = keys_ir
                    .first()
                    .map(|v| self.value_type(v))
                    .transpose()?
                    .unwrap_or(Type::Unknown);
                let value_type = values_ir
                    .first()
                    .map(|v| self.value_type(v))
                    .transpose()?
                    .unwrap_or(Type::Unknown);
                if !matches!(
                    key_type,
                    Type::Int | Type::Float | Type::Bool | Type::String | Type::Unknown
                ) {
                    bail!("dictionary keys must have a statically supported scalar type");
                }
                for key in &keys_ir {
                    let ty = self.value_type(key)?;
                    if ty != key_type && ty != Type::Unknown && key_type != Type::Unknown {
                        bail!("dictionary keys must have one consistent type");
                    }
                }
                for value in &values_ir {
                    let ty = self.value_type(value)?;
                    if ty != value_type && ty != Type::Unknown && value_type != Type::Unknown {
                        bail!("dictionary values must have one consistent type");
                    }
                }
                Ok(Value::Dict {
                    keys: keys_ir,
                    values: values_ir,
                    key_type,
                    value_type,
                })
            }
            tarvos_ast::Expr::Unary { operator, operand } => {
                let operand_ir = self.lower_expr(operand)?;
                let op_type = self.value_type(&operand_ir)?;
                let op = match operator.as_str() {
                    "usub" | "-" => tarvos_ir::UnaryOp::Neg,
                    "not" => tarvos_ir::UnaryOp::Not,
                    _ => bail!("unsupported unary operator: {}", operator),
                };
                let res_type = match op {
                    tarvos_ir::UnaryOp::Neg => op_type,
                    tarvos_ir::UnaryOp::Not => Type::Bool,
                };
                Ok(Value::Unary {
                    op,
                    operand: Box::new(operand_ir),
                    ty: res_type,
                })
            }
            tarvos_ast::Expr::Slice { lower, upper, step } => {
                let lower_ir = lower.as_ref().map(|l| self.lower_expr(l)).transpose()?;
                let upper_ir = upper.as_ref().map(|u| self.lower_expr(u)).transpose()?;
                let step_ir = step.as_ref().map(|s| self.lower_expr(s)).transpose()?;
                Ok(Value::Slice {
                    container: Box::new(Value::List {
                        elements: vec![],
                        element_type: Type::Unknown,
                    }),
                    lower: lower_ir.map(Box::new),
                    upper: upper_ir.map(Box::new),
                    step: step_ir.map(Box::new),
                    container_type: Type::Unknown,
                })
            }
            tarvos_ast::Expr::Subscript { value, index } => {
                let container_ir = self.lower_expr(value)?;
                let container_type = self.value_type(&container_ir)?;

                if let tarvos_ast::Expr::Slice { lower, upper, step } = index.as_ref() {
                    let lower_ir = lower.as_ref().map(|l| self.lower_expr(l)).transpose()?;
                    let upper_ir = upper.as_ref().map(|u| self.lower_expr(u)).transpose()?;
                    let step_ir = step.as_ref().map(|s| self.lower_expr(s)).transpose()?;
                    return Ok(Value::Slice {
                        container: Box::new(container_ir),
                        lower: lower_ir.map(Box::new),
                        upper: upper_ir.map(Box::new),
                        step: step_ir.map(Box::new),
                        container_type,
                    });
                }

                let index_ir = self.lower_expr(index)?;

                // Infer element type from the container
                let element_type = match container_type.clone() {
                    Type::Array(inner) => *inner,
                    Type::Dict { key, value } => {
                        let index_type = self.value_type(&index_ir)?;
                        if index_type != *key
                            && index_type != Type::Unknown
                            && *key != Type::Unknown
                        {
                            bail!(
                                "dictionary lookup key type mismatch: expected {}, got {}",
                                key,
                                index_type
                            );
                        }
                        *value
                    }
                    _ => Type::Unknown,
                };

                Ok(Value::Index {
                    container: Box::new(container_ir),
                    index: Box::new(index_ir),
                    element_type,
                    container_type,
                })
            }
            tarvos_ast::Expr::None => {
                bail!(
                    "None is not in the compilable subset. \
                     Tarvos targets statically-typed code; \
                     use typed functions and avoid None values."
                )
            }
            tarvos_ast::Expr::Attribute { value, attr } => {
                if let tarvos_ast::Expr::Name { id } = value.as_ref() {
                    if let Some(module) = self.module_aliases.get(id) {
                        if let Some(constant) = native_constant(module, attr) {
                            return Ok(Value::Float(constant));
                        }
                        bail!(
                            "module '{}.{}' is not supported by the native backend yet",
                            module,
                            attr
                        );
                    }
                }
                let inner = self.lower_expr(value)?;
                if let Some(class_name) = self.object_class_for_value(&inner) {
                    let field_type = self
                        .class_fields
                        .get(&class_name)
                        .and_then(|fields| fields.iter().find(|(name, _)| name == attr))
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or(Type::Unknown);
                    return Ok(Value::Field {
                        object: Box::new(inner),
                        field: attr.clone(),
                        ty: field_type,
                    });
                }
                match &inner {
                    Value::Name(id) => Ok(Value::Name(format!("{}_{}", id, attr))),
                    _ => Ok(Value::Call {
                        function: format!("__getattr_{}", attr),
                        args: vec![inner],
                        return_type: Type::Unknown,
                    }),
                }
            }
            tarvos_ast::Expr::BoolOp { operator, values } => {
                if values.is_empty() {
                    return Ok(Value::Bool(false));
                }
                let op = match operator.as_str() {
                    "and" | "And" => BinaryOp::And,
                    "or" | "Or" => BinaryOp::Or,
                    _ => BinaryOp::And,
                };
                let mut current = self.lower_expr(&values[0])?;
                for val in &values[1..] {
                    let next = self.lower_expr(val)?;
                    current = Value::Binary {
                        left: Box::new(current),
                        op,
                        right: Box::new(next),
                        ty: Type::Bool,
                    };
                }
                Ok(current)
            }
            tarvos_ast::Expr::IfExp { test, body, orelse } => {
                let test_ir = self.lower_expr(test)?;
                let body_ir = self.lower_expr(body)?;
                let orelse_ir = self.lower_expr(orelse)?;
                let body_type = self.value_type(&body_ir)?;
                Ok(Value::Call {
                    function: "__ternary".into(),
                    args: vec![test_ir, body_ir, orelse_ir],
                    return_type: body_type,
                })
            }
            tarvos_ast::Expr::Lambda { .. } => {
                bail!("unsupported feature: lambda expressions are not yet supported in native compiler")
            }
            tarvos_ast::Expr::ListComp {
                elt,
                target,
                iter,
                condition,
            } => {
                let iter_ir = self.lower_expr(iter)?;
                self.type_context.declare(target.clone(), Type::Int);
                let elt_ir = self.lower_expr(elt)?;
                let condition_ir = condition
                    .as_ref()
                    .map(|condition| self.lower_expr(condition))
                    .transpose()?;
                let elt_type = self.value_type(&elt_ir)?;
                Ok(Value::ListComp {
                    target: target.clone(),
                    iter: Box::new(iter_ir),
                    element: Box::new(elt_ir),
                    condition: condition_ir.map(Box::new),
                    element_type: elt_type,
                })
            }
            tarvos_ast::Expr::Set { elements } => {
                let mut elements_ir = Vec::new();
                for e in elements {
                    elements_ir.push(self.lower_expr(e)?);
                }
                let element_type = if let Some(first) = elements_ir.first() {
                    self.value_type(first)?
                } else {
                    Type::Unknown
                };
                Ok(Value::List {
                    elements: elements_ir,
                    element_type,
                })
            }
            tarvos_ast::Expr::Starred { value } => self.lower_expr(value),
        }
    }

    fn lower_call_args(
        &mut self,
        function: &str,
        args: &[tarvos_ast::Expr],
        keywords: &[tarvos_ast::Keyword],
    ) -> Result<Vec<Value>> {
        if keywords.is_empty() {
            return args.iter().map(|arg| self.lower_expr(arg)).collect();
        }
        let Some((param_names, _, _)) = self.function_signatures.get(function).cloned() else {
            bail!("keyword arguments require a known function signature");
        };
        if args.len() > param_names.len() {
            bail!("too many arguments for {}", function);
        }
        let mut slots: Vec<Option<Value>> = (0..param_names.len()).map(|_| None).collect();
        for (index, arg) in args.iter().enumerate() {
            slots[index] = Some(self.lower_expr(arg)?);
        }
        for keyword in keywords {
            let Some(name) = keyword.arg.as_ref() else {
                bail!("**kwargs expansion is not supported");
            };
            let Some(index) = param_names.iter().position(|param| param == name) else {
                bail!("unknown keyword argument '{}'", name);
            };
            if slots[index].is_some() {
                bail!("duplicate argument '{}'", name);
            }
            slots[index] = Some(self.lower_expr(&keyword.value)?);
        }
        if slots.iter().any(|slot| slot.is_none()) {
            bail!("missing required argument for {}", function);
        }
        Ok(slots.into_iter().map(Option::unwrap).collect())
    }

    fn parse_binary_op(&self, op: &str) -> Result<BinaryOp> {
        Ok(match op {
            "add" => BinaryOp::Add,
            "sub" => BinaryOp::Sub,
            "mul" => BinaryOp::Mul,
            "div" => BinaryOp::Div,
            "mod" => BinaryOp::Mod,
            "pow" => BinaryOp::Pow,
            "eq" => BinaryOp::Eq,
            "ne" => BinaryOp::NotEq,
            "lt" => BinaryOp::Lt,
            "le" => BinaryOp::LtEq,
            "gt" => BinaryOp::Gt,
            "ge" => BinaryOp::GtEq,
            _ => bail!("unsupported binary operator: {}", op),
        })
    }

    fn parse_compare_op(&self, op: &str) -> Result<BinaryOp> {
        Ok(match op {
            "eq" | "==" => BinaryOp::Eq,
            "ne" | "!=" => BinaryOp::NotEq,
            "lt" | "<" => BinaryOp::Lt,
            "le" | "<=" => BinaryOp::LtEq,
            "gt" | ">" => BinaryOp::Gt,
            "ge" | ">=" => BinaryOp::GtEq,
            _ => bail!("unsupported comparison operator: {}", op),
        })
    }

    fn value_type(&self, value: &Value) -> Result<Type> {
        match value {
            Value::Int(_) | Value::Int128(_) => Ok(Type::Int),
            Value::Float(_) => Ok(Type::Float),
            Value::String(_) => Ok(Type::String),
            Value::Bool(_) => Ok(Type::Bool),
            Value::Name(id) => Ok(self
                .type_context
                .lookup(id)
                .cloned()
                .unwrap_or(Type::Unknown)),
            Value::Field { ty, .. } => Ok(ty.clone()),
            Value::Unary { ty, .. } => Ok(ty.clone()),
            Value::Binary { ty, .. } => Ok(ty.clone()),
            Value::Call { return_type, .. } => Ok(return_type.clone()),
            Value::List { element_type, .. } => Ok(Type::Array(Box::new(element_type.clone()))),
            Value::ListComp { element_type, .. } => Ok(Type::Array(Box::new(element_type.clone()))),
            Value::Tuple { element_types, .. } => Ok(Type::Tuple(element_types.clone())),
            Value::Dict {
                key_type,
                value_type,
                ..
            } => Ok(Type::Dict {
                key: Box::new(key_type.clone()),
                value: Box::new(value_type.clone()),
            }),
            Value::Index { element_type, .. } => Ok(element_type.clone()),
            Value::Slice { container_type, .. } => Ok(container_type.clone()),
            Value::FormatString { .. } => Ok(Type::String),
        }
    }

    fn infer_binary_result_type(&self, left: &Type, op: &BinaryOp, right: &Type) -> Result<Type> {
        if matches!(left, Type::Unknown) || matches!(right, Type::Unknown) {
            return Ok(Type::Unknown);
        }

        match (left, right, op) {
            (Type::Int, Type::Int, BinaryOp::Add) => Ok(Type::Int),
            (Type::Int, Type::Int, BinaryOp::Sub) => Ok(Type::Int),
            (Type::Int, Type::Int, BinaryOp::Mul) => Ok(Type::Int),
            (Type::Int, Type::Int, BinaryOp::Div) => Ok(Type::Float), // Python 3: / returns float
            (Type::Int, Type::Int, BinaryOp::Mod) => Ok(Type::Int),
            (Type::Int, Type::Int, BinaryOp::Pow) => Ok(Type::Int),
            (Type::Int, Type::Int, BinaryOp::Eq) => Ok(Type::Bool),
            (Type::Int, Type::Int, BinaryOp::NotEq) => Ok(Type::Bool),
            (Type::Int, Type::Int, BinaryOp::Lt) => Ok(Type::Bool),
            (Type::Int, Type::Int, BinaryOp::LtEq) => Ok(Type::Bool),
            (Type::Int, Type::Int, BinaryOp::Gt) => Ok(Type::Bool),
            (Type::Int, Type::Int, BinaryOp::GtEq) => Ok(Type::Bool),
            (Type::Float, Type::Float, BinaryOp::Add) => Ok(Type::Float),
            (Type::Float, Type::Float, BinaryOp::Sub) => Ok(Type::Float),
            (Type::Float, Type::Float, BinaryOp::Mul) => Ok(Type::Float),
            (Type::Float, Type::Float, BinaryOp::Div) => Ok(Type::Float),
            (Type::Float, Type::Float, BinaryOp::Pow) => Ok(Type::Float),
            (Type::Int, Type::Float, BinaryOp::Add)
            | (Type::Int, Type::Float, BinaryOp::Sub)
            | (Type::Int, Type::Float, BinaryOp::Mul)
            | (Type::Int, Type::Float, BinaryOp::Div)
            | (Type::Float, Type::Int, BinaryOp::Add)
            | (Type::Float, Type::Int, BinaryOp::Sub)
            | (Type::Float, Type::Int, BinaryOp::Mul)
            | (Type::Float, Type::Int, BinaryOp::Div) => Ok(Type::Float),
            (Type::Int, Type::Float, BinaryOp::Pow) | (Type::Float, Type::Int, BinaryOp::Pow) => {
                Ok(Type::Float)
            }
            (Type::String, Type::String, BinaryOp::Add) => Ok(Type::String),
            (Type::String, Type::String, BinaryOp::Eq) => Ok(Type::Bool),
            _ => bail!("unsupported operation: {} {} {}", left, op.symbol(), right),
        }
    }

    fn infer_call_return_type(&self, function: &str, _args: &[Value]) -> Result<Type> {
        if let Some((_, _, return_type)) = self.function_signatures.get(function) {
            return Ok(return_type.clone());
        }

        Ok(match function {
            "print" => Type::None,
            "len" => Type::Int,
            "str" => Type::String,
            "int" => Type::Int,
            "float" => Type::Float,
            "bool" => Type::Bool,
            "range" => Type::Array(Box::new(Type::Int)),
            "abs" => Type::Int,
            "min" | "max" | "sum" | "round" => Type::Int,
            "any" | "all" => Type::Bool,
            "ord" => Type::Int,
            "chr" => Type::String,
            "sorted" | "list" => Type::Array(Box::new(Type::Unknown)),
            "dict" => Type::Dict {
                key: Box::new(Type::String),
                value: Box::new(Type::Unknown),
            },
            "tarvos_perf_counter" => Type::Float,
            "tarvos_time" | "tarvos_math_sqrt" | "tarvos_math_sin" | "tarvos_math_cos"
            | "tarvos_math_tan" | "tarvos_math_asin" | "tarvos_math_acos" | "tarvos_math_atan"
            | "tarvos_math_exp" | "tarvos_math_log" | "tarvos_math_log10" | "tarvos_math_fabs"
            | "tarvos_math_pow" | "tarvos_math_hypot" | "tarvos_math_atan2" => Type::Float,
            "tarvos_math_floor" | "tarvos_math_ceil" => Type::Int,
            "tarvos_math_isfinite" | "tarvos_math_isnan" | "tarvos_math_isinf" => Type::Bool,
            "tarvos_sleep" => Type::None,
            "tarvos_os_path_join" | "tarvos_os_path_basename" | "tarvos_os_path_dirname" => {
                Type::String
            }
            "tarvos_os_path_exists" | "tarvos_os_path_isfile" | "tarvos_os_path_isdir" => {
                Type::Bool
            }
            _ => Type::Unknown,
        })
    }

    fn infer_return_type_from_body(&mut self, body: &[tarvos_ast::Stmt]) -> Option<Type> {
        let mut inferred: Option<Type> = None;

        for stmt in body {
            match stmt {
                tarvos_ast::Stmt::Return { value } => {
                    let ty = match value {
                        Some(value) => {
                            let lowered = self.lower_expr(value).ok()?;
                            self.value_type(&lowered).ok()?
                        }
                        None => Type::None,
                    };
                    inferred = Some(match inferred {
                        Some(existing) => Self::merge_return_types(&existing, &ty),
                        None => ty,
                    });
                }
                tarvos_ast::Stmt::If { body, orelse, .. } => {
                    let if_ty = self.infer_return_type_from_body(body);
                    let else_ty = self.infer_return_type_from_body(orelse);
                    let merged = match (if_ty, else_ty) {
                        (Some(a), Some(b)) => Some(Self::merge_return_types(&a, &b)),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    };
                    if let Some(ty) = merged {
                        inferred = Some(match inferred {
                            Some(existing) => Self::merge_return_types(&existing, &ty),
                            None => ty,
                        });
                    }
                }
                tarvos_ast::Stmt::While { body, .. } | tarvos_ast::Stmt::For { body, .. } => {
                    if let Some(ty) = self.infer_return_type_from_body(body) {
                        inferred = Some(match inferred {
                            Some(existing) => Self::merge_return_types(&existing, &ty),
                            None => ty,
                        });
                    }
                }
                tarvos_ast::Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    if let Some(ty) = self.infer_return_type_from_body(body) {
                        inferred = Some(match inferred {
                            Some(existing) => Self::merge_return_types(&existing, &ty),
                            None => ty,
                        });
                    }
                    for h in handlers {
                        if let Some(ty) = self.infer_return_type_from_body(&h.body) {
                            inferred = Some(match inferred {
                                Some(existing) => Self::merge_return_types(&existing, &ty),
                                None => ty,
                            });
                        }
                    }
                    if let Some(ty) = self.infer_return_type_from_body(orelse) {
                        inferred = Some(match inferred {
                            Some(existing) => Self::merge_return_types(&existing, &ty),
                            None => ty,
                        });
                    }
                    if let Some(ty) = self.infer_return_type_from_body(finalbody) {
                        inferred = Some(match inferred {
                            Some(existing) => Self::merge_return_types(&existing, &ty),
                            None => ty,
                        });
                    }
                }
                tarvos_ast::Stmt::With { body, .. } => {
                    if let Some(ty) = self.infer_return_type_from_body(body) {
                        inferred = Some(match inferred {
                            Some(existing) => Self::merge_return_types(&existing, &ty),
                            None => ty,
                        });
                    }
                }
                _ => {}
            }
        }

        inferred
    }

    fn merge_return_types(left: &Type, right: &Type) -> Type {
        if left == right {
            left.clone()
        } else if matches!(left, Type::Unknown) {
            right.clone()
        } else if matches!(right, Type::Unknown) {
            left.clone()
        } else {
            Type::Unknown
        }
    }

    fn parse_type_annotation(&self, annotation: &str) -> Option<Type> {
        let annotation = annotation.trim();
        if self.class_names.contains(annotation) {
            return Some(Type::Object(annotation.to_string()));
        }
        match annotation {
            "int" | "Integer" => Some(Type::Int),
            "float" | "Float" => Some(Type::Float),
            "bool" | "Boolean" => Some(Type::Bool),
            "str" | "string" => Some(Type::String),
            "None" | "none" => Some(Type::None),
            _ => None,
        }
    }
}

fn module_reference(expr: &tarvos_ast::Expr) -> Option<String> {
    match expr {
        tarvos_ast::Expr::Name { id } => Some(id.clone()),
        tarvos_ast::Expr::Attribute { value, attr } => {
            Some(format!("{}.{}", module_reference(value)?, attr))
        }
        _ => None,
    }
}

fn collect_class_fields(body: &[tarvos_ast::Stmt]) -> Vec<(String, Type)> {
    let mut fields = Vec::new();
    for statement in body {
        let tarvos_ast::Stmt::FunctionDef {
            name: method_name,
            args,
            arg_annotations,
            body: method_body,
            ..
        } = statement
        else {
            continue;
        };
        if method_name != "__init__" {
            continue;
        }
        let parameter_types = args
            .iter()
            .zip(arg_annotations.iter())
            .filter_map(|(name, annotation)| {
                annotation
                    .as_deref()
                    .map(|annotation| (name.clone(), annotation_type(annotation)))
            })
            .collect::<HashMap<_, _>>();
        collect_fields_from_statements(method_body, &mut fields, &parameter_types);
    }
    fields
}

fn collect_fields_from_statements(
    statements: &[tarvos_ast::Stmt],
    fields: &mut Vec<(String, Type)>,
    parameter_types: &HashMap<String, Type>,
) {
    for statement in statements {
        match statement {
            tarvos_ast::Stmt::Assign {
                target: tarvos_ast::Expr::Attribute { value, attr },
                value: assigned,
            }
            | tarvos_ast::Stmt::AnnAssign {
                target: tarvos_ast::Expr::Attribute { value, attr },
                value: Some(assigned),
                ..
            } if matches!(value.as_ref(), tarvos_ast::Expr::Name { id } if id == "self") => {
                let ty = expression_type_with_names(assigned, parameter_types);
                if !fields.iter().any(|(name, _)| name == attr) {
                    fields.push((attr.clone(), ty));
                }
            }
            tarvos_ast::Stmt::If { body, orelse, .. } => {
                collect_fields_from_statements(body, fields, parameter_types);
                collect_fields_from_statements(orelse, fields, parameter_types);
            }
            tarvos_ast::Stmt::While { body, .. }
            | tarvos_ast::Stmt::For { body, .. }
            | tarvos_ast::Stmt::With { body, .. } => {
                collect_fields_from_statements(body, fields, parameter_types);
            }
            _ => {}
        }
    }
}

fn annotation_type(annotation: &str) -> Type {
    match annotation {
        "int" => Type::Int,
        "float" => Type::Float,
        "bool" => Type::Bool,
        "str" | "string" => Type::String,
        _ => Type::Unknown,
    }
}

fn expression_type_with_names(expr: &tarvos_ast::Expr, names: &HashMap<String, Type>) -> Type {
    match expr {
        tarvos_ast::Expr::Name { id } => names.get(id).cloned().unwrap_or(Type::Unknown),
        tarvos_ast::Expr::Binary { left, right, .. } => {
            let left_type = expression_type_with_names(left, names);
            let right_type = expression_type_with_names(right, names);
            if left_type == Type::Float || right_type == Type::Float {
                Type::Float
            } else {
                left_type
            }
        }
        _ => expression_type(expr),
    }
}

fn expression_type(expr: &tarvos_ast::Expr) -> Type {
    match expr {
        tarvos_ast::Expr::Int { .. } => Type::Int,
        tarvos_ast::Expr::Float { .. } => Type::Float,
        tarvos_ast::Expr::Bool { .. } => Type::Bool,
        tarvos_ast::Expr::String { .. } | tarvos_ast::Expr::FormatString { .. } => Type::String,
        tarvos_ast::Expr::List { elements } => Type::Array(Box::new(
            elements
                .first()
                .map(expression_type)
                .unwrap_or(Type::Unknown),
        )),
        tarvos_ast::Expr::Binary { left, right, .. } => {
            let left_type = expression_type(left);
            let right_type = expression_type(right);
            if left_type == Type::Float || right_type == Type::Float {
                Type::Float
            } else {
                left_type
            }
        }
        _ => Type::Unknown,
    }
}

fn rust_identifier(name: &str) -> String {
    let mut identifier = String::with_capacity(name.len());
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                identifier.push('_');
            }
            identifier.push(character.to_ascii_lowercase());
        } else if character.is_ascii_alphanumeric() || character == '_' {
            identifier.push(character);
        } else {
            identifier.push('_');
        }
    }
    if identifier.is_empty() {
        "_".to_string()
    } else {
        identifier
    }
}

pub fn lower_module(module: &tarvos_ast::Module) -> Result<Module> {
    let mut lowerer = Lowerer::new();
    lowerer.lower_module(module)
}
