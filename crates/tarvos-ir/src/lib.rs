use std::collections::HashMap;
use tarvos_types::Type;

/// Tarvos IR Module - contains all statements
#[derive(Debug, Clone)]
pub struct Module {
    pub statements: Vec<Stmt>,
}

/// IR Statement - simplified representation suitable for optimization
#[derive(Debug, Clone)]
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
    FieldAssign {
        object: Value,
        field: String,
        value: Value,
    },
    /// Print statement (multi-argument supported)
    Print(Vec<Value>),
    /// Subscript mutation: target[index] = value;
    IndexAssign {
        target: String,
        index: Value,
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

#[derive(Debug, Clone)]
pub struct ExceptHandler {
    pub name: Option<String>,
    pub exc_type: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct WithItem {
    pub context_expr: Value,
    pub target: Option<String>,
}

/// IR Value - atomic expression (no side effects)
#[derive(Debug, Clone)]
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
    // e.g. arr[i]  →  arr[i as usize]  in Rust
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

#[derive(Debug, Clone)]
pub enum FormatPart {
    Literal(String),
    Value {
        value: Box<Value>,
        format_spec: Option<String>,
        conversion: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
        }
    }
}

/// Type inference context
#[derive(Debug, Clone, Default)]
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
