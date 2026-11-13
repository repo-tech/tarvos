use thiserror::Error;
use serde::{Serialize, Deserialize};

/// A single token produced by the lexer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub line: usize,
    pub column: usize,
}

/// All possible token kinds in TarvosScript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TokenKind {
    // Keywords
    Fn,
    Let,
    Const,
    Struct,
    If,
    Else,
    While,
    For,
    Return,
    Break,
    Continue,
    Import,
    From,
    As,
    True,
    False,
    Null,
    Void,
    Ref,
    Mut,

    // Literals
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),

    // Identifiers
    Identifier(String),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    Not,
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Arrow,        // ->
    FatArrow,     // =>
    DotDot,       // ..
    DotDotEq,     // ..=

    // Delimiters
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Dot,
    Semicolon,
    Colon,
    DoubleColon,  // ::

    // Comments & whitespace (usually skipped but kept for error reporting)
    Comment(String),

    // Special
    Eof,
    Error(String),
}

/// Literal values extracted from tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}

/// Lexer errors.
#[derive(Error, Debug)]
pub enum LexerError {
    #[error("Unexpected character '{0}' at line {1}, column {2}")]
    UnexpectedChar(char, usize, usize),

    #[error("Unterminated string at line {0}, column {1}")]
    UnterminatedString(usize, usize),

    #[error("Invalid number format at line {0}, column {1}")]
    InvalidNumber(usize, usize),

    #[error("Invalid escape sequence '\\{0}' at line {1}, column {2}")]
    InvalidEscape(char, usize, usize),
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: String, line: usize, column: usize) -> Self {
        Token { kind, lexeme, line, column }
    }

    pub fn is_keyword(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Fn
                | TokenKind::Let
                | TokenKind::Const
                | TokenKind::Struct
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Import
                | TokenKind::From
                | TokenKind::As
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
                | TokenKind::Void
                | TokenKind::Ref
                | TokenKind::Mut
        )
    }
}

impl TokenKind {
    /// Check if this token kind is a keyword that starts a block.
    pub fn is_block_starter(&self) -> bool {
        matches!(self, TokenKind::Fn | TokenKind::Struct | TokenKind::If | TokenKind::While | TokenKind::For)
    }
}

/// Map of reserved keywords to their token kinds.
pub const KEYWORDS: &[(&str, TokenKind)] = &[
    ("fn", TokenKind::Fn),
    ("let", TokenKind::Let),
    ("const", TokenKind::Const),
    ("struct", TokenKind::Struct),
    ("if", TokenKind::If),
    ("else", TokenKind::Else),
    ("while", TokenKind::While),
    ("for", TokenKind::For),
    ("return", TokenKind::Return),
    ("break", TokenKind::Break),
    ("continue", TokenKind::Continue),
    ("import", TokenKind::Import),
    ("from", TokenKind::From),
    ("as", TokenKind::As),
    ("true", TokenKind::True),
    ("false", TokenKind::False),
    ("null", TokenKind::Null),
    ("void", TokenKind::Void),
    ("ref", TokenKind::Ref),
    ("mut", TokenKind::Mut),
];