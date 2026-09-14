use serde::Deserialize;

/// Python AST representation converted from Python's ast module
#[derive(Debug, Clone, Deserialize)]
pub struct Module {
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Stmt {
    #[serde(rename = "import")]
    Import { names: Vec<ImportName> },

    #[serde(rename = "import_from")]
    ImportFrom {
        module: String,
        names: Vec<ImportName>,
    },

    #[serde(rename = "assign")]
    Assign { target: Expr, value: Expr },

    #[serde(rename = "expr")]
    Expr { value: Expr },

    #[serde(rename = "if")]
    If {
        test: Expr,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },

    #[serde(rename = "while")]
    While { test: Expr, body: Vec<Stmt> },

    #[serde(rename = "for")]
    For {
        target: Expr,
        iter: Expr,
        body: Vec<Stmt>,
    },

    #[serde(rename = "funcdef")]
    FunctionDef {
        name: String,
        args: Vec<String>,
        #[serde(default)]
        arg_annotations: Vec<Option<String>>,
        body: Vec<Stmt>,
        returns: Option<String>,
    },

    #[serde(rename = "return")]
    Return { value: Option<Expr> },

    #[serde(rename = "break")]
    Break,

    #[serde(rename = "continue")]
    Continue,

    #[serde(rename = "raise")]
    Raise { exc: Option<Expr> },

    #[serde(rename = "try")]
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        #[serde(default)]
        orelse: Vec<Stmt>,
        #[serde(default)]
        finalbody: Vec<Stmt>,
    },

    #[serde(rename = "with")]
    With {
        items: Vec<WithItem>,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExceptHandler {
    pub name: Option<String>,
    pub exc_type: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WithItem {
    pub context_expr: Expr,
    pub optional_vars: Option<Expr>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportName {
    pub name: String,
    pub asname: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Expr {
    #[serde(rename = "name")]
    Name { id: String },

    #[serde(rename = "int")]
    Int { value: i64 },

    #[serde(rename = "float")]
    Float { value: f64 },

    #[serde(rename = "string")]
    String { value: String },

    #[serde(rename = "bool")]
    Bool { value: bool },

    #[serde(rename = "none")]
    None,

    #[serde(rename = "binary")]
    Binary {
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
    },

    #[serde(rename = "unary")]
    Unary {
        operator: String,
        operand: Box<Expr>,
    },

    #[serde(rename = "compare")]
    Compare {
        left: Box<Expr>,
        operators: Vec<String>,
        comparators: Vec<Expr>,
    },

    #[serde(rename = "call")]
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
        #[serde(default)]
        keywords: Vec<Keyword>,
    },

    #[serde(rename = "method_call")]
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },

    #[serde(rename = "list")]
    List { elements: Vec<Expr> },

    #[serde(rename = "tuple")]
    Tuple { elements: Vec<Expr> },

    #[serde(rename = "dict")]
    Dict { keys: Vec<Expr>, values: Vec<Expr> },

    #[serde(rename = "subscript")]
    Subscript { value: Box<Expr>, index: Box<Expr> },

    #[serde(rename = "slice")]
    Slice {
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },

    #[serde(rename = "format_string")]
    FormatString { parts: Vec<FormatPart> },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum FormatPart {
    #[serde(rename = "literal")]
    Literal { value: String },
    #[serde(rename = "value")]
    Value { value: Expr },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Keyword {
    pub arg: Option<String>,
    pub value: Expr,
}

impl Expr {
    /// Get a debug representation suitable for error messages
    pub fn kind_name(&self) -> &'static str {
        match self {
            Expr::Name { .. } => "name",
            Expr::Int { .. } => "int",
            Expr::Float { .. } => "float",
            Expr::String { .. } => "string",
            Expr::Bool { .. } => "bool",
            Expr::None => "None",
            Expr::Binary { .. } => "binary operation",
            Expr::Unary { .. } => "unary operation",
            Expr::Compare { .. } => "comparison",
            Expr::Call { .. } => "function call",
            Expr::MethodCall { .. } => "method call",
            Expr::List { .. } => "list",
            Expr::Tuple { .. } => "tuple",
            Expr::Dict { .. } => "dictionary",
            Expr::Subscript { .. } => "subscript",
            Expr::Slice { .. } => "slice",
            Expr::FormatString { .. } => "format string",
        }
    }
}
