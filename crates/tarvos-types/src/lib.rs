use serde::{Deserialize, Serialize};
use std::fmt::Display;
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

    /// Composite types (future)
    Array(Box<Type>),
    /// A `set`: homogeneous elements, unordered, deduplicated.
    ///
    /// It is its own type rather than a disguised `Array` so `print(s)` can
    /// render `{1, 2, 3}` instead of `[1, 2, 3]`, and so `list - list` stays a
    /// rejected operation while `set - set` is not.
    ///
    /// The runtime keeps elements **sorted**, which is what CPython's own
    /// iteration order produces for ints and floats (their hash is the value,
    /// so bucket order is ascending). A `str` set's order in CPython is
    /// randomised per process by `PYTHONHASHSEED` and cannot be reproduced by
    /// anything; sorting is the closest deterministic choice.
    Set(Box<Type>),
    Tuple(Vec<Type>),
    Dict {
        key: Box<Type>,
        value: Box<Type>,
    },
    Object(String),

    /// A union type: `x: int | str | None`.

    /// Used when a variable is assigned values of different types, e.g.
    /// `x = None; x = "hello"` infers `Union([None, String])`.
    Union(Vec<Type>),

    // Special
    Unknown,

    /// A tagged value for dynamic method dispatch.
    MethodCall(String),
    /// Dynamic typing: a name that changes type.
    Dynamic,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "str"),
            Type::None => write!(f, "None"),
            Type::Array(inner) => write!(f, "list[{}]", inner),
            Type::Set(inner) => write!(f, "set[{}]", inner),
            Type::Tuple(types) => write!(f, "({})", types.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(", ")),
            Type::Dict { key, value } => write!(f, "dict[{}, {}]", key, value),
            Type::Object(name) => write!(f, "class {}", name),
            Type::Union(types) => write!(f, "{}", types.iter().map(|t| t.to_string()).collect::<Vec<String>>().join(" | ")),
            Type::MethodCall(name) => write!(f, "method({})", name),
            Type::Unknown => write!(f, "Unknown"),
            Type::Dynamic => write!(f, "Dynamic"),
        }
    }
}
