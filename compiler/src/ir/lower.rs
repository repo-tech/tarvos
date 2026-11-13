use anyhow::{bail, Result};

use crate::ast::nodes as ast;

use super::nodes::*;

pub fn lower_module(module: &ast::Module) -> Result<Module> {
    let mut statements = Vec::new();

    for stmt in &module.body {
        match stmt {
            ast::Stmt::Assign { target, value } => {
                let ast::Expr::Name { id } = target else {
                    bail!("assignment target must be a variable name");
                };

                statements.push(Stmt::Let {
                    name: id.clone(),
                    value: lower_expr(value)?,
                });
            }

            ast::Stmt::Expr { value } => {
                match value {
                    ast::Expr::Call { function, args } => {
                        if let ast::Expr::Name { id } = function.as_ref() {
                            if id == "print" && args.len() == 1 {
                                statements.push(
                                    Stmt::Print(lower_expr(&args[0])?)
                                );

                                continue;
                            }
                        }

                        bail!("unsupported function call");
                    }

                    _ => {
                        bail!("unsupported expression statement");
                    }
                }
            }
        }
    }

    Ok(Module { statements })
}

fn lower_expr(expr: &ast::Expr) -> Result<Value> {
    match expr {
        ast::Expr::Int { value } => {
            Ok(Value::Int(*value))
        }

        ast::Expr::Float { value } => {
            Ok(Value::Float(*value))
        }

        ast::Expr::String { value } => {
            Ok(Value::Str(value.clone()))
        }

        ast::Expr::Bool { value } => {
            Ok(Value::Bool(*value))
        }

        ast::Expr::Name { id } => {
            Ok(Value::Name(id.clone()))
        }

        ast::Expr::Binary {
            left,
            operator,
            right,
        } => {
            let op = match operator.as_str() {
                "add" => BinaryOp::Add,
                "sub" => BinaryOp::Sub,
                "mul" => BinaryOp::Mul,
                "div" => BinaryOp::Div,
                _ => bail!("unsupported binary operator: {operator}"),
            };

            Ok(Value::Binary {
                left: Box::new(lower_expr(left)?),
                op,
                right: Box::new(lower_expr(right)?),
            })
        }

        _ => {
            bail!("unsupported expression")
        }
    }
}