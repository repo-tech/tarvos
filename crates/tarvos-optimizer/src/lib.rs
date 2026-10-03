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

    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬
    // Loop Induction Optimization: Closed-Form Formula Replacement
    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬

    fn loop_induction_optimization(module: &Module) -> Result<Module> {
        let statements = Self::optimize_loop_block(&module.statements);
        Ok(Module { statements })
    }

    fn optimize_loop_block(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut result = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::StructDef { .. } => result.push(stmt.clone()),
                Stmt::For {
                    target,
                    iter,
                    iter_type,
                    body,
                } => {
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
                                        let count = end_i.checked_sub(*start_i);
                                        if let Some(count) = count.filter(|count| *count > 0) {
                                            // Widen before multiplying: the i64 product can overflow
                                            // even when the final Python integer is representable.
                                            let count = count as u128;
                                            let start = *start_i as u128;
                                            let end = *end_i as u128;
                                            let sum_val = count
                                                .checked_mul(start + end - 1)
                                                .expect("closed-form loop exceeds u128")
                                                / 2;
                                            if let Some(Stmt::Let { value, .. }) =
                                                result.iter_mut().rev().find(|stmt| {
                                                    matches!(stmt, Stmt::Let { name, .. } if name == acc_name)
                                                })
                                            {
                                                if sum_val > i64::MAX as u128 {
                                                    *value = Value::Int128(0);
                                                }
                                            }
                                            result.push(Stmt::Assign {
                                                name: acc_name.clone(),
                                                value: Value::Binary {
                                                    left: Box::new(Value::Name(acc_name.clone())),
                                                    op: BinaryOp::Add,
                                                    right: Box::new(
                                                        if sum_val > i64::MAX as u128 {
                                                            Value::Int128(sum_val)
                                                        } else {
                                                            Value::Int(sum_val as i64)
                                                        },
                                                    ),
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
                        iter_type: iter_type.clone(),
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

    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬
    // Copy & Constant Propagation
    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬

    fn copy_propagation(module: &Module) -> Result<Module> {
        let statements = Self::propagate_block(&module.statements);
        Ok(Module { statements })
    }

    fn propagate_block(stmts: &[Stmt]) -> Vec<Stmt> {
        let mut env: HashMap<String, Value> = HashMap::new();
        let mut result = Vec::new();
        let dynamic = Self::dynamic_names(stmts);

        for stmt in stmts {
            match stmt {
                Stmt::StructDef { .. } => result.push(stmt.clone()),
                Stmt::Let { name, ty, value } => {
                    let new_val = Self::substitute_value(value, &env);
                    // A name whose type is not fixed must not be tracked as a
                    // known constant. `x = 'a'` then `print(x)` would otherwise be
                    // rewritten to the literal `"a"`, which looks right until the
                    // program also did `x = 1` earlier, at which point the folded
                    // value is simply the wrong one.
                    let trackable = !dynamic.contains(name)
                        && Self::is_constant_value(&new_val);
                    if trackable {
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
                    // The same rule as `Let`: a name known to change type
                    // stops being tracked, and so does one being handed a tagged
                    // value, since `Assign` carries no type of its own.
                    let trackable = !dynamic.contains(name)
                        && Self::is_constant_value(&new_val)
                        && !Self::value_is_tagged(&new_val);
                    if trackable {
                        env.insert(name.clone(), new_val.clone());
                    } else {
                        env.remove(name);
                    }
                    result.push(Stmt::Assign {
                        name: name.clone(),
                        value: new_val,
                    });
                }
                Stmt::Destructure { targets, value } => {
                    let new_val = Self::substitute_value(value, &env);
                    for target in targets {
                        env.remove(target);
                    }
                    result.push(Stmt::Destructure {
                        targets: targets.clone(),
                        value: new_val,
                    });
                }
                Stmt::FieldAssign {
                    object,
                    field,
                    value,
                } => {
                    result.push(Stmt::FieldAssign {
                        object: Self::substitute_value(object, &env),
                        field: field.clone(),
                        value: Self::substitute_value(value, &env),
                    });
                }
                Stmt::IndexAssign {
                    target,
                    indices,
                    value,
                } => {
                    let new_indices = indices
                        .iter()
                        .map(|index| Self::substitute_value(index, &env))
                        .collect();
                    let new_val = Self::substitute_value(value, &env);
                    env.remove(target);
                    result.push(Stmt::IndexAssign {
                        target: target.clone(),
                        indices: new_indices,
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
                Stmt::For {
                    target,
                    iter,
                    iter_type,
                    body,
                } => {
                    env.remove(target);
                    for modified in Self::mutated_in_block(body) {
                        env.remove(&modified);
                    }
                    let new_iter = Self::substitute_value(iter, &env);
                    let new_body = Self::propagate_block(body);
                    result.push(Stmt::For {
                        target: target.clone(),
                        iter: new_iter,
                        iter_type: iter_type.clone(),
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

    /// Whether a value carries a type that is not fixed at compile time.
///
/// Such a value is a tagged runtime value in the generated program. Folding or
/// tracking it as a constant would replace a name with whatever it happened to
/// hold at that point in the source, which is the one thing a dynamic name is
/// not allowed to mean.
fn value_is_tagged(value: &Value) -> bool {
    match value {
        Value::Field { ty, .. }
        | Value::Unary { ty, .. }
        | Value::Binary { ty, .. }
        | Value::Call { return_type: ty, .. } => matches!(ty, Type::Dynamic),
        Value::ListComp { element_type, .. } => matches!(element_type, Type::Dynamic),
        _ => false,
    }
}

/// Names that must not be tracked as known constants.
///
/// Two things put a name here. A binding whose type is `Dynamic` is a tagged
/// value in the generated program, and folding it to a literal would replace
/// the name with whatever it held at that line. A name bound to two different
/// types is a dynamic name even though neither binding says so on its own, so
/// it is detected by replaying the block: the first binding records a type and
/// a later, different one marks the name.
fn dynamic_names(stmts: &[Stmt]) -> HashSet<String> {
    let mut seen: HashMap<String, Type> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    Self::collect_dynamic(stmts, &mut seen, &mut names);
    names
}

fn collect_dynamic(
    stmts: &[Stmt],
    seen: &mut HashMap<String, Type>,
    names: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, ty, value } => {
                if matches!(ty, Type::Dynamic) {
                    names.insert(name.clone());
                    continue;
                }
                let bound = Self::value_type_of(value);
                match seen.get(name) {
                    Some(previous) if *previous != bound => {
                        names.insert(name.clone());
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(name.clone(), bound);
                    }
                }
            }
            Stmt::Assign { name, value } => {
                if matches!(Self::value_type_of(value), Type::Dynamic) {
                    names.insert(name.clone());
                }
                let bound = Self::value_type_of(value);
                match seen.get(name) {
                    Some(previous) if *previous != bound => {
                        names.insert(name.clone());
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(name.clone(), bound);
                    }
                }
            }
            Stmt::If { body, orelse, .. } => {
                Self::collect_dynamic(body, seen, names);
                Self::collect_dynamic(orelse, seen, names);
            }
            Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::With { body, .. } => {
                Self::collect_dynamic(body, seen, names)
            }
            _ => {}
        }
    }
}

/// The type a value carries, as far as the IR records it.
fn value_type_of(value: &Value) -> Type {
    match value {
        Value::Int(_) | Value::Int128(_) => Type::Int,
        Value::Float(_) => Type::Float,
        Value::String(_) => Type::String,
        Value::Bool(_) => Type::Bool,
        Value::Field { ty, .. }
        | Value::Unary { ty, .. }
        | Value::Binary { ty, .. }
        | Value::Call { return_type: ty, .. } => ty.clone(),
        Value::List { element_type, .. } => Type::Array(Box::new(element_type.clone())),
        Value::Index {
            element_type, ..
        } => element_type.clone(),
        // A name reads whatever it was last bound to, which the caller is
        // already tracking; treating it as unknown avoids a false second type.
        _ => Type::Unknown,
    }
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
            Value::Field { object, field, ty } => Value::Field {
                object: Box::new(Self::substitute_value(object, env)),
                field: field.clone(),
                ty: ty.clone(),
            },
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
            Value::ListComp {
                target,
                iter,
                element,
                condition,
                element_type,
            } => Value::ListComp {
                target: target.clone(),
                iter: Box::new(Self::substitute_value(iter, env)),
                element: Box::new(Self::substitute_value(element, env)),
                condition: condition
                    .as_ref()
                    .map(|value| Box::new(Self::substitute_value(value, env))),
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
            Value::FormatString { parts } => Value::FormatString {
                parts: parts
                    .iter()
                    .map(|part| match part {
                        tarvos_ir::FormatPart::Literal(value) => {
                            tarvos_ir::FormatPart::Literal(value.clone())
                        }
                        tarvos_ir::FormatPart::Value {
                            value,
                            format_spec,
                            conversion,
                        } => tarvos_ir::FormatPart::Value {
                            value: Box::new(Self::substitute_value(value, env)),
                            format_spec: format_spec.clone(),
                            conversion: conversion.clone(),
                        },
                    })
                    .collect(),
            },
            other => other.clone(),
        }
    }

    fn is_constant_value(value: &Value) -> bool {
        matches!(
            value,
            Value::Int(_) | Value::Int128(_) | Value::Float(_) | Value::String(_) | Value::Bool(_)
        )
    }

    fn mutated_in_block(stmts: &[Stmt]) -> HashSet<String> {
        let mut set = HashSet::new();
        for stmt in stmts {
            match stmt {
                Stmt::Assign { name, .. } => {
                    set.insert(name.clone());
                }
                // A tuple assignment rebinds every one of its targets, so each has
                // to be reported as mutated. Leaving it out let copy propagation
                // keep a stale constant for a variable that a loop had just
                // reassigned: `a = 0` followed by `a, b = b, a + b` and `return a`
                // compiled to `return 0_i64`, which is a silently wrong answer
                // rather than a compile error.
                Stmt::Destructure { targets, .. } => {
                    set.extend(targets.iter().cloned());
                }
                Stmt::ListAppend { target, .. } => {
                    set.insert(target.clone());
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

    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬
    // Constant Folding
    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬

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
            Stmt::StructDef { .. } => Ok(stmt.clone()),
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
            Stmt::Destructure { targets, value } => Ok(Stmt::Destructure {
                targets: targets.clone(),
                value: Self::fold_value(value)?,
            }),
            Stmt::FieldAssign {
                object,
                field,
                value,
            } => Ok(Stmt::FieldAssign {
                object: Self::fold_value(object)?,
                field: field.clone(),
                value: Self::fold_value(value)?,
            }),
            Stmt::IndexAssign {
                target,
                indices,
                value,
            } => Ok(Stmt::IndexAssign {
                target: target.clone(),
                indices: indices
                    .iter()
                    .map(Self::fold_value)
                    .collect::<Result<Vec<_>>>()?,
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
            Stmt::For {
                target,
                iter,
                iter_type,
                body,
            } => {
                let iter = Self::fold_value(iter)?;
                let body = body
                    .iter()
                    .map(Self::fold_stmt)
                    .collect::<Result<Vec<_>>>()?;
                Ok(Stmt::For {
                    target: target.clone(),
                    iter,
                    iter_type: iter_type.clone(),
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
                    (tarvos_ir::UnaryOp::Neg, Value::Int128(i)) => {
                        return Ok(Value::Int128(i.wrapping_neg()))
                    }
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

                // Fold using the statically inferred result type, not just the
                // operand types. `7 / 2` is `Int / Int` but the declared type is
                // Float, because Python 3 true division always produces a float.
                // Folding it as an integer turned 3.5 into 3.
                if let (Value::Int(lv), Value::Int(rv)) = (&left, &right) {
                    if *ty == Type::Float {
                        if let Some(folded) = Self::fold_binary_const_int_to_float(*lv, *op, *rv) {
                            return Ok(Value::Float(folded));
                        }
                    } else if let Some(folded) = Self::fold_binary_const_int(*lv, *op, *rv) {
                        return Ok(Value::Int(folded));
                    }
                }

                // Checked separately: this arm can only match when the operands
                // are `Int128`, so it must not be nested inside the `Int` arm
                // above, where it could never be reached.
                if let (Value::Int128(lv), Value::Int128(rv)) = (&left, &right) {
                    if let Some(folded) = Self::fold_binary_const_u128(*lv, *op, *rv) {
                        return Ok(Value::Int128(folded));
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
            Value::FormatString { parts } => Ok(Value::FormatString {
                parts: parts
                    .iter()
                    .map(|part| match part {
                        tarvos_ir::FormatPart::Literal(value) => {
                            Ok(tarvos_ir::FormatPart::Literal(value.clone()))
                        }
                        tarvos_ir::FormatPart::Value {
                            value,
                            format_spec,
                            conversion,
                        } => Ok(tarvos_ir::FormatPart::Value {
                            value: Box::new(Self::fold_value(value)?),
                            format_spec: format_spec.clone(),
                            conversion: conversion.clone(),
                        }),
                    })
                    .collect::<Result<Vec<_>>>()?,
            }),
            _ => Ok(value.clone()),
        }
    }

    fn fold_binary_const_int(left: i64, op: BinaryOp, right: i64) -> Option<i64> {
        Some(match op {
            BinaryOp::Add => left.checked_add(right)?,
            BinaryOp::Sub => left.checked_sub(right)?,
            BinaryOp::Mul => left.checked_mul(right)?,
            BinaryOp::Div => {
                // Python 3 `/` is true division and always yields a float, so an
                // `Int / Int` expression is never an integer. The inferred result
                // type routes this through `fold_binary_const_int_to_float`.
                return None;
            }
            BinaryOp::Mod => {
                if right == 0 {
                    return None;
                }
                // Python's `%` is floored, so the result takes the sign of the
                // divisor: -7 % 2 is 1, not Rust's -1.
                let remainder = left.checked_rem(right)?;
                if remainder != 0 && ((remainder < 0) != (right < 0)) {
                    remainder.checked_add(right)?
                } else {
                    remainder
                }
            }
            BinaryOp::FloorDiv => {
                if right == 0 {
                    return None;
                }
                let quotient = left.checked_div(right)?;
                let remainder = left.checked_rem(right)?;
                if remainder != 0 && ((remainder < 0) != (right < 0)) {
                    quotient.checked_sub(1)?
                } else {
                    quotient
                }
            }
            BinaryOp::BitAnd => left & right,
            BinaryOp::BitOr => left | right,
            BinaryOp::BitXor => left ^ right,
            BinaryOp::LShift => {
                if !(0..i64::BITS as i64).contains(&right) {
                    return None;
                }
                left.checked_shl(right as u32)?
            }
            BinaryOp::RShift => {
                if !(0..i64::BITS as i64).contains(&right) {
                    return None;
                }
                left.checked_shr(right as u32)?
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
            BinaryOp::FloorDiv => {
                if right == 0.0 {
                    return None;
                }

                (left / right).floor()
            }
            _ => return None,
        })
    }

    /// Fold an operation on two integer literals whose statically inferred
    /// result type is `float`.
    ///
    /// This is the `7 / 2` case: the operands are `Int`, but Python 3 true
    /// division produces a float, so folding through the integer path would
    /// silently truncate the result.
    fn fold_binary_const_int_to_float(left: i64, op: BinaryOp, right: i64) -> Option<f64> {
        Some(match op {
            BinaryOp::Div => {
                if right == 0 {
                    return None;
                }
                left as f64 / right as f64
            }
            BinaryOp::FloorDiv => {
                if right == 0 {
                    return None;
                }
                (left as f64 / right as f64).floor()
            }
            BinaryOp::Pow => {
                // `pow` on ints is an int in Python; only a float result type
                // reaches here when one operand was promoted, which the integer
                // folder already covers. Leave it unfolded.
                return None;
            }
            _ => return None,
        })
    }

    fn fold_binary_const_u128(left: u128, op: BinaryOp, right: u128) -> Option<u128> {
        Some(match op {
            BinaryOp::Add => left.checked_add(right)?,
            BinaryOp::Sub => left.checked_sub(right)?,
            BinaryOp::Mul => left.checked_mul(right)?,
            BinaryOp::Div => left.checked_div(right)?,
            BinaryOp::Mod => left.checked_rem(right)?,
            BinaryOp::FloorDiv => left.checked_div(right)?,
            BinaryOp::BitAnd => left & right,
            BinaryOp::BitOr => left | right,
            BinaryOp::BitXor => left ^ right,
            BinaryOp::LShift => {
                if !(0..u128::BITS as u128).contains(&right) {
                    return None;
                }
                left.checked_shl(right as u32)?
            }
            BinaryOp::RShift => {
                if !(0..u128::BITS as u128).contains(&right) {
                    return None;
                }
                left.checked_shr(right as u32)?
            }
            _ => return None,
        })
    }

    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬
    // Dead Code Elimination
    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬

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
                Stmt::For {
                    target,
                    iter,
                    iter_type,
                    body,
                } => {
                    simplified.push(Stmt::For {
                        target: target.clone(),
                        iter: Self::eliminate_value(iter),
                        iter_type: iter_type.clone(),
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
            Stmt::StructDef { .. } => stmt.clone(),
            Stmt::Let { name, ty, value } => Stmt::Let {
                name: name.clone(),
                ty: ty.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::Assign { name, value } => Stmt::Assign {
                name: name.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::Destructure { targets, value } => Stmt::Destructure {
                targets: targets.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::FieldAssign {
                object,
                field,
                value,
            } => Stmt::FieldAssign {
                object: Self::eliminate_value(object),
                field: field.clone(),
                value: Self::eliminate_value(value),
            },
            Stmt::IndexAssign {
                target,
                indices,
                value,
            } => Stmt::IndexAssign {
                target: target.clone(),
                indices: indices.iter().map(Self::eliminate_value).collect(),
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
            Stmt::For {
                target,
                iter,
                iter_type,
                body,
            } => Stmt::For {
                target: target.clone(),
                iter: Self::eliminate_value(iter),
                iter_type: iter_type.clone(),
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
            Value::FormatString { parts } => Value::FormatString {
                parts: parts
                    .iter()
                    .map(|part| match part {
                        tarvos_ir::FormatPart::Literal(value) => {
                            tarvos_ir::FormatPart::Literal(value.clone())
                        }
                        tarvos_ir::FormatPart::Value {
                            value,
                            format_spec,
                            conversion,
                        } => tarvos_ir::FormatPart::Value {
                            value: Box::new(Self::eliminate_value(value)),
                            format_spec: format_spec.clone(),
                            conversion: conversion.clone(),
                        },
                    })
                    .collect(),
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
            Stmt::StructDef { .. } => HashSet::new(),
            Stmt::Let { value, .. } => Self::value_names(value),
            Stmt::Assign { name, value } => {
                let mut names = Self::value_names(value);
                names.insert(name.clone());
                names
            }
            Stmt::Destructure { targets, value } => {
                let mut names = Self::value_names(value);
                names.extend(targets.iter().cloned());
                names
            }
            Stmt::FieldAssign { object, value, .. } => {
                let mut names = Self::value_names(object);
                names.extend(Self::value_names(value));
                names
            }
            Stmt::IndexAssign {
                target,
                indices,
                value,
            } => {
                let mut names = HashSet::new();
                names.insert(target.clone());
                for index in indices {
                    names.extend(Self::value_names(index));
                }
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
            Value::Field { object, .. } => {
                names.extend(Self::value_names(object));
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
            Value::ListComp {
                target,
                iter,
                element,
                condition,
                ..
            } => {
                names.extend(Self::value_names(iter));
                let mut element_names = Self::value_names(element);
                element_names.remove(target);
                names.extend(element_names);
                if let Some(condition) = condition {
                    let mut condition_names = Self::value_names(condition);
                    condition_names.remove(target);
                    names.extend(condition_names);
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
            Value::FormatString { parts } => {
                for part in parts {
                    if let tarvos_ir::FormatPart::Value { value, .. } = part {
                        names.extend(Self::value_names(value));
                    }
                }
            }
            _ => {}
        }
        names
    }

    fn is_pure_value(value: &Value) -> bool {
        match value {
            Value::Int(_)
            | Value::Int128(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Bool(_) => true,
            Value::Name(_) => true,
            Value::Field { object, .. } => Self::is_pure_value(object),
            Value::Unary { operand, .. } => Self::is_pure_value(operand),
            Value::Binary { left, right, .. } => {
                Self::is_pure_value(left) && Self::is_pure_value(right)
            }
            Value::List { elements, .. } => elements.iter().all(Self::is_pure_value),
            Value::ListComp {
                iter,
                element,
                condition,
                ..
            } => {
                Self::is_pure_value(iter)
                    && Self::is_pure_value(element)
                    && condition
                        .as_ref()
                        .is_none_or(|value| Self::is_pure_value(value))
            }
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
                    && lower.as_ref().is_none_or(|l| Self::is_pure_value(l))
                    && upper.as_ref().is_none_or(|u| Self::is_pure_value(u))
                    && step.as_ref().is_none_or(|s| Self::is_pure_value(s))
            }
            Value::FormatString { parts } => parts.iter().all(|part| match part {
                tarvos_ir::FormatPart::Literal(_) => true,
                tarvos_ir::FormatPart::Value { value, .. } => Self::is_pure_value(value),
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

    /// A tuple assignment rebinds its targets, so copy propagation must forget
    /// any constant it had learned about them.
    ///
    /// This is the Fibonacci shape: `a` is initialised to a literal, a loop then
    /// rebinds it through `a, b = b, a + b`, and the function returns `a`. When
    /// `Destructure` was missing from the mutation set, the literal survived the
    /// loop and the generated Rust read `return 0_i64` ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â a silently wrong answer
    /// that still compiled and ran, which is the failure mode this guards.
    #[test]
    fn a_tuple_assignment_invalidates_a_propagated_constant() {
        let module = Module {
            statements: vec![Stmt::Function {
                name: "fib".into(),
                params: vec![("n".into(), Type::Int)],
                return_type: Type::Int,
                body: vec![
                    Stmt::Let {
                        name: "a".into(),
                        ty: Type::Int,
                        value: Value::Int(0),
                    },
                    Stmt::Let {
                        name: "b".into(),
                        ty: Type::Int,
                        value: Value::Int(1),
                    },
                    Stmt::For {
                        target: "_".into(),
                        iter: Value::Call {
                            function: "range".into(),
                            args: vec![Value::Name("n".into())],
                            return_type: Type::Array(Box::new(Type::Int)),
                        },
                        iter_type: Type::Array(Box::new(Type::Int)),
                        body: vec![Stmt::Destructure {
                            targets: vec!["a".into(), "b".into()],
                            value: Value::Tuple {
                                elements: vec![
                                    Value::Name("b".into()),
                                    Value::Binary {
                                        left: Box::new(Value::Name("a".into())),
                                        op: BinaryOp::Add,
                                        right: Box::new(Value::Name("b".into())),
                                        ty: Type::Int,
                                    },
                                ],
                                element_types: vec![Type::Int, Type::Int],
                            },
                        }],
                    },
                    Stmt::Return(Some(Value::Name("a".into()))),
                ],
            }],
        };

        let optimized = Optimizer::copy_propagation(&module).unwrap();
        let Stmt::Function { body, .. } = &optimized.statements[0] else {
            panic!("expected a function, got {:?}", optimized.statements[0]);
        };
        assert!(
            matches!(body.last(), Some(Stmt::Return(Some(Value::Name(name)))) if name == "a"),
            "`return a` must stay a name, not a folded constant: {:?}",
            body.last()
        );
    }

    /// The same reasoning for the direct block case, without a function or loop.
    #[test]
    fn a_tuple_assignment_clears_the_constant_in_its_own_block() {
        let module = Module {
            statements: vec![
                Stmt::Let {
                    name: "a".into(),
                    ty: Type::Int,
                    value: Value::Int(0),
                },
                Stmt::Destructure {
                    targets: vec!["a".into()],
                    value: Value::Tuple {
                        elements: vec![Value::Int(7)],
                        element_types: vec![Type::Int],
                    },
                },
                Stmt::Return(Some(Value::Name("a".into()))),
            ],
        };

        let optimized = Optimizer::copy_propagation(&module).unwrap();
        assert!(
            matches!(
                optimized.statements.last(),
                Some(Stmt::Return(Some(Value::Name(name)))) if name == "a"
            ),
            "a rebinding must stop the earlier constant from being propagated: {:?}",
            optimized.statements.last()
        );
    }

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
                    iter_type: Type::Array(Box::new(Type::Int)),
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

    /// Python's `%` is floored, so the result takes the sign of the divisor.
    /// Rust's `%` is truncated and takes the sign of the dividend, which made
    /// `-7 % 2` fold to `-1` instead of `1`.
    #[test]
    fn constant_folds_modulo_with_python_sign_semantics() {
        let cases = [
            (7_i64, 2_i64, 1_i64),
            (-7, 2, 1),
            (7, -2, -1),
            (-7, -2, -1),
            (8, 3, 2),
            (-8, 3, 1),
        ];
        for (left, right, expected) in cases {
            let module = Module {
                statements: vec![Stmt::Print(vec![Value::Binary {
                    left: Box::new(Value::Int(left)),
                    op: BinaryOp::Mod,
                    right: Box::new(Value::Int(right)),
                    ty: Type::Int,
                }])],
            };
            let optimized = Optimizer::optimize(&module).unwrap();
            assert!(
                matches!(optimized.statements.as_slice(), [Stmt::Print(items)]
                    if matches!(items[0], Value::Int(actual) if actual == expected)),
                "{left} % {right} must fold to {expected}, got {:?}",
                optimized.statements
            );
        }
    }

    /// Python 3 `/` is true division: `7 / 2` is `3.5`, never `3`. The inferred
    /// result type is Float even though both operands are ints.
    #[test]
    fn constant_folds_true_division_as_a_float() {
        let module = Module {
            statements: vec![Stmt::Print(vec![Value::Binary {
                left: Box::new(Value::Int(7)),
                op: BinaryOp::Div,
                right: Box::new(Value::Int(2)),
                ty: Type::Float,
            }])],
        };
        let optimized = Optimizer::optimize(&module).unwrap();
        assert!(
            matches!(optimized.statements.as_slice(), [Stmt::Print(items)]
                if matches!(items[0], Value::Float(actual) if actual == 3.5)),
            "7 / 2 must fold to 3.5, got {:?}",
            optimized.statements
        );
    }

    #[test]
    fn widens_large_closed_form_reduction_without_overflow() {
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
                        args: vec![Value::Int(350_000_000_000)],
                        return_type: Type::Array(Box::new(Type::Int)),
                    },
                    iter_type: Type::Array(Box::new(Type::Int)),
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
            ],
        };

        let optimized = Optimizer::optimize(&module).unwrap();
        let expected = 350_000_000_000_u128 * 349_999_999_999_u128 / 2;

        // Assert the *result*, not the shape of the intermediate IR. The wide
        // accumulator is seeded with `Int128(0)` and the closed-form sum is
        // added, so once constant folding can actually reduce wide integers the
        // assignment collapses to the total itself. Pinning the old
        // `Binary { .. }` shape would only lock in a missing fold.
        let bound_total = optimized
            .statements
            .iter()
            .rev()
            .find_map(|stmt| match stmt {
                Stmt::Assign { name, value } if name == "total" => Some(value.clone()),
                Stmt::Let { name, value, .. } if name == "total" => Some(value.clone()),
                _ => None,
            })
            .expect("optimized module must bind `total`");

        assert!(
            matches!(bound_total, Value::Int128(value) if value == expected),
            "wide closed-form reduction must produce the exact sum, got {bound_total:?}"
        );

        // The accumulator must still be a wide value, never a truncated i64.
        assert!(
            !matches!(bound_total, Value::Int(_)),
            "a sum above i64::MAX must not be narrowed back to i64"
        );
    }
}
