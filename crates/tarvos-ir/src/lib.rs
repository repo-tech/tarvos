use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use tarvos_types::Type;

/// Tarvos IR Module - contains all statements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub statements: Vec<Stmt>,
}

/// IR Statement - simplified representation suitable for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    StructDef {
        name: String,
        fields: Vec<(String, Type)>,
    },
    /// Initial variable declaration: let x = value;
    Let {
        name: String,
        ty: Type,
        value: Value,
    },
    /// Reassignment to an existing variable: x = value;
    Assign {
        name: String,
        value: Value,
    },
    /// Assign tuple elements to names in one evaluation.
    Destructure {
        targets: Vec<String>,
        value: Value,
    },
    FieldAssign {
        object: Value,
        field: String,
        value: Value,
    },
    /// Print statement (multi-argument supported)
    Print(Vec<Value>),
    /// Subscript mutation through an index chain: `target[i0][i1]... = value;`
    IndexAssign {
        target: String,
        indices: Vec<Value>,
        value: Value,
    },
    /// If statement
    If {
        test: Value,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    /// While loop
    While {
        test: Value,
        body: Vec<Stmt>,
    },
    /// For loop
    For {
        target: String,
        iter: Value,
        iter_type: Type,
        body: Vec<Stmt>,
    },
    /// Function definition
    Function {
        name: String,
        params: Vec<(String, Type)>,
        return_type: Type,
        body: Vec<Stmt>,
    },
    /// Return statement
    Return(Option<Value>),
    /// Python's `pass`: a statement that does nothing. Emitted as an empty
    /// statement so a body that is only `pass` still forms a valid Rust block.
    Pass,
    Break,
    Continue,
    Raise(Option<Value>),
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },
    With {
        items: Vec<WithItem>,
        body: Vec<Stmt>,
    },
    /// Mutating append on a statically typed list.
    ListAppend {
        target: String,
        value: Value,
    },
    /// Evaluate a call for its side effects and discard its return value.
    Expr(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptHandler {
    pub name: Option<String>,
    pub exc_type: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithItem {
    pub context_expr: Value,
    pub target: Option<String>,
}

/// IR Value - atomic expression (no side effects)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    // Constants
    Int(i64),
    /// Wide integer used for compile-time reductions that exceed i64.
    Int128(u128),
    Float(f64),
    String(String),
    Bool(bool),

    // Variable reference
    Name(String),
    Field {
        object: Box<Value>,
        field: String,
        ty: Type,
    },

    // Unary operation
    Unary {
        op: UnaryOp,
        operand: Box<Value>,
        ty: Type,
    },

    // Binary operation
    Binary {
        left: Box<Value>,
        op: BinaryOp,
        right: Box<Value>,
        ty: Type,
    },

    // Function call
    Call {
        function: String,
        args: Vec<Value>,
        return_type: Type,
    },

    // List construction
    List {
        elements: Vec<Value>,
        element_type: Type,
    },
    ListComp {
        target: String,
        iter: Box<Value>,
        element: Box<Value>,
        condition: Option<Box<Value>>,
        element_type: Type,
    },
    Tuple {
        elements: Vec<Value>,
        element_types: Vec<Type>,
    },
    Dict {
        keys: Vec<Value>,
        values: Vec<Value>,
        key_type: Type,
        value_type: Type,
    },

    // Subscript read: container[index]
    // e.g. arr[i]  â†’  arr[i as usize]  in Rust
    Index {
        container: Box<Value>,
        index: Box<Value>,
        element_type: Type,
        container_type: Type,
    },

    // Slice read: container[lower:upper:step]
    Slice {
        container: Box<Value>,
        lower: Option<Box<Value>>,
        upper: Option<Box<Value>>,
        step: Option<Box<Value>>,
        container_type: Type,
    },
    FormatString {
        parts: Vec<FormatPart>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormatPart {
    Literal(String),
    Value {
        value: Box<Value>,
        format_spec: Option<String>,
        conversion: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    /// Python `~value` (bitwise NOT) on native integers.
    Invert,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Pow,
    BitXor,
    BitAnd,
    BitOr,
    LShift,
    RShift,
    FloorDiv,
    /// Python's `x in container`, lowered to a membership test.
    ///
    /// Membership is not an arithmetic operator and has no numeric result, so it
    /// is a distinct variant rather than a reuse of `Eq`: the operand order is
    /// reversed (`7 in values`, not `values == 7`) and the right side is a
    /// container, not a value of the same type.
    In,
    /// Python's `x not in container`, the negation of [`BinaryOp::In`].
    NotIn,
}

impl BinaryOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::Pow => "**",
            BinaryOp::BitXor => "^",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::LShift => "<<",
            BinaryOp::RShift => ">>",
            BinaryOp::FloorDiv => "//",
            // Membership prints as the word Python spells it with, which keeps
            // generated Rust readable when it is inspected.
            BinaryOp::In => "in",
            BinaryOp::NotIn => "not in",
        }
    }
}

/// Type inference context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeContext {
    pub symbols: HashMap<String, Type>,
}

impl TypeContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn declare(&mut self, name: String, ty: Type) {
        self.symbols.insert(name, ty);
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.symbols.get(name)
    }
}
