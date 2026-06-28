// Bang 프로그래밍 언어 — 토큰 정의
//
// Lexer가 소스 코드를 스캔하여 생성하는 토큰의 종류와
// 소스 위치 정보(Span)를 정의한다.

use std::fmt;

/// 소스 코드 내 위치 (줄/열, 1-based)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// 토큰 종류
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // === 리터럴 ===
    Int(i64),
    Float(f64),
    Str(String),
    True,
    False,
    Nil,

    // === 식별자 ===
    Ident(String),

    // === 키워드 ===
    Let,
    Fn,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Spawn,
    Parallel,
    And,
    Or,
    Not,
    Break,
    Continue,
    Try,
    Catch,
    Throw,

    // === 연산자 ===
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent, // %
    EqEq,    // ==
    BangEq,  // !=
    Lt,      // <
    LtEq,    // <=
    Gt,      // >
    GtEq,    // >=
    Eq,      // =
    Arrow,   // ->

    // === 구두점 ===
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    Dot,      // .
    Colon,    // :

    // === 끝 ===
    Newline, // 자동 삽입 문장 종결자
    Eof,
}

impl TokenKind {
    /// 키워드 문자열 → TokenKind 변환. 키워드가 아니면 None.
    pub fn from_keyword(word: &str) -> Option<TokenKind> {
        match word {
            "let" => Some(TokenKind::Let),
            "fn" => Some(TokenKind::Fn),
            "if" => Some(TokenKind::If),
            "else" => Some(TokenKind::Else),
            "while" => Some(TokenKind::While),
            "for" => Some(TokenKind::For),
            "in" => Some(TokenKind::In),
            "return" => Some(TokenKind::Return),
            "spawn" => Some(TokenKind::Spawn),
            "parallel" => Some(TokenKind::Parallel),
            "and" => Some(TokenKind::And),
            "or" => Some(TokenKind::Or),
            "not" => Some(TokenKind::Not),
            "break" => Some(TokenKind::Break),
            "continue" => Some(TokenKind::Continue),
            "try" => Some(TokenKind::Try),
            "catch" => Some(TokenKind::Catch),
            "throw" => Some(TokenKind::Throw),
            "true" => Some(TokenKind::True),
            "false" => Some(TokenKind::False),
            "nil" => Some(TokenKind::Nil),
            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Int(n) => write!(f, "Int({n})"),
            TokenKind::Float(n) => write!(f, "Float({n})"),
            TokenKind::Str(s) => write!(f, "Str({s:?})"),
            TokenKind::True => write!(f, "True"),
            TokenKind::False => write!(f, "False"),
            TokenKind::Nil => write!(f, "Nil"),
            TokenKind::Ident(s) => write!(f, "Ident({s})"),
            TokenKind::Let => write!(f, "Let"),
            TokenKind::Fn => write!(f, "Fn"),
            TokenKind::If => write!(f, "If"),
            TokenKind::Else => write!(f, "Else"),
            TokenKind::While => write!(f, "While"),
            TokenKind::For => write!(f, "For"),
            TokenKind::In => write!(f, "In"),
            TokenKind::Return => write!(f, "Return"),
            TokenKind::Spawn => write!(f, "Spawn"),
            TokenKind::Parallel => write!(f, "Parallel"),
            TokenKind::And => write!(f, "And"),
            TokenKind::Or => write!(f, "Or"),
            TokenKind::Not => write!(f, "Not"),
            TokenKind::Break => write!(f, "Break"),
            TokenKind::Continue => write!(f, "Continue"),
            TokenKind::Try => write!(f, "Try"),
            TokenKind::Catch => write!(f, "Catch"),
            TokenKind::Throw => write!(f, "Throw"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::BangEq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::Eq => write!(f, "="),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Newline => write!(f, "Newline"),
            TokenKind::Eof => write!(f, "Eof"),
        }
    }
}

/// 토큰: 종류 + 소스 위치
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} @ {}", self.kind, self.span)
    }
}
