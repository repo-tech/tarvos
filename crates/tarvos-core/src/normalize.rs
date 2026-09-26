//! AST-level desugaring shared by both front ends.
//!
//! The shared IR models a narrower set of constructs than Python has. Rather
//! than teach every backend about Python-only syntax, the constructs are
//! rewritten here into the equivalent statements the IR already supports. The
//! rewrites are semantics-preserving: each one is valid Python on its own.

use std::collections::HashSet;

use tarvos_ast::{Expr, Stmt};

/// Apply every body-level rewrite, recursing into nested statement lists.
pub fn normalize_body(body: Vec<Stmt>, counter: &mut usize) -> Vec<Stmt> {
    let mut taken = HashSet::new();
    collect_bound_names(&body, &mut taken);
    body.into_iter()
        .flat_map(|stmt| normalize_stmt(stmt, counter, &mut taken))
        .collect()
}

/// Collect every name a scope binds, so generated temporaries cannot shadow one.
fn collect_bound_names(body: &[Stmt], names: &mut HashSet<String>) {
    for statement in body {
        match statement {
            Stmt::Assign { target, .. }
            | Stmt::AugAssign { target, .. }
            | Stmt::AnnAssign { target, .. } => collect_target_names(target, names),
            Stmt::For { target, .. } => collect_target_names(target, names),
            Stmt::FunctionDef { name, args, .. } => {
                names.insert(name.clone());
                names.extend(args.iter().cloned());
            }
            Stmt::ClassDef { name, .. } => {
                names.insert(name.clone());
            }
            Stmt::Import { names: imports } => {
                for import in imports {
                    names.insert(import.asname.clone().unwrap_or_else(|| import.name.clone()));
                }
            }
            Stmt::Global { names: globals } | Stmt::Nonlocal { names: globals } => {
                names.extend(globals.iter().cloned());
            }
            _ => {}
        }
    }
}

fn collect_target_names(target: &Expr, names: &mut HashSet<String>) {
    match target {
        Expr::Name { id } => {
            names.insert(id.clone());
        }
        Expr::Tuple { elements } | Expr::List { elements } => {
            for element in elements {
                collect_target_names(element, names);
            }
        }
        _ => {}
    }
}

fn normalize_stmt(stmt: Stmt, counter: &mut usize, taken: &mut HashSet<String>) -> Vec<Stmt> {
    match stmt {
        Stmt::Assign { target, value } => {
            let target = normalize_expr(target, counter, taken);
            let value = normalize_expr(value, counter, taken);
            expand_parallel_assignment(target, value, counter, taken)
        }

        Stmt::AugAssign {
            target,
            operator,
            value,
        } => vec![Stmt::AugAssign {
            target: normalize_expr(target, counter, taken),
            operator,
            value: normalize_expr(value, counter, taken),
        }],

        Stmt::AnnAssign {
            target,
            annotation,
            value,
        } => vec![Stmt::AnnAssign {
            target: normalize_expr(target, counter, taken),
            annotation,
            value: value.map(|value| normalize_expr(value, counter, taken)),
        }],

        Stmt::Expr { value } => vec![Stmt::Expr {
            value: normalize_expr(value, counter, taken),
        }],

        Stmt::If { test, body, orelse } => vec![Stmt::If {
            test: normalize_expr(test, counter, taken),
            body: normalize_body(body, counter),
            orelse: normalize_body(orelse, counter),
        }],

        Stmt::While { test, body } => vec![Stmt::While {
            test: normalize_expr(test, counter, taken),
            body: normalize_body(body, counter),
        }],

        Stmt::For { target, iter, body } => vec![Stmt::For {
            target: normalize_expr(target, counter, taken),
            iter: normalize_expr(iter, counter, taken),
            body: normalize_body(body, counter),
        }],

        Stmt::FunctionDef {
            name,
            args,
            arg_annotations,
            body,
            returns,
        } => vec![Stmt::FunctionDef {
            name,
            args,
            arg_annotations,
            body: normalize_body(body, counter),
            returns,
        }],

        Stmt::ClassDef { name, bases, body } => vec![Stmt::ClassDef {
            name,
            bases,
            body: normalize_body(body, counter),
        }],

        Stmt::Return { value } => vec![Stmt::Return {
            value: value.map(|value| normalize_expr(value, counter, taken)),
        }],

        Stmt::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => vec![Stmt::Try {
            body: normalize_body(body, counter),
            handlers: handlers
                .into_iter()
                .map(|handler| tarvos_ast::ExceptHandler {
                    name: handler.name,
                    exc_type: handler
                        .exc_type
                        .map(|exc| normalize_expr(exc, counter, taken)),
                    body: normalize_body(handler.body, counter),
                })
                .collect(),
            orelse: normalize_body(orelse, counter),
            finalbody: normalize_body(finalbody, counter),
        }],

        Stmt::With { items, body } => vec![Stmt::With {
            items: items
                .into_iter()
                .map(|item| tarvos_ast::WithItem {
                    context_expr: normalize_expr(item.context_expr, counter, taken),
                    optional_vars: item
                        .optional_vars
                        .map(|vars| normalize_expr(vars, counter, taken)),
                })
                .collect(),
            body: normalize_body(body, counter),
        }],

        other => vec![other],
    }
}

fn normalize_expr(expr: Expr, counter: &mut usize, taken: &mut HashSet<String>) -> Expr {
    match expr {
        Expr::Name { id } => Expr::Name { id },
        Expr::Int { value } => Expr::Int { value },
        Expr::BigInt { value } => Expr::BigInt { value },
        Expr::Float { value } => Expr::Float { value },
        Expr::String { value } => Expr::String { value },
        Expr::Bool { value } => Expr::Bool { value },
        Expr::None => Expr::None,

        Expr::List { elements } => Expr::List {
            elements: elements
                .into_iter()
                .map(|element| normalize_expr(element, counter, taken))
                .collect(),
        },
        Expr::Tuple { elements } => Expr::Tuple {
            elements: elements
                .into_iter()
                .map(|element| normalize_expr(element, counter, taken))
                .collect(),
        },
        Expr::Set { elements } => Expr::Set {
            elements: elements
                .into_iter()
                .map(|element| normalize_expr(element, counter, taken))
                .collect(),
        },
        Expr::Dict { keys, values } => Expr::Dict {
            keys: keys
                .into_iter()
                .map(|key| normalize_expr(key, counter, taken))
                .collect(),
            values: values
                .into_iter()
                .map(|value| normalize_expr(value, counter, taken))
                .collect(),
        },

        Expr::Binary {
            left,
            operator,
            right,
        } => Expr::Binary {
            left: Box::new(normalize_expr(*left, counter, taken)),
            operator,
            right: Box::new(normalize_expr(*right, counter, taken)),
        },
        Expr::Unary { operator, operand } => Expr::Unary {
            operator,
            operand: Box::new(normalize_expr(*operand, counter, taken)),
        },
        Expr::Subscript { value, index } => Expr::Subscript {
            value: Box::new(normalize_expr(*value, counter, taken)),
            index: Box::new(normalize_expr(*index, counter, taken)),
        },
        Expr::Slice { lower, upper, step } => Expr::Slice {
            lower: lower.map(|bound| Box::new(normalize_expr(*bound, counter, taken))),
            upper: upper.map(|bound| Box::new(normalize_expr(*bound, counter, taken))),
            step: step.map(|bound| Box::new(normalize_expr(*bound, counter, taken))),
        },
        Expr::Compare {
            left,
            operators,
            comparators,
        } => chain_comparison(*left, operators, comparators, counter, taken),
        Expr::BoolOp { operator, values } => Expr::BoolOp {
            operator,
            values: values
                .into_iter()
                .map(|value| normalize_expr(value, counter, taken))
                .collect(),
        },
        Expr::IfExp { test, body, orelse } => Expr::IfExp {
            test: Box::new(normalize_expr(*test, counter, taken)),
            body: Box::new(normalize_expr(*body, counter, taken)),
            orelse: Box::new(normalize_expr(*orelse, counter, taken)),
        },
        Expr::Lambda { args, body } => Expr::Lambda {
            args,
            body: Box::new(normalize_expr(*body, counter, taken)),
        },
        Expr::Starred { value } => Expr::Starred {
            value: Box::new(normalize_expr(*value, counter, taken)),
        },
        Expr::ListComp {
            elt,
            target,
            iter,
            condition,
        } => Expr::ListComp {
            elt: Box::new(normalize_expr(*elt, counter, taken)),
            target,
            iter: Box::new(normalize_expr(*iter, counter, taken)),
            condition: condition.map(|value| Box::new(normalize_expr(*value, counter, taken))),
        },
        Expr::Attribute { value, attr } => Expr::Attribute {
            value: Box::new(normalize_expr(*value, counter, taken)),
            attr,
        },
        Expr::Call {
            function,
            args,
            keywords,
        } => Expr::Call {
            function: Box::new(normalize_expr(*function, counter, taken)),
            args: args
                .into_iter()
                .map(|arg| normalize_expr(arg, counter, taken))
                .collect(),
            keywords: keywords
                .into_iter()
                .map(|keyword| tarvos_ast::Keyword {
                    arg: keyword.arg,
                    value: normalize_expr(keyword.value, counter, taken),
                })
                .collect(),
        },
        Expr::MethodCall {
            object,
            method,
            args,
        } => Expr::MethodCall {
            object: Box::new(normalize_expr(*object, counter, taken)),
            method,
            args: args
                .into_iter()
                .map(|arg| normalize_expr(arg, counter, taken))
                .collect(),
        },
        Expr::FormatString { parts } => Expr::FormatString {
            parts: parts
                .into_iter()
                .map(|part| match part {
                    tarvos_ast::FormatPart::Literal { value } => {
                        tarvos_ast::FormatPart::Literal { value }
                    }
                    tarvos_ast::FormatPart::Value {
                        value,
                        format_spec,
                        conversion,
                    } => tarvos_ast::FormatPart::Value {
                        value: normalize_expr(value, counter, taken),
                        format_spec,
                        conversion,
                    },
                })
                .collect(),
        },
    }
}

/// Fold `a <= b <= c` into `(a <= b) and (b <= c)`.
///
/// Python evaluates the shared middle operand exactly once, so re-using it in
/// each pairwise comparison is only equivalent when that operand is free of side
/// effects. A chain with an effectful middle operand is left intact; the
/// lowering stage rejects it rather than silently evaluating it twice.
fn chain_comparison(
    left: Expr,
    operators: Vec<String>,
    comparators: Vec<Expr>,
    counter: &mut usize,
    taken: &mut HashSet<String>,
) -> Expr {
    let single = operators.len() < 2 || operators.len() != comparators.len();
    if single {
        return Expr::Compare {
            left: Box::new(normalize_expr(left, counter, taken)),
            operators,
            comparators: comparators
                .into_iter()
                .map(|comparator| normalize_expr(comparator, counter, taken))
                .collect(),
        };
    }

    let mut operands = vec![left];
    operands.extend(comparators);
    if !operands[1..operands.len() - 1]
        .iter()
        .all(is_repeatable_operand)
    {
        return Expr::Compare {
            left: Box::new(operands.remove(0)),
            operators,
            comparators: operands
                .into_iter()
                .map(|operand| normalize_expr(operand, counter, taken))
                .collect(),
        };
    }

    // `and` short-circuits, so a failing earlier term skips the rest of the
    // chain exactly like Python.
    let terms = operators
        .iter()
        .enumerate()
        .map(|(index, operator)| Expr::Compare {
            left: Box::new(normalize_expr(operands[index].clone(), counter, taken)),
            operators: vec![operator.clone()],
            comparators: vec![normalize_expr(operands[index + 1].clone(), counter, taken)],
        })
        .collect();
    Expr::BoolOp {
        operator: "and".to_string(),
        values: terms,
    }
}

/// True when an expression can be duplicated without changing program behavior.
fn is_repeatable_operand(expr: &Expr) -> bool {
    match expr {
        Expr::Name { .. }
        | Expr::Int { .. }
        | Expr::BigInt { .. }
        | Expr::Float { .. }
        | Expr::String { .. }
        | Expr::Bool { .. }
        | Expr::None => true,
        Expr::Unary { operand, .. } => is_repeatable_operand(operand),
        Expr::Attribute { value, .. } => is_repeatable_operand(value),
        _ => false,
    }
}

/// Rewrite `a, b[i] = x, y` into temporaries followed by single-target writes.
///
/// The shared IR only destructures plain names, but the swap idiom
/// (`chars[i], chars[j] = chars[j], chars[i]`) writes through subscripts. Python
/// evaluates every right-hand side before writing any target, so staging the
/// values in temporaries first preserves that ordering — and is what makes the
/// swap actually swap.
fn expand_parallel_assignment(
    target: Expr,
    value: Expr,
    counter: &mut usize,
    taken: &mut HashSet<String>,
) -> Vec<Stmt> {
    let unchanged = |target: Expr, value: Expr| vec![Stmt::Assign { target, value }];
    let Expr::Tuple { elements: targets } = &target else {
        return unchanged(target, value);
    };
    // Plain name destructuring is already modelled by the IR.
    if targets
        .iter()
        .all(|target| matches!(target, Expr::Name { .. }))
    {
        return unchanged(target, value);
    }
    // Unpacking an unknown iterable needs a runtime length check the IR cannot
    // express, so the original statement is left for the lowering stage.
    let Expr::Tuple { elements: values } = &value else {
        return unchanged(target, value);
    };
    if targets.len() != values.len() {
        return unchanged(target, value);
    }

    // Stage every right-hand side before any target is written.
    let mut statements = Vec::with_capacity(targets.len() * 2);
    let mut temporaries = Vec::with_capacity(values.len());
    for staged in values {
        let name = fresh_temporary(counter, taken);
        statements.push(Stmt::Assign {
            target: Expr::Name { id: name.clone() },
            value: staged.clone(),
        });
        temporaries.push(name);
    }
    for (target, temporary) in targets.iter().cloned().zip(temporaries) {
        statements.push(Stmt::Assign {
            target,
            value: Expr::Name { id: temporary },
        });
    }
    statements
}

/// A temporary name that cannot collide with a user binding in this scope.
fn fresh_temporary(counter: &mut usize, taken: &mut HashSet<String>) -> String {
    loop {
        let candidate = format!("__tarvos_tmp{counter}");
        *counter += 1;
        if !taken.contains(&candidate) {
            taken.insert(candidate.clone());
            return candidate;
        }
    }
}
