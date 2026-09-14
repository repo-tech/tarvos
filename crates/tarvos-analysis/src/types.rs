use anyhow::{anyhow, Result};
use tarvos_ir::TypeContext;
use tarvos_types::Type;

#[derive(Default)]
pub struct TypeInference {
    context: TypeContext,
}

impl TypeInference {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn context(&self) -> &TypeContext {
        &self.context
    }

    pub fn declare(&mut self, name: String, ty: Type) {
        self.context.declare(name, ty);
    }

    pub fn infer_expr(&self, expr: &tarvos_ast::Expr) -> Result<Type> {
        match expr {
            tarvos_ast::Expr::Int { .. } => Ok(Type::Int),
            tarvos_ast::Expr::Float { .. } => Ok(Type::Float),
            tarvos_ast::Expr::String { .. } => Ok(Type::String),
            tarvos_ast::Expr::Bool { .. } => Ok(Type::Bool),
            tarvos_ast::Expr::None => Ok(Type::None),
            tarvos_ast::Expr::Name { id } => self
                .context
                .lookup(id)
                .cloned()
                .ok_or_else(|| anyhow!("undefined variable: {}", id)),
            tarvos_ast::Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left_type = self.infer_expr(left)?;
                let right_type = self.infer_expr(right)?;

                self.infer_binary_op(&left_type, operator, &right_type)
            }
            tarvos_ast::Expr::Compare { .. } => Ok(Type::Bool),
            tarvos_ast::Expr::Call { function, .. } => {
                // For now, assume built-in functions have known return types
                if let tarvos_ast::Expr::Name { id } = function.as_ref() {
                    match id.as_str() {
                        "print" => Ok(Type::None),
                        "len" => Ok(Type::Int),
                        "str" => Ok(Type::String),
                        "int" => Ok(Type::Int),
                        "float" => Ok(Type::Float),
                        "bool" => Ok(Type::Bool),
                        "range" => Ok(Type::Array(Box::new(Type::Int))),
                        "__import__" => Ok(Type::String),
                        "tarvos_perf_counter" => Ok(Type::Float),
                        _ => Err(anyhow!("unknown function: {}", id)),
                    }
                } else {
                    Err(anyhow!("cannot infer type of complex function call"))
                }
            }
            tarvos_ast::Expr::List { .. } => Ok(Type::Array(Box::new(Type::Unknown))),
            tarvos_ast::Expr::Tuple { elements } => {
                let types = elements
                    .iter()
                    .map(|element| self.infer_expr(element))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Type::Tuple(types))
            }
            tarvos_ast::Expr::Dict { keys, values } => {
                let key_type = keys
                    .first()
                    .map(|key| self.infer_expr(key))
                    .transpose()?
                    .unwrap_or(Type::Unknown);
                let value_type = values
                    .first()
                    .map(|value| self.infer_expr(value))
                    .transpose()?
                    .unwrap_or(Type::Unknown);
                Ok(Type::Dict {
                    key: Box::new(key_type),
                    value: Box::new(value_type),
                })
            }
            tarvos_ast::Expr::Subscript { .. } => Ok(Type::Unknown),
            tarvos_ast::Expr::MethodCall { .. } => Ok(Type::None),
            tarvos_ast::Expr::Unary { operand, operator } => {
                let operand_type = self.infer_expr(operand)?;
                match operator.as_str() {
                    "usub" | "uadd" if matches!(operand_type, Type::Int | Type::Float) => {
                        Ok(operand_type)
                    }
                    "not" => Ok(Type::Bool),
                    _ => Err(anyhow!("unsupported unary operator: {}", operator)),
                }
            }
            tarvos_ast::Expr::Slice { .. } => Ok(Type::Unknown),
            tarvos_ast::Expr::FormatString { .. } => Ok(Type::String),
        }
    }

    fn infer_binary_op(&self, left: &Type, op: &str, right: &Type) -> Result<Type> {
        match (left, right) {
            (Type::Int, Type::Int) => match op {
                "add" | "sub" | "mul" | "div" | "mod" => Ok(Type::Int),
                "eq" | "ne" | "lt" | "le" | "gt" | "ge" => Ok(Type::Bool),
                _ => Err(anyhow!("unknown operator: {}", op)),
            },
            (Type::Float, Type::Float) => match op {
                "add" | "sub" | "mul" | "div" => Ok(Type::Float),
                "eq" | "ne" | "lt" | "le" | "gt" | "ge" => Ok(Type::Bool),
                _ => Err(anyhow!("unknown operator: {}", op)),
            },
            (Type::Int, Type::Float) | (Type::Float, Type::Int) => match op {
                "add" | "sub" | "mul" | "div" => Ok(Type::Float),
                "eq" | "ne" | "lt" | "le" | "gt" | "ge" => Ok(Type::Bool),
                _ => Err(anyhow!("unknown operator: {}", op)),
            },
            (Type::String, Type::String) => match op {
                "add" => Ok(Type::String),
                "eq" | "ne" | "lt" | "le" | "gt" | "ge" => Ok(Type::Bool),
                _ => Err(anyhow!("unknown operator: {}", op)),
            },
            (Type::Bool, Type::Bool) => match op {
                "and" | "or" => Ok(Type::Bool),
                "eq" | "ne" => Ok(Type::Bool),
                _ => Err(anyhow!("unknown operator: {}", op)),
            },
            _ => Err(anyhow!(
                "type mismatch in binary operation: {} {} {}",
                left,
                op,
                right
            )),
        }
    }
}

pub fn infer_expr_type(context: &TypeInference, expr: &tarvos_ast::Expr) -> Result<Type> {
    context.infer_expr(expr)
}
