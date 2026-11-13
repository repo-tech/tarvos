use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Module {
    pub body: Vec<Stmt>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Stmt {
    #[serde(rename = "assign")]
    Assign {
        target: Expr,
        value: Expr,
    },

    #[serde(rename = "expr")]
    Expr {
        value: Expr,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Expr {
    #[serde(rename = "name")]
    Name {
        id: String,
    },

    #[serde(rename = "int")]
    Int {
        value: i64,
    },

    #[serde(rename = "float")]
    Float {
        value: f64,
    },

    #[serde(rename = "string")]
    String {
        value: String,
    },

    #[serde(rename = "bool")]
    Bool {
        value: bool,
    },

    #[serde(rename = "none")]
    None,

    #[serde(rename = "binary")]
    Binary {
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
    },

    #[serde(rename = "call")]
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
}