use serde::{Deserialize, Serialize};
use std::fmt;

/// Tarvos type system - initial subset
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    // Primitive types
    Int,
    Float,
    Bool,
    String,
    None,

    // Composite types (future)
    Array(Box<Type>),
    Tuple(Vec<Type>),
    Dict { key: Box<Type>, value: Box<Type> },

    // Special
    Unknown,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "str"),
            Type::None => write!(f, "None"),
            Type::Array(inner) => write!(f, "Array[{}]", inner),
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            Type::Dict { key, value } => write!(f, "Dict[{}, {}]", key, value),
            Type::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug)]
pub enum TypeError {
    Mismatch { expected: Type, actual: Type },
    Undefined(String),
    UnsupportedOperation { op: String, left: Type, right: Type },
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TypeError::Mismatch { expected, actual } => {
                write!(f, "type mismatch: expected {}, got {}", expected, actual)
            }
            TypeError::Undefined(name) => {
                write!(f, "undefined variable: {}", name)
            }
            TypeError::UnsupportedOperation { op, left, right } => {
                write!(f, "cannot apply '{}' to {} and {}", op, left, right)
            }
        }
    }
}

impl std::error::Error for TypeError {}
