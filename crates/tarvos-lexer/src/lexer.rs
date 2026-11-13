use crate::token::{Token, TokenKind, Literal, KEYWORDS, LexerError};

pub type LexerResult<T> = Result<T, LexerError>;

pub struct Lexer<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
    column: usize,
    position: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Lexer {
            source,
            chars: source.chars().peekable(),
            line: 1,
            column: 1,
            position: 0,
        }
    }

    pub fn lex_all(mut self) -> LexerResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let has_eof = matches!(token.kind, TokenKind::Eof);
            tokens.push(token);
            if has_eof { break; }
        }
        Ok(tokens)
    }