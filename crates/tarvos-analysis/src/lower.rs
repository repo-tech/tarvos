use crate::stdlib::{dict_method_mutates, list_method_mutates, MethodReceiver};
use crate::{module_supported, native_builtin_method, native_constant, native_function};
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
    /// Element types for list literals that are empty where they are bound.
    ///
    /// `xs = []` carries no element type of its own; the only evidence is a
    /// later `xs.append(...)`. This is resolved by a pre-pass so the binding is
    /// declared as a real `Vec<T>` instead of `Vec<()>`, which would otherwise
    /// make the function's own return type unusable.
    empty_list_hints: HashMap<String, Type>,
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
            empty_list_hints: HashMap::new(),
        }
    }

    pub fn lower_module(&mut self, module: &tarvos_ast::Module) -> Result<Module> {
        let mut statements = Vec::new();

        // One pre-pass serves both the empty-list element types and the
        // unannotated parameter types, so they cannot disagree about a name.
        let variable_types = collect_variable_types(&module.body);
        self.empty_list_hints = variable_types
            .iter()
            .filter_map(|(name, ty)| match ty {
                Type::Array(element) if **element != Type::Unknown => {
                    Some((name.clone(), (**element).clone()))
                }
                _ => None,
            })
            .collect();

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
                let inferred = infer_function_argument_types(&module.body, name, args.len());
                let param_types: Vec<Type> = args
                    .iter()
                    .enumerate()
                    .map(|(index, _)| {
                        arg_annotations
                            .get(index)
                            .and_then(|annotation| annotation.as_deref())
                            .and_then(|s| self.parse_type_annotation(s))
                            .unwrap_or_else(|| inferred[index].clone())
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
                             supported native modules: math, time, os, os.path, json",
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
                         supported native modules: math, time, os, os.path, json",
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
                    // An empty list literal has no element type of its own, so the
                    // pre-pass hint from a later `append` supplies it here. Without
                    // this the binding becomes `Vec<()>` and the function's return
                    // type is rejected as dynamic.
                    let value_ir = self.apply_empty_list_hint(id, value_ir);
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
                tarvos_ast::Expr::Subscript { .. } => {
                    let (id, index_exprs) = Self::split_subscript_target(target)?;
                    let mut indices = Vec::with_capacity(index_exprs.len());
                    for (position, index_expr) in index_exprs.iter().enumerate() {
                        let index_ir = self.lower_expr(index_expr)?;
                        if position + 1 < index_exprs.len() && matches!(index_ir, Value::String(_))
                        {
                            bail!(
                                "nested subscript assignment through a dictionary is not \
                                 supported natively; use --python-fallback for that pattern"
                            );
                        }
                        indices.push(index_ir);
                    }
                    let value_ir = self.lower_expr(value)?;
                    Ok(Stmt::IndexAssign {
                        target: id,
                        indices,
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
                tarvos_ast::Expr::Tuple { elements } => {
                    let mut targets = Vec::with_capacity(elements.len());
                    for element in elements {
                        let tarvos_ast::Expr::Name { id } = element else {
                            bail!(
                                "tuple assignment targets must be variable names; \
                                 starred and nested targets are not supported natively"
                            );
                        };
                        targets.push(id.clone());
                    }
                    if targets.is_empty() {
                        bail!("tuple assignment requires at least one target");
                    }
                    let value_ir = self.lower_expr(value)?;
                    let tarvos_types::Type::Tuple(element_types) = self.value_type(&value_ir)?
                    else {
                        bail!(
                            "tuple assignment requires a tuple value with the same number of elements"
                        );
                    };
                    if element_types.len() != targets.len() {
                        bail!(
                            "tuple assignment has {} targets but {} values",
                            targets.len(),
                            element_types.len()
                        );
                    }
                    for (target, element_type) in targets.iter().zip(element_types) {
                        self.type_context.declare(target.clone(), element_type);
                    }
                    Ok(Stmt::Destructure {
                        targets,
                        value: value_ir,
                    })
                }
                _ => bail!("assignment target must be a variable name, tuple, or subscript"),
            },

            tarvos_ast::Stmt::AugAssign {
                target,
                operator,
                value,
            } => {
                // Desugar: x op= e  →  x = x op e
                //
                // Routing through `Stmt::Assign` means name targets and subscript
                // chains (`grid[i][j] += e`) share one lowering path.
                let synthetic = tarvos_ast::Expr::Binary {
                    left: Box::new(target.clone()),
                    operator: operator.clone(),
                    right: Box::new(value.clone()),
                };
                self.lower_stmt(&tarvos_ast::Stmt::Assign {
                    target: target.clone(),
                    value: synthetic,
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
                                if let Some(reference) = module_reference(function) {
                                    if let Some((module_name, method)) = reference.rsplit_once('.')
                                    {
                                        let module = self
                                            .module_aliases
                                            .get(module_name)
                                            .cloned()
                                            .or_else(|| {
                                                let (parent, child) =
                                                    module_name.rsplit_once('.')?;
                                                let parent_module =
                                                    self.module_aliases.get(parent)?;
                                                let qualified = format!("{parent_module}.{child}");
                                                (qualified == module_name).then_some(qualified)
                                            });
                                        if let Some(module) = module.as_deref() {
                                            if let Some(function) = native_function(module, method)
                                            {
                                                if function.rust_name == "tarvos_json_dumps_static"
                                                {
                                                    return Ok(Stmt::Expr(
                                                        self.lower_static_json_expr(args)?,
                                                    ));
                                                }
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
                                }
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
                            let module =
                                self.module_aliases.get(&module_name).cloned().or_else(|| {
                                    let (parent, child) = module_name.rsplit_once('.')?;
                                    let parent_module = self.module_aliases.get(parent)?;
                                    let qualified = format!("{parent_module}.{child}");
                                    (qualified == module_name).then_some(qualified)
                                });
                            if let Some(module) = module.as_deref() {
                                if let Some(function) = native_function(module, method) {
                                    if function.rust_name == "tarvos_json_dumps_static" {
                                        return Ok(Stmt::Expr(self.lower_static_json_expr(args)?));
                                    }
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

                let element_type = self.iterable_element_type(&iter_ir)?;
                self.type_context.declare(id.clone(), element_type);

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

                let inferred_params = self
                    .function_signatures
                    .get(name)
                    .map(|(_, types, _)| types.clone())
                    .unwrap_or_else(|| vec![Type::Unknown; args.len()]);
                for (index, arg_name) in args.iter().enumerate() {
                    let arg_annotation =
                        arg_annotations.get(index).and_then(|value| value.as_ref());
                    let arg_type = arg_annotation
                        .and_then(|value| self.parse_type_annotation(value))
                        .or_else(|| inferred_params.get(index).cloned())
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
                bail!(
                    "`{obj_name}.append()` requires a list binding, but `{obj_name}` is {obj_type}"
                );
            }
            "extend" | "insert" | "remove" | "pop" | "sort" | "reverse" | "clear" => {
                // These mutate the receiver in place, so they become a statement
                // that is evaluated for its effect rather than a call expression.
                let value = self.lower_builtin_method(
                    &tarvos_ast::Expr::Name {
                        id: obj_name.to_string(),
                    },
                    method,
                    args,
                    true,
                )?;
                let Some(value) = value else {
                    bail!("`{obj_name}` is not a list, so `{method}()` is not available natively");
                };
                Ok(Stmt::Expr(value))
            }
            "update" | "setdefault" | "get" | "keys" | "values" | "items" => {
                let value = self.lower_builtin_method(
                    &tarvos_ast::Expr::Name {
                        id: obj_name.to_string(),
                    },
                    method,
                    args,
                    true,
                )?;
                let Some(value) = value else {
                    bail!("`{obj_name}` is not a dict, so `{method}()` is not available natively");
                };
                Ok(Stmt::Expr(value))
            }
            _ => {
                // Non-mutating builtin (e.g. `s.strip()`) is a pure call, so it
                // shares the expression path and its real return type.
                if let Some(value) = self.lower_builtin_method(
                    &tarvos_ast::Expr::Name {
                        id: obj_name.to_string(),
                    },
                    method,
                    args,
                    true,
                )? {
                    return Ok(Stmt::Expr(value));
                }
                bail!(
                    "method `{method}()` is not supported natively on `{obj_name}`; \
                     supported str methods: lower, upper, strip, split, join, replace, \
                     startswith, endswith, find, count; \
                     supported list methods: append, extend, insert, pop, remove, sort, reverse, clear"
                );
            }
        }
    }

    /// Fill in the element type of an empty list literal bound to `name`.
    ///
    /// Only a list that is actually empty is touched: a non-empty literal's type
    /// comes from its own elements, and a list that already has a known element
    /// type is left alone so the pre-pass can never contradict the literal.
    fn apply_empty_list_hint(&self, name: &str, value: Value) -> Value {
        let Value::List {
            elements,
            element_type,
        } = value
        else {
            return value;
        };
        if !elements.is_empty() || element_type != Type::Unknown {
            return Value::List {
                elements,
                element_type,
            };
        }
        match self.empty_list_hints.get(name) {
            Some(hint) => Value::List {
                elements,
                element_type: hint.clone(),
            },
            None => Value::List {
                elements,
                element_type,
            },
        }
    }

    /// Element type produced by iterating `value`.
    ///
    /// Python iterates a `str` by character and a `dict` by key, so those are
    /// resolved here rather than treated as their container type. The value's
    /// own type is tried first, and its shape second, because a `range()` call
    /// only reveals its element type through the value it produces.
    fn iterable_element_type(&self, value: &Value) -> Result<Type> {
        let ty = self.value_type(value)?;
        let from_type = match &ty {
            Type::Array(element) => Some((**element).clone()),
            Type::Dict { key, .. } => Some((**key).clone()),
            Type::String => Some(Type::String),
            Type::Tuple(types) => types.first().cloned(),
            _ => None,
        };
        if let Some(element) = from_type.filter(|element| *element != Type::Unknown) {
            return Ok(element);
        }
        // `range()` and a bare list literal only know their element type here.
        let from_value = match value {
            Value::List { element_type, .. } | Value::ListComp { element_type, .. } => {
                Some(element_type.clone())
            }
            Value::Name(name) => match self.type_context.lookup(name) {
                Some(Type::Array(inner)) => Some(*inner.clone()),
                _ => None,
            },
            _ => None,
        };
        Ok(from_value.unwrap_or(Type::Unknown))
    }

    /// Resolve the static type of a method-call receiver.
    ///
    /// Only the forms the native backend can type are recognized: a named
    /// binding from the current scope, or a literal. Anything else returns
    /// `None` so the caller can fall through to the class-instance path.
    fn receiver_type(&mut self, object: &tarvos_ast::Expr) -> Result<Option<Type>> {
        Ok(match object {
            tarvos_ast::Expr::Name { id } => self.type_context.lookup(id).cloned(),
            tarvos_ast::Expr::String { .. } | tarvos_ast::Expr::FormatString { .. } => {
                Some(Type::String)
            }
            tarvos_ast::Expr::List { elements } => {
                let element = match elements.first() {
                    Some(first) => self.lower_expr(first).and_then(|ir| self.value_type(&ir))?,
                    None => Type::Unknown,
                };
                Some(Type::Array(Box::new(element)))
            }
            // A chained call such as `text.strip().lower()` names its receiver
            // through the previous call's result, so the type has to come from
            // lowering it rather than from the scope.
            tarvos_ast::Expr::Call { .. } | tarvos_ast::Expr::MethodCall { .. } => {
                let lowered = self.lower_expr(object)?;
                Some(self.value_type(&lowered)?)
            }
            _ => None,
        })
    }

    /// Lower a `str`/`list`/`dict` builtin method call, if the receiver's type selects one.
    ///
    /// Returns `Ok(None)` when the receiver is not a builtin container or the
    /// method is not part of the native subset, so the caller can continue with
    /// user-defined class methods.
    ///
    /// `in_statement_position` allows a method that mutates its receiver. Such a
    /// call is fine as a statement (`xs.sort()`) but cannot be an expression,
    /// because the IR has no way to express reading a value that a later write
    /// would invalidate.
    fn lower_builtin_method(
        &mut self,
        object: &tarvos_ast::Expr,
        method: &str,
        args: &[tarvos_ast::Expr],
        in_statement_position: bool,
    ) -> Result<Option<Value>> {
        let receiver = match self.receiver_type(object)? {
            Some(Type::String) => MethodReceiver::Str,
            Some(Type::Array(_)) => MethodReceiver::List,
            Some(Type::Dict { .. }) => MethodReceiver::Dict,
            _ => return Ok(None),
        };

        let Some(builtin) = native_builtin_method(receiver, method) else {
            return Ok(None);
        };
        let crate::stdlib::BuiltinMethod {
            rust_name,
            arity,
            mut return_type,
        } = builtin;

        // A mutating builtin used in value position would need to both return and
        // write back; the IR models that separately, so reject it here.
        let mutates = match receiver {
            MethodReceiver::List => list_method_mutates(method),
            MethodReceiver::Dict => dict_method_mutates(method),
            MethodReceiver::Str => false,
        };
        if mutates && !in_statement_position {
            bail!("`{method}()` mutates its receiver in place and cannot be used as an expression");
        }

        if !arity.contains(&args.len()) {
            let expected = if arity.start() == arity.end() {
                format!("{}", arity.start())
            } else {
                format!("{} to {}", arity.start(), arity.end())
            };
            bail!(
                "`{method}()` takes {expected} argument(s) but {} were given",
                args.len()
            );
        }

        let receiver_ir = self.lower_expr(object)?;
        let receiver_ty = self.value_type(&receiver_ir)?;

        // `sort()` needs a total order, and `f64` is not `Ord`. The element type
        // is only known here, so the helper is selected at lowering time rather
        // than guessed at the call site.
        let rust_name = match (receiver, method, &receiver_ty) {
            (MethodReceiver::List, "sort", Type::Array(element)) => match **element {
                Type::Float => "tarvos_list_sort_f64",
                Type::Int => "tarvos_list_sort_i64",
                Type::String => "tarvos_list_sort_string",
                Type::Bool => "tarvos_list_sort_bool",
                ref other => bail!("`sort()` is not supported natively for {other} elements"),
            },
            _ => rust_name,
        };

        // `pop()`, `get()` and `setdefault()` return the container's element or
        // value type, which the static registry cannot know.
        return_type = match (receiver, method, &receiver_ty) {
            (MethodReceiver::List, "pop", Type::Array(element)) => (**element).clone(),
            (MethodReceiver::Dict, "get" | "pop" | "setdefault", Type::Dict { value, .. }) => {
                (**value).clone()
            }
            _ => return_type,
        };

        let mut args_ir = vec![receiver_ir];
        args_ir.extend(
            args.iter()
                .map(|arg| self.lower_expr(arg))
                .collect::<Result<Vec<_>>>()?,
        );

        Ok(Some(Value::Call {
            function: rust_name.to_string(),
            args: args_ir,
            return_type,
        }))
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
                if operators.len() != comparators.len() || operators.is_empty() {
                    bail!(
                        "malformed comparison: {} operator(s) with {} operand(s)",
                        operators.len(),
                        comparators.len()
                    );
                }

                let mut left_ir = self.lower_expr(left)?;
                let mut result = None;
                for (operator, comparator) in operators.iter().zip(comparators) {
                    let right_ir = self.lower_expr(comparator)?;
                    let op = self.parse_compare_op(operator)?;
                    let term = Value::Binary {
                        left: Box::new(left_ir),
                        op,
                        right: Box::new(right_ir.clone()),
                        ty: Type::Bool,
                    };
                    // `and` short-circuits, so a failing earlier term skips the rest
                    // of the chain exactly like Python.
                    result = Some(match result {
                        None => term,
                        Some(previous) => Value::Binary {
                            left: Box::new(previous),
                            op: BinaryOp::And,
                            right: Box::new(term),
                            ty: Type::Bool,
                        },
                    });
                    left_ir = right_ir;
                }
                result.ok_or_else(|| anyhow::anyhow!("comparison has no operands"))
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
                    if matches!(id.as_str(), "list" | "sorted")
                        && !self.class_names.contains(id)
                        && !self.imported_functions.contains_key(id)
                    {
                        return self.lower_list_or_sorted_call(id, args, keywords);
                    }
                    let function_name = self
                        .imported_functions
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| id.clone());
                    if function_name == "tarvos_json_dumps_static" {
                        return self.lower_static_json_expr(args);
                    }
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
                    let module = self.module_aliases.get(&module_name).cloned().or_else(|| {
                        let (parent, child) = module_name.rsplit_once('.')?;
                        let parent_module = self.module_aliases.get(parent)?;
                        let qualified = format!("{parent_module}.{child}");
                        (qualified == module_name).then_some(qualified)
                    });
                    if let Some(module) = module.as_deref() {
                        if let Some(function) = native_function(module, method) {
                            if function.rust_name == "tarvos_json_dumps_static" {
                                return self.lower_static_json_expr(args);
                            }
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
                            if function.rust_name == "tarvos_json_dumps_static" {
                                return self.lower_static_json_expr(args);
                            }
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

                // A builtin method is selected by the receiver's static type. This
                // runs before the class-instance path so `s.lower()` resolves
                // against a `str` receiver instead of being mistaken for a method
                // on a user-defined class instance.
                if let Some(value) = self.lower_builtin_method(object, method, args, false)? {
                    return Ok(value);
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
                // Python's unary plus is a numeric identity: `+x` keeps `x` unchanged.
                if matches!(operator.as_str(), "uadd" | "+") {
                    return match op_type {
                        Type::Int | Type::Float | Type::Unknown => Ok(operand_ir),
                        other => bail!("unary plus requires a numeric operand, got {}", other),
                    };
                }
                let op = match operator.as_str() {
                    "usub" | "-" => tarvos_ir::UnaryOp::Neg,
                    "not" => tarvos_ir::UnaryOp::Not,
                    "invert" | "~" => tarvos_ir::UnaryOp::Invert,
                    _ => bail!("unsupported unary operator: {}", operator),
                };
                let res_type = match op {
                    tarvos_ir::UnaryOp::Neg => op_type,
                    tarvos_ir::UnaryOp::Not => Type::Bool,
                    tarvos_ir::UnaryOp::Invert => {
                        if !matches!(op_type, Type::Int | Type::Unknown) {
                            bail!("bitwise NOT (~) requires an integer operand");
                        }
                        Type::Int
                    }
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
                // The loop variable has the iterable's element type. Assuming
                // `Int` here would type every comprehension over strings wrong
                // and produce uncompilable Rust for the element expression.
                let element_type = self.iterable_element_type(&iter_ir)?;
                self.type_context.declare(target.clone(), element_type);
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

    /// Split a subscript assignment target into its base variable and index chain:
    /// `grid[i][j]` → `("grid", [i, j])`.
    fn split_subscript_target(
        target: &tarvos_ast::Expr,
    ) -> Result<(String, Vec<tarvos_ast::Expr>)> {
        let mut indices = Vec::new();
        let mut current = target;
        loop {
            match current {
                tarvos_ast::Expr::Subscript { value, index } => {
                    indices.insert(0, (**index).clone());
                    current = value;
                }
                tarvos_ast::Expr::Name { id } => {
                    if indices.is_empty() {
                        bail!("subscript assignment target requires at least one index");
                    }
                    return Ok((id.clone(), indices));
                }
                other => bail!(
                    "unsupported assignment target `{}`; only `name[i]` and `name[i][j]` \
                     chains are supported natively",
                    other.kind_name()
                ),
            }
        }
    }

    fn parse_binary_op(&self, op: &str) -> Result<BinaryOp> {
        Ok(match op {
            "add" => BinaryOp::Add,
            "sub" => BinaryOp::Sub,
            "mul" => BinaryOp::Mul,
            "div" => BinaryOp::Div,
            "mod" => BinaryOp::Mod,
            "pow" => BinaryOp::Pow,
            "bitand" => BinaryOp::BitAnd,
            "bitor" => BinaryOp::BitOr,
            "bitxor" => BinaryOp::BitXor,
            "lshift" => BinaryOp::LShift,
            "rshift" => BinaryOp::RShift,
            "floordiv" => BinaryOp::FloorDiv,
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
            (
                Type::Int,
                Type::Int,
                BinaryOp::BitXor
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::LShift
                | BinaryOp::RShift,
            ) => Ok(Type::Int),
            (Type::Int, Type::Int, BinaryOp::FloorDiv) => Ok(Type::Int),
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
            (Type::Float, Type::Float, BinaryOp::FloorDiv) => Ok(Type::Float),
            (Type::Int, Type::Float, BinaryOp::Add)
            | (Type::Int, Type::Float, BinaryOp::Sub)
            | (Type::Int, Type::Float, BinaryOp::Mul)
            | (Type::Int, Type::Float, BinaryOp::Div)
            | (Type::Int, Type::Float, BinaryOp::FloorDiv)
            | (Type::Float, Type::Int, BinaryOp::Add)
            | (Type::Float, Type::Int, BinaryOp::Sub)
            | (Type::Float, Type::Int, BinaryOp::Mul)
            | (Type::Float, Type::Int, BinaryOp::Div)
            | (Type::Float, Type::Int, BinaryOp::FloorDiv) => Ok(Type::Float),
            (Type::Int, Type::Float, BinaryOp::Pow) | (Type::Float, Type::Int, BinaryOp::Pow) => {
                Ok(Type::Float)
            }
            (Type::Array(_), Type::Int, BinaryOp::Mul)
            | (Type::Int, Type::Array(_), BinaryOp::Mul) => Ok(left.clone()),
            (Type::String, Type::Int, BinaryOp::Mul) | (Type::Int, Type::String, BinaryOp::Mul) => {
                Ok(Type::String)
            }
            (Type::String, Type::String, BinaryOp::Add) => Ok(Type::String),
            (Type::String, Type::String, BinaryOp::Eq) => Ok(Type::Bool),
            _ => bail!("unsupported operation: {} {} {}", left, op.symbol(), right),
        }
    }

    fn infer_call_return_type(&self, function: &str, args: &[Value]) -> Result<Type> {
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
            // `list()` / `sorted()` preserve their input's element type. The
            // lowering stage computes the exact type; this fallback only runs
            // for shapes the dedicated path already rejected, so `Unknown`
            // forces an explicit fallback diagnostic instead of `Vec<()>`.
            "sorted" | "list" => Type::Array(Box::new(
                args.first()
                    .map(|arg| self.call_arg_element_type(arg))
                    .transpose()?
                    .unwrap_or(Type::Unknown),
            )),
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
            "tarvos_os_getcwd" => Type::String,
            "tarvos_os_listdir" => Type::Array(Box::new(Type::String)),
            "tarvos_os_mkdir" | "tarvos_os_makedirs" | "tarvos_os_chdir" => Type::None,
            "tarvos_os_path_join" | "tarvos_os_path_basename" | "tarvos_os_path_dirname" => {
                Type::String
            }
            "tarvos_os_path_exists" | "tarvos_os_path_isfile" | "tarvos_os_path_isdir" => {
                Type::Bool
            }
            "tarvos_json_dumps_static" => Type::String,
            _ => Type::Unknown,
        })
    }

    /// Element type produced by iterating a lowered `list()`/`sorted()` argument.
    ///
    /// `range()` and `str` only reveal their element type through the value,
    /// so the value shape is checked alongside the static type.
    fn call_arg_element_type(&self, arg: &Value) -> Result<Type> {
        if let Value::Call { function, .. } = arg {
            if function == "range" {
                return Ok(Type::Int);
            }
        }
        if matches!(arg, Value::String(_) | Value::FormatString { .. }) {
            return Ok(Type::String);
        }
        let ty = self.value_type(arg)?;
        match ty {
            Type::Array(element) => Ok(*element),
            Type::Dict { key, .. } => Ok(*key),
            Type::Tuple(elements) => {
                let mut uniform: Option<Type> = None;
                for element in elements {
                    match &uniform {
                        None => uniform = Some(element),
                        Some(existing) if *existing == element => {}
                        _ => bail!(
                            "list() over a heterogeneous tuple is not supported natively; \
                             use --python-fallback for mixed element types"
                        ),
                    }
                }
                Ok(uniform.unwrap_or(Type::Unknown))
            }
            Type::String => Ok(Type::String),
            other => bail!(
                "list() argument of type {other} is not a supported native iterable; \
                 supported inputs are list, range(), str, tuple, and dict; \
                 use --python-fallback otherwise"
            ),
        }
    }

    /// Lower `list(...)` / `sorted(...)` with an exact, preserved element type.
    ///
    /// `list()` materializes its argument (`list(range(3))` collects the lazy
    /// range, `list("ab")` splits into characters, `list(xs)` clones the vec)
    /// while `sorted()` materializes then sorts with the element-typed helper.
    /// The emitted call names the source kind (`__tarvos_list_from_dict`,
    /// `__tarvos_sorted_from_range`, ...) so codegen never re-derives the
    /// argument's type from a bare `Name`.
    fn lower_list_or_sorted_call(
        &mut self,
        function: &str,
        args: &[tarvos_ast::Expr],
        keywords: &[tarvos_ast::Keyword],
    ) -> Result<Value> {
        if !keywords.is_empty() {
            bail!("{function}() does not accept keyword arguments natively");
        }
        if args.len() > 1 {
            bail!(
                "{function}() takes at most 1 argument ({} given)",
                args.len()
            );
        }
        if args.is_empty() {
            return Ok(Value::List {
                elements: Vec::new(),
                element_type: Type::Unknown,
            });
        }
        let arg_ir = self.lower_expr(&args[0])?;
        // A tuple-typed `Name` (e.g. `t = (1, 2, 3); list(t)`) cannot be
        // cloned as a `Vec` — Rust tuples have no `.clone()`-to-`Vec` shape —
        // so it is desugared to field reads (`[t.0, t.1, ...]`) here, where the
        // arity is still known.
        if let Value::Name(name) = &arg_ir {
            if let Some(Type::Tuple(element_types)) = self.type_context.lookup(name).cloned() {
                let mut uniform: Option<Type> = None;
                for element in &element_types {
                    match &uniform {
                        None => uniform = Some(element.clone()),
                        Some(existing) if *existing == *element => {}
                        _ => bail!(
                            "{function}() over a heterogeneous tuple is not supported natively; \
                             use --python-fallback for mixed element types"
                        ),
                    }
                }
                let element = uniform.unwrap_or(Type::Unknown);
                if element == Type::Unknown {
                    bail!(
                        "{function}() argument has an unknown element type; \
                         bind it to a typed list first or use --python-fallback"
                    );
                }
                let elements = element_types
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| Value::Field {
                        object: Box::new(Value::Name(name.clone())),
                        field: index.to_string(),
                        ty: ty.clone(),
                    })
                    .collect::<Vec<_>>();
                let list = Value::List {
                    elements,
                    element_type: element.clone(),
                };
                if function == "sorted" {
                    return Ok(Value::Call {
                        function: format!("__tarvos_sorted_from_{}", Self::list_source_kind(&list)),
                        args: vec![list],
                        return_type: Type::Array(Box::new(element)),
                    });
                }
                return Ok(list);
            }
        }
        // A homogeneous tuple is already materialized; desugar it to a list
        // literal so tuples never reach Rust codegen as `list(tuple)`.
        if let Value::Tuple {
            elements,
            element_types,
        } = arg_ir
        {
            let mut uniform: Option<Type> = None;
            for element in &element_types {
                match &uniform {
                    None => uniform = Some(element.clone()),
                    Some(existing) if *existing == *element => {}
                    _ => bail!(
                        "{function}() over a heterogeneous tuple is not supported natively; \
                         use --python-fallback for mixed element types"
                    ),
                }
            }
            let element = uniform.unwrap_or(Type::Unknown);
            if element == Type::Unknown {
                bail!(
                    "{function}() argument has an unknown element type; \
                     bind it to a typed list first or use --python-fallback"
                );
            }
            let list = Value::List {
                elements,
                element_type: element.clone(),
            };
            if function == "sorted" {
                return Ok(Value::Call {
                    function: format!("__tarvos_sorted_from_{}", Self::list_source_kind(&list)),
                    args: vec![list],
                    return_type: Type::Array(Box::new(element)),
                });
            }
            return Ok(list);
        }
        let element = self.call_arg_element_type(&arg_ir)?;
        if element == Type::Unknown {
            bail!(
                "{function}() argument has an unknown element type; \
                 bind it to a typed list first or use --python-fallback"
            );
        }
        // `Name` dispatch must use the binding's declared type, not the value
        // shape: `d = {...}; sorted(d)` lowers `d` to a bare `Name`, whose
        // shape alone would select the `vec` (`.clone()`) path and emit
        // `HashMap::sort()`. The kind tag carries the proven shape instead.
        let kind = self.list_source_kind_for(&arg_ir);
        let lowered = if function == "sorted" {
            "sorted"
        } else {
            "list"
        };
        Ok(Value::Call {
            function: format!("__tarvos_{lowered}_from_{kind}"),
            args: vec![arg_ir],
            return_type: Type::Array(Box::new(element)),
        })
    }

    /// Codegen dispatch kind for a `list()` / `sorted()` source value.
    ///
    /// `Name` bindings are resolved through the type context: a `dict` name
    /// must take the `keys().cloned()` shape, not the `Vec::clone()` shape,
    /// and a `str` name must split into characters. Anything unrecognized
    /// stays `vec` so lowering still bails with an explicit diagnostic.
    fn list_source_kind_for(&self, arg: &Value) -> &'static str {
        if let Value::Name(name) = arg {
            match self.type_context.lookup(name) {
                Some(Type::String) => return "str",
                Some(Type::Dict { .. }) => return "dict",
                Some(Type::Array(_)) => return "vec",
                Some(Type::Tuple(_)) => return "tuple",
                _ => {}
            }
        }
        Self::list_source_kind(arg)
    }

    /// Codegen dispatch kind for a `list()` / `sorted()` source value.
    fn list_source_kind(arg: &Value) -> &'static str {
        if let Value::Call { function, .. } = arg {
            if function == "range" {
                return "range";
            }
            return "vec";
        }
        match arg {
            Value::String(_) | Value::FormatString { .. } => "str",
            Value::Dict { .. } => "dict",
            Value::Tuple { .. } => "tuple",
            Value::Slice { .. } => "vec",
            Value::List { .. } | Value::ListComp { .. } => "vec",
            _ => "vec",
        }
    }

    fn lower_static_json_expr(&self, args: &[tarvos_ast::Expr]) -> Result<Value> {
        let [value] = args else {
            bail!("json.dumps() currently requires exactly one compile-time literal argument");
        };
        let Some(json) = static_json_expr(value) else {
            bail!(
                "json.dumps() requires a compile-time JSON literal in native mode; \
                 use --python-fallback for dynamic JSON values"
            );
        };
        Ok(Value::String(json))
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

/// Best-effort static type of an expression, resolving names through `names`.
///
/// The native backend is monomorphic, so a parameter that cannot be typed falls
/// back to `i64` and silently produces wrong Rust. Resolving variable names is
/// what lets an unannotated `def f(s): return s.lower()` bind `s: str` when it
/// is called as `f(word)` and `word` was itself assigned a string.
fn expression_type_with_names(expr: &tarvos_ast::Expr, names: &HashMap<String, Type>) -> Type {
    match expr {
        tarvos_ast::Expr::Name { id } => names.get(id).cloned().unwrap_or(Type::Unknown),
        tarvos_ast::Expr::Binary { left, right, .. } => binary_result_type(
            expression_type_with_names(left, names),
            expression_type_with_names(right, names),
        ),
        // `list(x)` / `sorted(x)` preserve the argument's element type, so
        // `y = list(range(3))` is `[int]` and `for c in list("ab")` binds `str`.
        tarvos_ast::Expr::Call { function, args, .. } if matches!(function.as_ref(), tarvos_ast::Expr::Name { id } if id == "list" || id == "sorted") => {
            Type::Array(Box::new(
                args.first()
                    .map(|arg| list_arg_element_type_with_names(arg, names))
                    .unwrap_or(Type::Unknown),
            ))
        }
        _ => expression_type(expr),
    }
}

/// Result type of a binary expression given its operand types.
///
/// Python promotes to `float` when either side is a float, and concatenation
/// keeps the string type, so both are resolved before falling back to the left
/// operand.
fn binary_result_type(left: Type, right: Type) -> Type {
    if left == Type::Float || right == Type::Float {
        Type::Float
    } else if left == Type::String || right == Type::String {
        Type::String
    } else {
        left
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
        tarvos_ast::Expr::ListComp { elt, .. } => Type::Array(Box::new(expression_type(elt))),
        tarvos_ast::Expr::Tuple { elements } => {
            Type::Tuple(elements.iter().map(expression_type).collect())
        }
        // Indexing a container yields its element type, which is what makes
        // `words = ["a", "b"]; word = words[i]` resolve to `str`.
        tarvos_ast::Expr::Subscript { value, .. } => match expression_type(value) {
            Type::Array(element) => *element,
            Type::Dict { value, .. } => *value,
            Type::String => Type::String,
            _ => Type::Unknown,
        },
        tarvos_ast::Expr::Compare { .. } | tarvos_ast::Expr::BoolOp { .. } => Type::Bool,
        tarvos_ast::Expr::Unary { operand, .. } => expression_type(operand),
        tarvos_ast::Expr::Binary { left, right, .. } => {
            binary_result_type(expression_type(left), expression_type(right))
        }
        _ => expression_type_fallback(expr),
    }
}

/// Element type of a `list(x)` / `sorted(x)` argument during the pre-pass.
fn list_arg_element_type_with_names(arg: &tarvos_ast::Expr, names: &HashMap<String, Type>) -> Type {
    if let tarvos_ast::Expr::Call { function, .. } = arg {
        if matches!(function.as_ref(), tarvos_ast::Expr::Name { id } if id == "range") {
            return Type::Int;
        }
    }
    if matches!(
        arg,
        tarvos_ast::Expr::String { .. } | tarvos_ast::Expr::FormatString { .. }
    ) {
        return Type::String;
    }
    match expression_type_with_names(arg, names) {
        Type::Array(element) => *element,
        Type::Dict { key, .. } => *key,
        Type::Tuple(elements) => {
            let mut uniform: Option<Type> = None;
            for element in elements {
                match &uniform {
                    None => uniform = Some(element),
                    Some(existing) if *existing == element => {}
                    // Heterogeneous tuples bail later in lowering; the pre-pass
                    // must not guess an element type here.
                    _ => return Type::Unknown,
                }
            }
            uniform.unwrap_or(Type::Unknown)
        }
        Type::String => Type::String,
        _ => Type::Unknown,
    }
}

/// Types for the call-shaped expressions, split out to keep `expression_type`
/// readable.
fn expression_type_fallback(expr: &tarvos_ast::Expr) -> Type {
    match expr {
        tarvos_ast::Expr::Call { function, args, .. } => match function.as_ref() {
            tarvos_ast::Expr::Name { id } => match id.as_str() {
                "str" | "chr" => Type::String,
                "int" | "len" | "ord" | "round" => Type::Int,
                "float" | "abs" => Type::Float,
                "bool" | "any" | "all" => Type::Bool,
                "range" => Type::Array(Box::new(Type::Int)),
                // The pre-pass resolves the argument through `names`, so this
                // literal-only path mirrors it without scope information.
                "list" | "sorted" => Type::Array(Box::new(
                    args.first()
                        .map(|arg| list_arg_element_type_with_names(arg, &HashMap::new()))
                        .unwrap_or(Type::Unknown),
                )),
                _ => Type::Unknown,
            },
            _ => Type::Unknown,
        },
        // A method call's result follows the builtin registry, so `s.lower()`
        // and `"".join(items)` both stay strings.
        tarvos_ast::Expr::MethodCall {
            object,
            method,
            args,
            ..
        } => match builtin_method_return_type(object, method) {
            // `split(None)` and `split(sep)` both yield a list of strings; the
            // registry already records that, but a bare-name receiver can be
            // guessed as a `List`, so the argument form is checked explicitly.
            Some((_, _)) if method == "split" && !args.is_empty() => {
                Type::Array(Box::new(Type::String))
            }
            Some((_, return_type)) => return_type,
            None => Type::Unknown,
        },
        _ => Type::Unknown,
    }
}

/// Static return type of a `str`/`list`/`dict` builtin for an unknown receiver.
///
/// The receiver is usually a name whose type is only known from the enclosing
/// scope, so both container kinds are tried and whichever the registry knows
/// wins. This only feeds type *inference*, so a wrong guess is re-checked by
/// the lowering stage rather than emitted.
fn builtin_method_return_type(
    object: &tarvos_ast::Expr,
    method: &str,
) -> Option<(MethodReceiver, Type)> {
    let candidates: &[MethodReceiver] = match object {
        tarvos_ast::Expr::String { .. } | tarvos_ast::Expr::FormatString { .. } => {
            return native_builtin_method(MethodReceiver::Str, method)
                .map(|entry| (MethodReceiver::Str, entry.return_type));
        }
        tarvos_ast::Expr::List { .. } | tarvos_ast::Expr::ListComp { .. } => {
            return native_builtin_method(MethodReceiver::List, method)
                .map(|entry| (MethodReceiver::List, entry.return_type));
        }
        tarvos_ast::Expr::Dict { .. } => {
            return native_builtin_method(MethodReceiver::Dict, method)
                .map(|entry| (MethodReceiver::Dict, entry.return_type));
        }
        _ => &[
            MethodReceiver::Str,
            MethodReceiver::List,
            MethodReceiver::Dict,
        ],
    };
    candidates.iter().find_map(|receiver| {
        native_builtin_method(*receiver, method).map(|entry| (*receiver, entry.return_type))
    })
}

/// Collect the static type of every variable assigned anywhere in the module.
///
/// This runs before lowering so an unannotated function's parameters can be
/// typed from their call sites even when the arguments are local variables
/// rather than literals.
fn collect_variable_types(statements: &[tarvos_ast::Stmt]) -> HashMap<String, Type> {
    let mut names: HashMap<String, Type> = HashMap::new();
    collect_variable_types_into(statements, &mut names, 0);
    collect_empty_list_element_types(statements, &mut names);
    names
}

fn collect_variable_types_into(
    statements: &[tarvos_ast::Stmt],
    names: &mut HashMap<String, Type>,
    depth: usize,
) {
    // Guard against pathologically nested sources; deeper scopes are skipped
    // rather than risking unbounded recursion.
    if depth > 8 {
        return;
    }
    for statement in statements {
        match statement {
            tarvos_ast::Stmt::Assign { target, value } => {
                let tarvos_ast::Expr::Name { id } = target else {
                    continue;
                };
                // An empty list literal has no element type of its own, so it is
                // recorded as unknown and upgraded from its first `append`.
                if matches!(value, tarvos_ast::Expr::List { elements } if elements.is_empty()) {
                    names
                        .entry(id.clone())
                        .or_insert_with(|| Type::Array(Box::new(Type::Unknown)));
                    continue;
                }
                let ty = expression_type_with_names(value, names);
                if ty != Type::Unknown {
                    names.insert(id.clone(), ty);
                }
            }
            tarvos_ast::Stmt::AnnAssign {
                target: tarvos_ast::Expr::Name { id },
                value,
                ..
            } => {
                let ty = value
                    .as_ref()
                    .map(|value| expression_type_with_names(value, names))
                    .unwrap_or(Type::Unknown);
                if ty != Type::Unknown {
                    names.insert(id.clone(), ty);
                }
            }
            // A loop variable has the iterable's element type, so a `for` over a
            // list of strings yields a string variable.
            tarvos_ast::Stmt::For {
                target, iter, body, ..
            } => {
                if let tarvos_ast::Expr::Name { id } = target {
                    let element = match expression_type_with_names(iter, names) {
                        Type::Array(inner) => *inner,
                        Type::Dict { key, .. } => *key,
                        Type::String => Type::String,
                        _ => Type::Unknown,
                    };
                    if element != Type::Unknown {
                        names.insert(id.clone(), element);
                    }
                }
                collect_variable_types_into(body, names, depth + 1);
            }
            tarvos_ast::Stmt::If { body, orelse, .. } => {
                collect_variable_types_into(body, names, depth + 1);
                collect_variable_types_into(orelse, names, depth + 1);
            }
            tarvos_ast::Stmt::While { body, .. }
            | tarvos_ast::Stmt::Try { body, .. }
            | tarvos_ast::Stmt::With { body, .. }
            | tarvos_ast::Stmt::FunctionDef { body, .. }
            | tarvos_ast::Stmt::ClassDef { body, .. } => {
                collect_variable_types_into(body, names, depth + 1)
            }
            _ => {}
        }
    }
}

/// Resolve `xs = []` followed by `xs.append(value)` to a concrete element type.
///
/// Without this an empty list literal would lower to `Vec<()>` and every
/// `append` would then emit a type error instead of native code.
fn collect_empty_list_element_types(
    statements: &[tarvos_ast::Stmt],
    names: &mut HashMap<String, Type>,
) {
    collect_empty_list_element_types_into(statements, names, 0)
}

fn collect_empty_list_element_types_into(
    statements: &[tarvos_ast::Stmt],
    names: &mut HashMap<String, Type>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    for statement in statements {
        // Nested scopes are visited too: an `append` inside a loop or an `if` is
        // just as likely to be the only evidence of the element type, and a
        // function that builds a list in a loop is an ordinary shape.
        let nested: &[&[tarvos_ast::Stmt]] = match statement {
            tarvos_ast::Stmt::If { body, orelse, .. } => &[body, orelse],
            tarvos_ast::Stmt::While { body, .. }
            | tarvos_ast::Stmt::For { body, .. }
            | tarvos_ast::Stmt::Try { body, .. }
            | tarvos_ast::Stmt::With { body, .. }
            | tarvos_ast::Stmt::FunctionDef { body, .. }
            | tarvos_ast::Stmt::ClassDef { body, .. } => &[body],
            _ => &[],
        };
        for body in nested {
            collect_empty_list_element_types_into(body, names, depth + 1);
        }

        let value = match statement {
            // Both `xs.append(v)` and the unusual `y = xs.append(v)` name the
            // receiver, and only the receiver matters here.
            tarvos_ast::Stmt::Expr { value } | tarvos_ast::Stmt::Assign { value, .. } => value,
            _ => continue,
        };
        let tarvos_ast::Expr::MethodCall {
            object,
            method,
            args,
            ..
        } = value
        else {
            continue;
        };
        if !matches!(method.as_str(), "append" | "extend") {
            continue;
        }
        let tarvos_ast::Expr::Name { id } = object.as_ref() else {
            continue;
        };
        let Some(argument) = args.first() else {
            continue;
        };
        let element = expression_type_with_names(argument, names);
        if element == Type::Unknown {
            continue;
        }
        if let Some(Type::Array(existing)) = names.get(id).cloned() {
            if *existing == Type::Unknown {
                names.insert(id.clone(), Type::Array(Box::new(element)));
            }
        }
    }
}

fn infer_function_argument_types(
    statements: &[tarvos_ast::Stmt],
    function_name: &str,
    arity: usize,
) -> Vec<Type> {
    let mut inferred = vec![Type::Unknown; arity];
    let names = collect_variable_types(statements);
    visit_call_sites(statements, function_name, &mut inferred, &names);
    inferred
        .into_iter()
        .map(|ty| if ty == Type::Unknown { Type::Int } else { ty })
        .collect()
}

/// Seed parameter types from every call site of `function_name` in the module.
fn visit_call_sites(
    statements: &[tarvos_ast::Stmt],
    function_name: &str,
    inferred: &mut [Type],
    names: &HashMap<String, Type>,
) {
    for statement in statements {
        match statement {
            tarvos_ast::Stmt::Expr { value }
            | tarvos_ast::Stmt::Return { value: Some(value) }
            | tarvos_ast::Stmt::Assign { value, .. }
            | tarvos_ast::Stmt::AugAssign { value, .. }
            | tarvos_ast::Stmt::AnnAssign {
                value: Some(value), ..
            } => infer_from_expr(value, function_name, inferred, names),
            tarvos_ast::Stmt::If { test, body, orelse } => {
                infer_from_expr(test, function_name, inferred, names);
                visit_call_sites(body, function_name, inferred, names);
                visit_call_sites(orelse, function_name, inferred, names);
            }
            tarvos_ast::Stmt::While { test, body } => {
                infer_from_expr(test, function_name, inferred, names);
                visit_call_sites(body, function_name, inferred, names);
            }
            tarvos_ast::Stmt::For { iter, body, .. } => {
                infer_from_expr(iter, function_name, inferred, names);
                visit_call_sites(body, function_name, inferred, names);
            }
            tarvos_ast::Stmt::FunctionDef { body, .. }
            | tarvos_ast::Stmt::ClassDef { body, .. } => {
                visit_call_sites(body, function_name, inferred, names)
            }
            _ => {}
        }
    }
}
/// Walk an expression tree, seeding parameter types from matching call sites.
fn infer_from_expr(
    expression: &tarvos_ast::Expr,
    function_name: &str,
    inferred: &mut [Type],
    names: &HashMap<String, Type>,
) {
    match expression {
        tarvos_ast::Expr::Call { function, args, .. } if matches!(function.as_ref(), tarvos_ast::Expr::Name { id } if id == function_name) => {
            for (index, argument) in args.iter().enumerate().take(inferred.len()) {
                let ty = expression_type_with_names(argument, names);
                if inferred[index] == Type::Unknown && ty != Type::Unknown {
                    inferred[index] = ty;
                }
            }
        }
        // Nested expressions are walked too, so a call hidden inside a container
        // literal, a comprehension or an f-string still contributes its types.
        tarvos_ast::Expr::Binary { left, right, .. } => {
            infer_from_expr(left, function_name, inferred, names);
            infer_from_expr(right, function_name, inferred, names);
        }
        tarvos_ast::Expr::Call { function, args, .. } => {
            infer_from_expr(function, function_name, inferred, names);
            for argument in args {
                infer_from_expr(argument, function_name, inferred, names);
            }
        }
        tarvos_ast::Expr::List { elements }
        | tarvos_ast::Expr::Tuple { elements }
        | tarvos_ast::Expr::Set { elements } => {
            for element in elements {
                infer_from_expr(element, function_name, inferred, names);
            }
        }
        tarvos_ast::Expr::ListComp { elt, iter, .. } => {
            infer_from_expr(elt, function_name, inferred, names);
            infer_from_expr(iter, function_name, inferred, names);
        }
        tarvos_ast::Expr::Subscript { value, index } => {
            infer_from_expr(value, function_name, inferred, names);
            infer_from_expr(index, function_name, inferred, names);
        }
        tarvos_ast::Expr::BoolOp { values, .. } => {
            for value in values {
                infer_from_expr(value, function_name, inferred, names);
            }
        }
        tarvos_ast::Expr::Compare { comparators, .. } => {
            for comparator in comparators {
                infer_from_expr(comparator, function_name, inferred, names);
            }
        }
        tarvos_ast::Expr::MethodCall { object, args, .. } => {
            infer_from_expr(object, function_name, inferred, names);
            for argument in args {
                infer_from_expr(argument, function_name, inferred, names);
            }
        }
        _ => {}
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

fn static_json_expr(value: &tarvos_ast::Expr) -> Option<String> {
    match value {
        tarvos_ast::Expr::Int { value } => Some(value.to_string()),
        tarvos_ast::Expr::BigInt { value } => Some(value.clone()),
        tarvos_ast::Expr::Float { value } if value.is_finite() => Some(value.to_string()),
        tarvos_ast::Expr::String { value } => Some(format!("\"{}\"", escape_json_string(value))),
        tarvos_ast::Expr::Bool { value } => Some(value.to_string()),
        tarvos_ast::Expr::None => Some("null".to_string()),
        tarvos_ast::Expr::List { elements } | tarvos_ast::Expr::Tuple { elements } => {
            let values = elements
                .iter()
                .map(static_json_expr)
                .collect::<Option<Vec<_>>>()?;
            Some(format!("[{}]", values.join(",")))
        }
        tarvos_ast::Expr::Dict { keys, values } => {
            let mut entries = Vec::with_capacity(keys.len());
            for (key, value) in keys.iter().zip(values) {
                let tarvos_ast::Expr::String { value: key } = key else {
                    return None;
                };
                entries.push(format!(
                    "\"{}\":{}",
                    escape_json_string(key),
                    static_json_expr(value)?
                ));
            }
            Some(format!("{{{}}}", entries.join(",")))
        }
        tarvos_ast::Expr::Unary { operator, operand } if operator == "usub" => {
            let value = static_json_expr(operand)?;
            (!value.starts_with('-')).then(|| format!("-{}", value))
        }
        _ => None,
    }
}

fn escape_json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write;
                let _ = write!(escaped, "\\u{:04x}", character as u32);
            }
            character => escaped.push(character),
        }
    }
    escaped
}
