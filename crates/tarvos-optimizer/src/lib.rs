mod specializer;

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tarvos_ir::{BinaryOp, Module, Stmt, Value};
use tarvos_types::Type;

pub use specializer::TypeSpecializer;

pub struct Optimizer;

impl Optimizer {
    /// Run optimization passes on the IR in dependency order
    pub fn optimize(module: &Module) -> Result<Module> {
        let mut optimized = module.clone();
        // 1. Loop induction closed-form reduction (O(N) -> O(1))
        optimized = Self::loop_induction_optimization(&optimized)?;
        // 2. Propagate copies and known constants across statements
        optimized = Self::copy_propagation(&optimized)?;
        // 3. Fold constant arithmetic expressions
        optimized = Self::constant_folding(&optimized)?;
        // 4. Second propagation pass after folding
        optimized = Self::copy_propagation(&optimized)?;
        optimized = Self::constant_folding(&optimized)?;
        // 5. Specialize types
        optimized = TypeSpecializer::specialize(&optimized)?;
        // 6. Eliminate unreachable branches and dead lets
        optimized = Self::dead_code_elimination(&optimized)?;
        Ok(optimized)
    }

    // ────────────────────────────────────────────────────────────────────────
    // Loop Induction Optimization: Closed-Form Formula Replacement
    // ────────────────────────────────────────────────────────────────────────

    fn loop_induction_optimization(module: &Module) -> Result<Module> {
        let statements = Self::optimize_loop_block(&module.statements);
        Ok(Module { statements })
    }

    fn optimize_loop_block(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut result = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::For { target, iter, body } => {
                    if let Some((start_val, end_val)) = Self::extract_range_bounds(iter) {
                        if body.len() == 1 {
                            if let Stmt::Assign {
                                name: acc_name,
                                value:
                                    Value::Binary {
                                        left,
                                        op: BinaryOp::Add,
                                        right,
                                        ..
                                    },
                            } = &body[0]
                            {
                                let is_acc_add_target = (matches!(left.as_ref(), Value::Name(n) if n == acc_name)
                                    && matches!(right.as_ref(), Value::Name(t) if t == target))
                                    || (matches!(right.as_ref(), Value::Name(n) if n == acc_name)
                                        && matches!(left.as_ref(), Value::Name(t) if t == target));

                                if is_acc_add_target {
                                    if let (Value::Int(start_i), Value::Int(end_i)) =
                                        (&start_val, &end_val)
                                    {
                                        let count = *end_i - *start_i;
                                        if count > 0 {
                                            // Gauss arithmetic series sum: count * (start + end - 1) / 2
                                            let sum_val = (count * (*start_i + *end_i - 1)) / 2;
                                            result.push(Stmt::Assign {
                                                name: acc_name.clone(),
                                                value: Value::Binary {
                                                    left: Box::new(Value::Name(acc_name.clone())),
                                                    op: BinaryOp::Add,
                                                    right: Box::new(Value::Int(sum_val)),
                                                    ty: Type::Int,
                                                },
                                            });
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    result.push(Stmt::For {
                        target: target.clone(),
                        iter: iter.clone(),
                        body: Self::optimize_loop_block(body),
                    });
                }
                Stmt::If { test, body, orelse } => {
                    result.push(Stmt::If {
                        test: test.clone(),
                        body: Self::optimize_loop_block(body),
                        orelse: Self::optimize_loop_block(orelse),
                    });
                }
                Stmt::While { test, body } => {
                    result.push(Stmt::While {
                        test: test.clone(),
                        body: Self::optimize_loop_block(body),
                    });
                }
                Stmt::Function {
                    name,
                    params,
                    return_type,
                    body,
                } => {
                    result.push(Stmt::Function {
                        name: name.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        body: Self::optimize_loop_block(body),
                    });
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    let handlers = handlers
                        .iter()
                        .map(|h| tarvos_ir::ExceptHandler {
                            name: h.name.clone(),
                            exc_type: h.exc_type.clone(),
                            body: Self::optimize_loop_block(&h.body),
                        })
                        .collect();
                    result.push(Stmt::Try {
                        body: Self::optimize_loop_block(body),
                        handlers,
                        orelse: Self::optimize_loop_block(orelse),
                        finalbody: Self::optimize_loop_block(finalbody),
                    });
                }
                Stmt::With { items, body } => {
                    result.push(Stmt::With {
                        items: items.clone(),
                        body: Self::optimize_loop_block(body),
                    });
                }
                other => result.push(other.clone()),
            }
        }
        result
    }

    fn extract_range_bounds(iter: &Value) -> Option<(Value, Value)> {
        if let Value::Call { function, args, .. } = iter {
            if function == "range" {
                if args.len() == 1 {
                    return Some((Value::Int(0), args[0].clone()));
                } else if args.len() == 2 {
                    return Some((args[0].clone(), args[1].clone()));
                }
            }
        }
        None
    }

    // ────────────────────────────────────────────────────────────────────────
    // Copy & Constant Propagation
    // ────────────────────────────────────────────────────────────────────────

    fn copy_propagation(module: &Module) -> Result<Module> {
        let statements = Self::propagate_block(&module.statements);
        Ok(Module { statements })
    }

    fn propagate_block(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut env: HashMap<String, Value> = HashMap::new();
        let mut result = Vec::new();

        for stmt in stmts {
            match stmt {
                Stmt::Let { name, ty, value } => {
                    let new_val = Self::substitute_value(value, &env);
                    if Self::is_constant_value(&new_val) {
                        env.insert(name.clone(), new_val.clone());
                    } else {
                        env.remove(name);
                    }
                    result.push(Stmt::Let {
                        name: name.clone(),
                        ty: ty.clone(),
                        value: new_val,
                    });
                }
                Stmt::Assign { name, value } => {
                    let new_val = Self::substitute_value(value, &env);
                    if Self::is_constant_value(&new_val) {
                        env.insert(name.clone(), new_val.clone());
                    } else {
                        env.remove(name);
                    }
                    result.push(Stmt::Assign {
                        name: name.clone(),
                        value: new_val,
                    });
                }
                Stmt::IndexAssign {
                    target,
                    index,
                    value,
                } => {
                    let new_idx = Self::substitute_value(index, &env);
                    let new_val = Self::substitute_value(value, &env);
                    env.remove(target);
                    result.push(Stmt::IndexAssign {
                        target: target.clone(),
                        index: new_idx,
                        value: new_val,
                    });
                }
                Stmt::ListAppend { target, value } => {
                    env.remove(target);
                    result.push(Stmt::ListAppend {
                        target: target.clone(),
                        value: Self::substitute_value(value, &env),
                    });
                }
                Stmt::Break => result.push(Stmt::Break),
                Stmt::Continue => result.push(Stmt::Continue),
                Stmt::Raise(val) => {
                    result.push(Stmt::Raise(
                        val.as_ref().map(|v| Self::substitute_value(v, &env)),
                    ));
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    for modified in Self::mutated_in_block(body)
                        .iter()
                        .chain(Self::mutated_in_block(orelse).iter())
                        .chain(Self::mutated_in_block(finalbody).iter())
                    {
                        env.remove(modified);
                    }
                    for h in handlers {
                        for modified in Self::mutated_in_block(&h.body) {
                            env.remove(&modified);
                        }
                    }
                    let new_body = Self::propagate_block(body);
                    let new_handlers = handlers
                        .iter()
                        .map(|h| tarvos_ir::ExceptHandler {
                            name: h.name.clone(),
                            exc_type: h.exc_type.clone(),
                            body: Self::propagate_block(&h.body),
                        })
                        .collect();
                    let new_orelse = Self::propagate_block(orelse);
                    let new_finalbody = Self::propagate_block(finalbody);
                    result.push(Stmt::Try {
                        body: new_body,
                        handlers: new_handlers,
                        orelse: new_orelse,
                        finalbody: new_finalbody,
                    });
                }
                Stmt::With { items, body } => {
                    let new_items = items
                        .iter()
                        .map(|item| {
                            if let Some(ref t) = item.target {
                                env.remove(t);
                            }
                            tarvos_ir::WithItem {
                                context_expr: Self::substitute_value(&item.context_expr, &env),
                                target: item.target.clone(),
                            }
                        })
                        .collect();
                    for modified in Self::mutated_in_block(body) {
                        env.remove(&modified);
                    }
                    let new_body = Self::propagate_block(body);
                    result.push(Stmt::With {
                        items: new_items,
                        body: new_body,
                    });
                }
                Stmt::Expr(value) => {
                    result.push(Stmt::Expr(Self::substitute_value(value, &env)));
                }
                Stmt::Print(values) => {
                    let new_vals = values
                        .iter()
                        .map(|v| Self::substitute_value(v, &env))
                        .collect();
                    result.push(Stmt::Print(new_vals));
                }
                Stmt::If { test, body, orelse } => {
                    let new_test = Self::substitute_value(test, &env);
                    let new_body = Self::propagate_block(body);
                    let new_orelse = Self::propagate_block(orelse);
                    for modified in Self::mutated_in_block(body)
                        .iter()
                        .chain(Self::mutated_in_block(orelse).iter())
                    {
                        env.remove(modified);
                    }
                    result.push(Stmt::If {
                        test: new_test,
                        body: new_body,
                        orelse: new_orelse,
                    });
                }
                Stmt::While { test, body } => {
                    for modified in Self::mutated_in_block(body) {
                        env.remove(&modified);
                    }
                    let new_test = Self::substitute_value(test, &env);
                    let new_body = Self::propagate_block(body);
                    result.push(Stmt::While {
                        test: new_test,
                        body: new_body,
                    });
                }
                Stmt::For { target, iter, body } => {
                    env.remove(target);
                    for modified in Self::mutated_in_block(body) {
                        env.remove(&modified);
                    }
                    let new_iter = Self::substitute_value(iter, &env);
                    let new_body = Self::propagate_block(body);
                    result.push(Stmt::For {
                        target: target.clone(),
                        iter: new_iter,
                        body: new_body,
                    });
                }
                Stmt::Function {
                    name,
                    params,
                    return_type,
                    body,
                } => {
                    result.push(Stmt::Function {
                        name: name.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        body: Self::propagate_block(body),
                    });
                }
                Stmt::Return(val) => {
                    result.push(Stmt::Return(
                        val.as_ref().map(|v| Self::substitute_value(v, &env)),
                    ));
                }
            }
        }

        result
    }

    fn substitute_value(value: &Value, env: &HashMap<String, Value>) -> Value {
        match value {
            Value::Name(id) => {
                if let Some(known) = env.get(id) {
                    known.clone()
                } else {
                    value.clone()
                }
            }
            Value::Unary { op, operand, ty } => Value::Unary {
                op: *op,
                operand: Box::new(Self::substitute_value(operand, env)),
                ty: ty.clone(),
            },
            Value::Binary {
                left,
                op,
                right,
                ty,
            } => Value::Binary {
                left: Box::new(Self::substitute_value(left, env)),
                op: *op,
                right: Box::new(Self::substitute_value(right, env)),
                ty: ty.clone(),
            },
            Value::Call {
                function,
                args,
                return_type,
            } => Value::Call {
                function: function.clone(),
                args: args
                    .iter()
                    .map(|a| Self::substitute_value(a, env))
                    .collect(),
                return_type: return_type.clone(),
            },
            Value::List {
                elements,
                element_type,
            } => Value::List {
                elements: elements
                    .iter()
                    .map(|e| Self::substitute_value(e, env))
                    .collect(),
                element_type: element_type.clone(),
            },
            Value::Index {
                container,
                index,
                element_type,
                container_type,
            } => Value::Index {
                container: Box::new(Self::substitute_value(container, env)),
                index: Box::new(Self::substitute_value(index, env)),
                element_type: element_type.clone(),
                container_type: container_type.clone(),
            },
            Value::Slice {
                container,
                lower,
                upper,
                step,
                container_type,
            } => Value::Slice {
                container: Box::new(Self::substitute_value(container, env)),
                lower: lower
                    .as_ref()
                    .map(|l| Box::new(Self::substitute_value(l, env))),
                upper: upper
                    .as_ref()
                    .map(|u| Box::new(Self::substitute_value(u, env))),
                step: step
                    .as_ref()
                    .map(|s| Box::new(Self::substitute_value(s, env))),
                container_type: container_type.clone(),
            },
            other => other.clone(),
        }
    }

    fn is_constant_value(value: &Value) -> bool {
        matches!(
            value,
            Value::Int(_) | Value::Float(_) | Value::String(_) | Value::Bool(_)
        )
    }

    fn mutated_in_block(stmts: &[Stmt]) -> HashSet<String> {
        let mut set = HashSet::new();
        for stmt in stmts {
            match stmt {
                Stmt::Assign { name, .. } => {
                    set.insert(name.clone());
                }
                Stmt::IndexAssign { target, .. } => {
                    set.insert(target.clone());
                }
                Stmt::If { body, orelse, .. } => {
                    set.extend(Self::mutated_in_block(body));
                    set.extend(Self::mutated_in_block(orelse));
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    set.extend(Self::mutated_in_block(body));
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    set.extend(Self::mutated_in_block(body));
                    for h in handlers {
                        set.extend(Self::mutated_in_block(&h.body));
                    }
                    set.extend(Self::mutated_in_block(orelse));
                    set.extend(Self::mutated_in_block(finalbody));
                }
                Stmt::With { items, body } => {
                    for item in items {
                        if let Some(ref t) = item.target {
                            set.insert(t.clone());
                        }
                    }
                    set.extend(Self::mutated_in_block(body));
                }
                _ => {}
            }
        }
        set
    }

    // ────────────────────────────────────────────────────────────────────────
    // Constant Folding
    // ────────────────────────────────────────────────────────────────────────

    fn constant_folding(module: &Module) -> Result<Module> {
        let statements = module
            .statements
            .iter()
            .map(Self::fold_stmt)
            .collect::<Result<Vec<_>>>()?;

        Ok(Module { statements })
    }

    fn fold_stmt(stmt: &Stmt) -> Result<Stmt> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let folded_value = Self::fold_value(value)?;
                Ok(Stmt::Let {
                    name: name.clone(),
                    ty: ty.clone(),
                    value: folded_value,
                })
            }
            Stmt::Assign { name, value } => {
                let folded_value = Self::fold_value(value)?;
                Ok(Stmt::Assign {
                    name: name.clone(),
                    value: folded_value,
                })
            }
            Stmt::IndexAssign {
                target,
                index,
                value,
            } => Ok(Stmt::IndexAssign {
                target: target.clone(),
                index: Self::fold_value(index)?,
                value: Self::fold_value(value)?,
            }),
            Stmt::ListAppend { target, value } => Ok(Stmt::ListAppend {
                target: target.clone(),
                value: Self::fold_value(value)?,
            }),
            Stmt::Break => Ok(Stmt::Break),
            Stmt::Continue => Ok(Stmt::Continue),
            Stmt::Raise(value) => Ok(Stmt::Raise(
                value.as_ref().map(Self::fold_value).transpose()?,
            )),
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                let handlers = handlers
                    .iter()
                    .map(|h| {
                        Ok(tarvos_ir::ExceptHandler {
                            name: h.name.clone(),
                            exc_type: h.exc_type.clone(),
                            body: h
                                .body
                                .iter()
                                .map(Self::fold_stmt)
                                .collect::<Result<Vec<_>>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let orelse = orelse
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                let finalbody = finalbody
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                })
            }
            Stmt::With { items, body } => {
                let items = items
                    .iter()
                    .map(|i| {
                        Ok(tarvos_ir::WithItem {
                            context_expr: Self::fold_value(&i.context_expr)?,
                            target: i.target.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::With { items, body })
            }
            Stmt::Expr(value) => Ok(Stmt::Expr(Self::fold_value(value)?)),
            Stmt::Print(values) => {
                let folded = values
                    .iter()
                    .map(Self::fold_value)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::Print(folded))
            }
            Stmt::If { test, body, orelse } => {
                let test = Self::fold_value(test)?;
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                let orelse = orelse
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;

                Ok(Stmt::If { test, body, orelse })
            }
            Stmt::While { test, body } => {
                let test = Self::fold_value(test)?;
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::While { test, body })
            }
            Stmt::For { target, iter, body } => {
                let iter = Self::fold_value(iter)?;
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::For {
                    target: target.clone(),
                    iter,
                    body,
                })
            }
            Stmt::Function {
                name,
                params,
                return_type,
                body,
            } => {
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::Function {
                    name: name.clone(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body,
                })
            }
            Stmt::Return(value) => {
                let folded = value.as_ref().map(Self::fold_value).transpose()?;
                Ok(Stmt::Return(folded))
            }
        }
    }

    fn fold_value(value: &Value) -> Result<Value> {
        match value {
            Value::Unary { op, operand, ty } => {
                let operand = Self::fold_value(operand)?;
                match (&op, &operand) {
                    (tarvos_ir::UnaryOp::Neg, Value::Int(i)) => return Ok(Value::Int(-i)),
                    (tarvos_ir::UnaryOp::Neg, Value::Float(f)) => return Ok(Value::Float(-f)),
                    (tarvos_ir::UnaryOp::Not, Value::Bool(b)) => return Ok(Value::Bool(!b)),
                    _ => {}
                }
                Ok(Value::Unary {
                    op: *op,
                    operand: Box::new(operand),
                    ty: ty.clone(),
                })
            }
            Value::Binary {
                left,
                op,
                right,
                ty,
            } => {
                let left = Self::fold_value(left)?;
                let right = Self::fold_value(right)?;

                if let (Value::Int(lv), Value::Int(rv)) = (&left, &right) {
                    if let Some(folded) = Self::fold_binary_const_int(*lv, *op, *rv) {
                        return Ok(Value::Int(folded));
                    }
                }

                if let (Value::Float(lv), Value::Float(rv)) = (&left, &right) {
                    if let Some(folded) = Self::fold_binary_const_float(*lv, *op, *rv) {
                        return Ok(Value::Float(folded));
                    }
                }

                Ok(Value::Binary {
                    left: Box::new(left),
                    op: *op,
                    right: Box::new(right),
                    ty: ty.clone(),
                })
            }
            Value::Call {
                function,
                args,
                return_type,
            } => {
                let args = args
                    .iter()
                    .map(Self::fold_value)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::Call {
                    function: function.clone(),
                    args,
                    return_type: return_type.clone(),
                })
            }
            Value::List {
                elements,
                element_type,
            } => {
                let elements = elements
                    .iter()
                    .map(Self::fold_value)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::List {
                    elements,
                    element_type: element_type.clone(),
                })
            }
            Value::Index {
                container,
                index,
                element_type,
                container_type,
            } => {
                let container = Self::fold_value(container)?;
                let index = Self::fold_value(index)?;
                if let (Value::List { elements, .. }, Value::Int(idx)) = (&container, &index) {
                    if *idx >= 0 && (*idx as usize) < elements.len() {
                        return Ok(elements[*idx as usize].clone());
                    }
                }
                Ok(Value::Index {
                    container: Box::new(container),
                    index: Box::new(index),
                    element_type: element_type.clone(),
                    container_type: container_type.clone(),
                })
            }
            Value::Slice {
                container,
                lower,
                upper,
                step,
                container_type,
            } => {
                let container = Self::fold_value(container)?;
                let lower = lower
                    .as_ref()
                    .map(|value| Self::fold_value(value))
                    .transpose()?
                    .map(Box::new);
                let upper = upper
                    .as_ref()
                    .map(|value| Self::fold_value(value))
                    .transpose()?
                    .map(Box::new);
                let step = step
                    .as_ref()
                    .map(|value| Self::fold_value(value))
                    .transpose()?
                    .map(Box::new);
                Ok(Value::Slice {
                    container: Box::new(container),
                    lower,
                    upper,
                    step,
                    container_type: container_type.clone(),
                })
            }
            _ => Ok(value.clone()),
        }
    }

    fn fold_binary_const_int(left: i64, op: BinaryOp, right: i64) -> Option<i64> {
        Some(match op {
            BinaryOp::Add => left.checked_add(right)?,
            BinaryOp::Sub => left.checked_sub(right)?,
            BinaryOp::Mul => left.checked_mul(right)?,
            BinaryOp::Div => {
                if right == 0 {
                    return None;
                }
                left / right
            }
            BinaryOp::Mod => {
                if right == 0 {
                    return None;
                }
                left % right
            }
            _ => return None,
        })
    }

    fn fold_binary_const_float(left: f64, op: BinaryOp, right: f64) -> Option<f64> {
        Some(match op {
            BinaryOp::Add => left + right,
            BinaryOp::Sub => left - right,
            BinaryOp::Mul => left * right,
            BinaryOp::Div => {
                if right == 0.0 {
                    return None;
                }
                left / right
            }
            _ => return None,
        })
    }

    // ────────────────────────────────────────────────────────────────────────
    // Dead Code Elimination
    // ────────────────────────────────────────────────────────────────────────

    fn dead_code_elimination(module: &Module) -> Result<Module> {
        let statements = Self::eliminate_block(&module.statements);
        Ok(Module { statements })
    }

    fn eliminate_block(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut simplified = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::If { test, body, orelse } => {
                    let test = Self::eliminate_value(test);
                    match test {
                        Value::Bool(true) => {
                            simplified.extend(Self::preserve_branch_bindings(body))
                        }
                        Value::Bool(false) => {
                            simplified.extend(Self::preserve_branch_bindings(orelse))
                        }
                        other => simplified.push(Stmt::If {
                            test: other,
                            body: Self::preserve_branch_bindings(body),
                            orelse: Self::preserve_branch_bindings(orelse),
                        }),
                    }
                }
                Stmt::While { test, body } => {
                    let test = Self::eliminate_value(test);
                    if matches!(test, Value::Bool(false)) {
                        continue;
                    }
                    simplified.push(Stmt::While {
                        test,
                        body: Self::preserve_branch_bindings(body),
                    });
                }
                other => simplified.push(Self::eliminate_stmt(other)),
            }
        }

        Self::remove_unused_lets(&simplified)
    }

    fn preserve_branch_bindings(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut simplified = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::If { test, body, orelse } => {
                    let test = Self::eliminate_value(test);
                    simplified.push(Stmt::If {
                        test,
                        body: Self::preserve_branch_bindings(body),
                        orelse: Self::preserve_branch_bindings(orelse),
                    });
                }
                Stmt::While { test, body } => {
                    simplified.push(Stmt::While {
                        test: Self::eliminate_value(test),
                        body: Self::preserve_branch_bindings(body),
                    });
                }
                Stmt::For { target, iter, body } => {
                    simplified.push(Stmt::For {
                        target: target.clone(),
                        iter: Self::eliminate_value(iter),
                        body: Self::preserve_branch_bindings(body),
                    });
                }
                Stmt::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    let handlers = handlers
                        .iter()
                        .map(|h| tarvos_ir::ExceptHandler {
                            name: h.name.clone(),
                            exc_type: h.exc_type.clone(),
                            body: Self::preserve_branch_bindings(&h.body),
                        })
                        .collect();
                    simplified.push(Stmt::Try {
                        body: Self::preserve_branch_bindings(body),
                        handlers,
                        orelse: Self::preserve_branch_bindings(orelse),
                        finalbody: Self::preserve_branch_bindings(finalbody),
                    });
                }
                Stmt::With { items, body } => {
                    simplified.push(Stmt::With {
                        items: items.clone(),
                        body: Self::preserve_branch_bindings(body),
                    });
                }
                other => simplified.push(Self::eliminate_stmt(other)),
            }
        }
        simplified
    }

    fn eliminate_stmt(stmt: &Stmt) -> Stmt {
        match stmt {
            Stmt::Let { name, ty, value } => Stmt::Let {
                name: name.clone(),
                ty: ty.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::Assign { name, value } => Stmt::Assign {
                name: name.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::IndexAssign {
                target,
                index,
                value,
            } => Stmt::IndexAssign {
                target: target.clone(),
                index: Self::eliminate_value(index),
                value: Self::eliminate_value(value),
            },
            Stmt::ListAppend { target, value } => Stmt::ListAppend {
                target: target.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::Break => Stmt::Break,
            Stmt::Continue => Stmt::Continue,
            Stmt::Raise(value) => Stmt::Raise(value.as_ref().map(Self::eliminate_value)),
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let handlers = handlers
                    .iter()
                    .map(|h| tarvos_ir::ExceptHandler {
                        name: h.name.clone(),
                        exc_type: h.exc_type.clone(),
                        body: Self::preserve_branch_bindings(&h.body),
                    })
                    .collect();
                Stmt::Try {
                    body: Self::preserve_branch_bindings(body),
                    handlers,
                    orelse: Self::preserve_branch_bindings(orelse),
                    finalbody: Self::preserve_branch_bindings(finalbody),
                }
            }
            Stmt::With { items, body } => {
                let items = items
                    .iter()
                    .map(|i| tarvos_ir::WithItem {
                        context_expr: Self::eliminate_value(&i.context_expr),
                        target: i.target.clone(),
                    })
                    .collect();
                Stmt::With {
                    items,
                    body: Self::preserve_branch_bindings(body),
                }
            }
            Stmt::Expr(value) => Stmt::Expr(Self::eliminate_value(value)),
            Stmt::Print(values) => Stmt::Print(values.iter().map(Self::eliminate_value).collect()),
            Stmt::If { test, body, orelse } => Stmt::If {
                test: Self::eliminate_value(test),
                body: Self::preserve_branch_bindings(body),
                orelse: Self::preserve_branch_bindings(orelse),
            },
            Stmt::While { test, body } => Stmt::While {
                test: Self::eliminate_value(test),
                body: Self::preserve_branch_bindings(body),
            },
            Stmt::For { target, iter, body } => Stmt::For {
                target: target.clone(),
                iter: Self::eliminate_value(iter),
                body: Self::preserve_branch_bindings(body),
            },
            Stmt::Function {
                name,
                params,
                return_type,
                body,
            } => Stmt::Function {
                name: name.clone(),
                params: params.clone(),
                return_type: return_type.clone(),
                body: Self::preserve_branch_bindings(body),
            },
            Stmt::Return(value) => Stmt::Return(value.as_ref().map(Self::eliminate_value)),
        }
    }

    fn eliminate_value(value: &Value) -> Value {
        match value {
            Value::Unary { op, operand, ty } => Value::Unary {
                op: *op,
                operand: Box::new(Self::eliminate_value(operand)),
                ty: ty.clone(),
            },
            Value::Binary {
                left,
                op,
                right,
                ty,
            } => Value::Binary {
                left: Box::new(Self::eliminate_value(left)),
                op: *op,
                right: Box::new(Self::eliminate_value(right)),
                ty: ty.clone(),
            },
            Value::Call {
                function,
                args,
                return_type,
            } => Value::Call {
                function: function.clone(),
                args: args.iter().map(Self::eliminate_value).collect(),
                return_type: return_type.clone(),
            },
            Value::List {
                elements,
                element_type,
            } => Value::List {
                elements: elements.iter().map(Self::eliminate_value).collect(),
                element_type: element_type.clone(),
            },
            Value::Index {
                container,
                index,
                element_type,
                container_type,
            } => Value::Index {
                container: Box::new(Self::eliminate_value(container)),
                index: Box::new(Self::eliminate_value(index)),
                element_type: element_type.clone(),
                container_type: container_type.clone(),
            },
            Value::Slice {
                container,
                lower,
                upper,
                step,
                container_type,
            } => Value::Slice {
                container: Box::new(Self::eliminate_value(container)),
                lower: lower.as_ref().map(|l| Box::new(Self::eliminate_value(l))),
                upper: upper.as_ref().map(|u| Box::new(Self::eliminate_value(u))),
                step: step.as_ref().map(|s| Box::new(Self::eliminate_value(s))),
                container_type: container_type.clone(),
            },
            other => other.clone(),
        }
    }

    fn remove_unused_lets(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut used_names = HashSet::new();
        let mut kept = Vec::new();

        for stmt in stmts.iter().rev() {
            match stmt {
                Stmt::Let { name, value, .. } => {
                    let references = Self::value_names(value);
                    let is_pure = Self::is_pure_value(value);
                    if !used_names.contains(name) && is_pure && !references.contains(name) {
                        continue;
                    }

                    let next_stmt = Self::eliminate_stmt(stmt);
                    used_names.extend(Self::stmt_reads(&next_stmt));
                    kept.push(next_stmt);
                }
                _ => {
                    let next_stmt = Self::eliminate_stmt(stmt);
                    used_names.extend(Self::stmt_reads(&next_stmt));
                    kept.push(next_stmt);
                }
            }
        }

        kept.reverse();
        kept
    }

    fn stmt_reads(stmt: &Stmt) -> HashSet<String> {
        match stmt {
            Stmt::Let { value, .. } => Self::value_names(value),
            Stmt::Assign { name, value } => {
                let mut names = Self::value_names(value);
                names.insert(name.clone());
                names
            }
            Stmt::IndexAssign {
                target,
                index,
                value,
            } => {
                let mut names = HashSet::new();
                names.insert(target.clone());
                names.extend(Self::value_names(index));
                names.extend(Self::value_names(value));
                names
            }
            Stmt::ListAppend { target, value } => {
                let mut names = Self::value_names(value);
                names.insert(target.clone());
                names
            }
            Stmt::Print(values) => {
                let mut names = HashSet::new();
                for v in values {
                    names.extend(Self::value_names(v));
                }
                names
            }
            Stmt::If { test, body, orelse } => {
                let mut names = Self::value_names(test);
                names.extend(Self::block_reads(body));
                names.extend(Self::block_reads(orelse));
                names
            }
            Stmt::While { test, body } => {
                let mut names = Self::value_names(test);
                names.extend(Self::block_reads(body));
                names
            }
            Stmt::For { iter, body, .. } => {
                let mut names = Self::value_names(iter);
                names.extend(Self::block_reads(body));
                names
            }
            Stmt::Function { body, .. } => Self::block_reads(body),
            Stmt::Return(value) => value.as_ref().map(Self::value_names).unwrap_or_default(),
            Stmt::Break => HashSet::new(),
            Stmt::Continue => HashSet::new(),
            Stmt::Raise(value) => value.as_ref().map(Self::value_names).unwrap_or_default(),
            Stmt::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let mut names = Self::block_reads(body);
                for h in handlers {
                    names.extend(Self::block_reads(&h.body));
                }
                names.extend(Self::block_reads(orelse));
                names.extend(Self::block_reads(finalbody));
                names
            }
            Stmt::With { items, body } => {
                let mut names = HashSet::new();
                for item in items {
                    names.extend(Self::value_names(&item.context_expr));
                }
                names.extend(Self::block_reads(body));
                names
            }
            Stmt::Expr(value) => Self::value_names(value),
        }
    }

    fn block_reads(stmts: &[Stmt]) -> HashSet<String> {
        let mut names = HashSet::new();
        for stmt in stmts {
            names.extend(Self::stmt_reads(stmt));
        }
        names
    }

    fn value_names(value: &Value) -> HashSet<String> {
        let mut names = HashSet::new();
        match value {
            Value::Name(name) => {
                names.insert(name.clone());
            }
            Value::Unary { operand, .. } => {
                names.extend(Self::value_names(operand));
            }
            Value::Binary { left, right, .. } => {
                names.extend(Self::value_names(left));
                names.extend(Self::value_names(right));
            }
            Value::Call { args, .. } => {
                for arg in args {
                    names.extend(Self::value_names(arg));
                }
            }
            Value::List { elements, .. } => {
                for element in elements {
                    names.extend(Self::value_names(element));
                }
            }
            Value::Index {
                container, index, ..
            } => {
                names.extend(Self::value_names(container));
                names.extend(Self::value_names(index));
            }
            Value::Slice {
                container,
                lower,
                upper,
                step,
                ..
            } => {
                names.extend(Self::value_names(container));
                if let Some(l) = lower {
                    names.extend(Self::value_names(l));
                }
                if let Some(u) = upper {
                    names.extend(Self::value_names(u));
                }
                if let Some(s) = step {
                    names.extend(Self::value_names(s));
                }
            }
            _ => {}
        }
        names
    }

    fn is_pure_value(value: &Value) -> bool {
        match value {
            Value::Int(_) | Value::Float(_) | Value::String(_) | Value::Bool(_) => true,
            Value::Name(_) => true,
            Value::Unary { operand, .. } => Self::is_pure_value(operand),
            Value::Binary { left, right, .. } => {
                Self::is_pure_value(left) && Self::is_pure_value(right)
            }
            Value::List { elements, .. } => elements.iter().all(Self::is_pure_value),
            Value::Tuple { elements, .. } => elements.iter().all(Self::is_pure_value),
            Value::Dict { keys, values, .. } => {
                keys.iter().all(Self::is_pure_value) && values.iter().all(Self::is_pure_value)
            }
            Value::Index {
                container, index, ..
            } => Self::is_pure_value(container) && Self::is_pure_value(index),
            Value::Slice {
                container,
                lower,
                upper,
                step,
                ..
            } => {
                Self::is_pure_value(container)
                    && lower.as_ref().map_or(true, |l| Self::is_pure_value(l))
                    && upper.as_ref().map_or(true, |u| Self::is_pure_value(u))
                    && step.as_ref().map_or(true, |s| Self::is_pure_value(s))
            }
            Value::FormatString { parts } => parts.iter().all(|part| match part {
                tarvos_ir::FormatPart::Literal(_) => true,
                tarvos_ir::FormatPart::Value(value) => Self::is_pure_value(value),
            }),
            Value::Call { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarvos_ir::{BinaryOp, Module, Stmt, Value};
    use tarvos_types::Type;

    #[test]
    fn removes_unused_literal_assignments() {
        let module = Module {
            statements: vec![
                Stmt::Let {
                    name: "unused".into(),
                    ty: Type::Int,
                    value: Value::Int(10),
                },
                Stmt::Let {
                    name: "keep".into(),
                    ty: Type::Int,
                    value: Value::Int(20),
                },
                Stmt::Print(vec![Value::Name("keep".into())]),
            ],
        };

        let optimized = Optimizer::dead_code_elimination(&module).unwrap();
        assert_eq!(optimized.statements.len(), 2);
        assert!(matches!(optimized.statements[0], Stmt::Let { ref name, .. } if name == "keep"));
        assert!(matches!(optimized.statements[1], Stmt::Print(ref v) if v.len() == 1));
    }

    #[test]
    fn resolves_constant_if_branches() {
        let module = Module {
            statements: vec![Stmt::If {
                test: Value::Bool(true),
                body: vec![
                    Stmt::Let {
                        name: "x".into(),
                        ty: Type::Int,
                        value: Value::Int(42),
                    },
                    Stmt::Print(vec![Value::Name("x".into())]),
                ],
                orelse: vec![
                    Stmt::Let {
                        name: "y".into(),
                        ty: Type::Int,
                        value: Value::Int(99),
                    },
                    Stmt::Print(vec![Value::Name("y".into())]),
                ],
            }],
        };

        let optimized = Optimizer::dead_code_elimination(&module).unwrap();
        assert_eq!(optimized.statements.len(), 2);
        assert!(matches!(optimized.statements[0], Stmt::Let { ref name, .. } if name == "x"));
    }

    #[test]
    fn preserves_branch_assignments_in_conditionals() {
        let module = Module {
            statements: vec![
                Stmt::Let {
                    name: "x".into(),
                    ty: Type::Int,
                    value: Value::Int(7),
                },
                Stmt::If {
                    test: Value::Binary {
                        left: Box::new(Value::Name("x".into())),
                        op: BinaryOp::Gt,
                        right: Box::new(Value::Int(5)),
                        ty: Type::Bool,
                    },
                    body: vec![Stmt::Assign {
                        name: "result".into(),
                        value: Value::Int(1),
                    }],
                    orelse: vec![Stmt::Assign {
                        name: "result".into(),
                        value: Value::Int(0),
                    }],
                },
                Stmt::Print(vec![Value::Name("result".into())]),
            ],
        };

        let optimized = Optimizer::optimize(&module).unwrap();
        assert!(
            matches!(optimized.statements[0], Stmt::If { ref body, ref orelse, .. } if !body.is_empty() && !orelse.is_empty())
        );
    }

    #[test]
    fn folds_constant_binary_operations() {
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Binary {
                left: Box::new(Value::Int(10)),
                op: BinaryOp::Add,
                right: Box::new(Value::Int(20)),
                ty: Type::Int,
            }])],
        };

        let optimized = Optimizer::constant_folding(&module).unwrap();
        assert!(
            matches!(optimized.statements[0], Stmt::Print(ref v) if matches!(v[0], Value::Int(30)))
        );
    }

    #[test]
    fn propagates_copies_and_folds_loop_induction() {
        // total = 0; for i in range(10): total += i; print(total)
        let module = Module {
            statements: vec![
                Stmt::Let {
                    name: "total".into(),
                    ty: Type::Int,
                    value: Value::Int(0),
                },
                Stmt::For {
                    target: "i".into(),
                    iter: Value::Call {
                        function: "range".into(),
                        args: vec![Value::Int(10)],
                        return_type: Type::Array(Box::new(Type::Int)),
                    },
                    body: vec![Stmt::Assign {
                        name: "total".into(),
                        value: Value::Binary {
                            left: Box::new(Value::Name("total".into())),
                            op: BinaryOp::Add,
                            right: Box::new(Value::Name("i".into())),
                            ty: Type::Int,
                        },
                    }],
                },
                Stmt::Print(vec![Value::Name("total".into())]),
            ],
        };

        let optimized = Optimizer::optimize(&module).unwrap();
        // 0 + sum(0..9) = 45. Should fold into let mut total = 45; print(total)
        let print_stmt = &optimized.statements.last().unwrap();
        assert!(
            matches!(print_stmt, Stmt::Print(ref v) if matches!(v[0], Value::Int(45)) || matches!(v[0], Value::Name(ref n) if n == "total"))
        );
    }
}
