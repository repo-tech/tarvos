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
    Object(String),

    // Special
    Unknown,

    /// A value whose type is not fixed at compile time.
    ///
    /// This is what Python's dynamic typing looks like from the outside: the
    /// name `x` is bound to an `Int` on one line and a `String` on the next.
    /// Tarvos keeps the native typed path for the overwhelming majority of
    /// variables, so `Dynamic` is reserved for names that genuinely change type.
    /// Those are compiled to a tagged runtime value instead of a bare `i64`,
    /// which costs a tag word and a match on each operation, and buys the
    /// ability to do what the source said.
    ///
    /// It is deliberately *not* a fallback for "the analyser could not work this
    /// out". `Unknown` already means that, and silently treating the two the
    /// same would hide real type changes behind a convenient guess.
    Dynamic,
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
            Type::Object(name) => write!(f, "{}", name),
            Type::Unknown => write!(f, "Unknown"),
            Type::Dynamic => write!(f, "dynamic"),
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
