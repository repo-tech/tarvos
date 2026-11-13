#[derive(Debug, Clone)]
pub struct Module {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        value: Value,
    },
    Print(Value),
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Name(String),
    Binary {
        left: Box<Value>,
        op: BinaryOp,
        right: Box<Value>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}
