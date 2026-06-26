// Bang 프로그래밍 언어 — 렉서(Lexer)
//
// 소스 코드 문자열을 Token 스트림으로 변환한다.
// 줄바꿈을 Go-style로 자동 삽입: 종결 가능한 토큰 뒤의 \n →
// Newline 토큰 하나로 병합. `(` `[` 안에서는 억제.

pub mod token;

use token::{Span, Token, TokenKind};

use std::fmt;

// =============================================================================
// 에러
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum LexErrorKind {
    UnexpectedChar(char),
    UnterminatedString,
    InvalidEscape(char),
    InvalidNumber(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match &self.kind {
            LexErrorKind::UnexpectedChar(c) => format!("예상하지 못한 문자: '{c}'"),
            LexErrorKind::UnterminatedString => "종료되지 않은 문자열".to_string(),
            LexErrorKind::InvalidEscape(c) => format!("잘못된 이스케이프 시퀀스: '\\{c}'"),
            LexErrorKind::InvalidNumber(s) => format!("잘못된 숫자 리터럴: '{s}'"),
        };
        write!(f, "[{}:{}] 오류: {}", self.line, self.col, msg)
    }
}

impl std::error::Error for LexError {}

// =============================================================================
// Lexer
// =============================================================================

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    // `(` 와 `[` 깊이만 추적한다. `{` 는 블록 문장 구분을 위해 억제하지 않음.
    paren_depth: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            paren_depth: 0,
        }
    }

    /// 소스 코드를 토큰화하여 반환.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, Vec<LexError>> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut errors: Vec<LexError> = Vec::new();
        // 마지막으로 방출한 토큰 종류 (Newline 판단용)
        let mut last_kind: Option<TokenKind> = None;

        loop {
            self.skip_blanks_and_line_comments();

            // 줄바꿈 처리
            if !self.is_at_end() && self.peek() == '\n' {
                // 연속된 \n + 빈 줄 + // 주석 줄을 모두 소비
                while !self.is_at_end() && self.peek() == '\n' {
                    self.advance();
                    self.skip_blanks_and_line_comments();
                }
                // 종결 가능한 토큰 뒤이고 괄호 밖이면 Newline 방출
                if self.paren_depth == 0 {
                    if let Some(ref k) = last_kind {
                        if Self::is_newline_terminable(k) {
                            let span = self.span();
                            tokens.push(Token::new(TokenKind::Newline, span));
                            last_kind = Some(TokenKind::Newline);
                        }
                    }
                }
                continue;
            }

            if self.is_at_end() {
                // 파일 끝에서도 Newline 자동 삽입
                if self.paren_depth == 0 {
                    if let Some(ref k) = last_kind {
                        if Self::is_newline_terminable(k) {
                            tokens.push(Token::new(TokenKind::Newline, self.span()));
                        }
                    }
                }
                tokens.push(Token::new(TokenKind::Eof, self.span()));
                break;
            }

            let result = match self.peek() {
                '0'..='9' => self.scan_number(),
                '"' => self.scan_string(),
                'a'..='z' | 'A'..='Z' | '_' => Ok(self.scan_identifier()),
                _ => self.scan_punct(),
            };

            match result {
                Ok(tok) => {
                    // 괄호 깊이 추적 (`(` 과 `[` 만)
                    match tok.kind {
                        TokenKind::LParen | TokenKind::LBracket => self.paren_depth += 1,
                        TokenKind::RParen | TokenKind::RBracket => {
                            self.paren_depth = self.paren_depth.saturating_sub(1);
                        }
                        _ => {}
                    }
                    last_kind = Some(tok.kind.clone());
                    tokens.push(tok);
                }
                Err(e) => {
                    errors.push(e);
                    // 에러 복구: 한 글자 전진
                    if !self.is_at_end() {
                        self.advance();
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(tokens)
        } else {
            Err(errors)
        }
    }

    // =========================================================================
    // 헬퍼
    // =========================================================================

    fn span(&self) -> Span {
        Span::new(self.line, self.col)
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn peek(&self) -> char {
        if self.is_at_end() { '\0' } else { self.source[self.pos] }
    }

    fn peek_next(&self) -> char {
        if self.pos + 1 >= self.source.len() { '\0' } else { self.source[self.pos + 1] }
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        ch
    }

    fn error(&self, kind: LexErrorKind, line: usize, col: usize) -> LexError {
        LexError { kind, line, col }
    }

    /// 직전 토큰이 이 종류이면 뒤따르는 \n → Newline 토큰으로 방출한다.
    fn is_newline_terminable(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::Str(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Nil
                | TokenKind::Ident(_)
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::RBrace
        )
    }

    // =========================================================================
    // 공백 · 주석 건너뛰기 (줄바꿈은 건너뛰지 않음)
    // =========================================================================

    fn skip_blanks_and_line_comments(&mut self) {
        loop {
            // 공백(스페이스·탭·캐리지리턴)만 건너뜀
            while !self.is_at_end() && matches!(self.peek(), ' ' | '\t' | '\r') {
                self.advance();
            }
            // `//` 주석: 줄 끝(\n)까지 건너뜀 (\n 자체는 소비하지 않음)
            if self.peek() == '/' && self.peek_next() == '/' {
                while !self.is_at_end() && self.peek() != '\n' {
                    self.advance();
                }
                continue;
            }
            break;
        }
    }

    // =========================================================================
    // 숫자 리터럴
    // =========================================================================

    fn scan_number(&mut self) -> Result<Token, LexError> {
        let start = self.span();
        let mut num_str = String::new();
        let mut is_float = false;

        while !self.is_at_end() && self.peek().is_ascii_digit() {
            num_str.push(self.advance());
        }

        if !self.is_at_end() && self.peek() == '.' && self.peek_next().is_ascii_digit() {
            is_float = true;
            num_str.push(self.advance()); // '.'
            while !self.is_at_end() && self.peek().is_ascii_digit() {
                num_str.push(self.advance());
            }
        }

        if is_float {
            num_str
                .parse::<f64>()
                .map(|n| Token::new(TokenKind::Float(n), start))
                .map_err(|_| self.error(LexErrorKind::InvalidNumber(num_str), start.line, start.col))
        } else {
            num_str
                .parse::<i64>()
                .map(|n| Token::new(TokenKind::Int(n), start))
                .map_err(|_| self.error(LexErrorKind::InvalidNumber(num_str), start.line, start.col))
        }
    }

    // =========================================================================
    // 문자열 리터럴
    // =========================================================================

    fn scan_string(&mut self) -> Result<Token, LexError> {
        let start = self.span();
        self.advance(); // '"'

        let mut value = String::new();

        loop {
            if self.is_at_end() || self.peek() == '\n' {
                return Err(self.error(LexErrorKind::UnterminatedString, start.line, start.col));
            }

            let ch = self.advance();
            match ch {
                '"' => return Ok(Token::new(TokenKind::Str(value), start)),
                '\\' => {
                    if self.is_at_end() {
                        return Err(self.error(LexErrorKind::UnterminatedString, start.line, start.col));
                    }
                    let esc_span = self.span();
                    let esc = self.advance();
                    match esc {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        '\\' => value.push('\\'),
                        '"' => value.push('"'),
                        _ => {
                            return Err(self.error(
                                LexErrorKind::InvalidEscape(esc),
                                esc_span.line,
                                esc_span.col,
                            ));
                        }
                    }
                }
                _ => value.push(ch),
            }
        }
    }

    // =========================================================================
    // 식별자 / 키워드
    // =========================================================================

    fn scan_identifier(&mut self) -> Token {
        let start = self.span();
        let mut name = String::new();

        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            name.push(self.advance());
        }

        let kind = TokenKind::from_keyword(&name).unwrap_or(TokenKind::Ident(name));
        Token::new(kind, start)
    }

    // =========================================================================
    // 연산자 / 구두점
    // =========================================================================

    fn scan_punct(&mut self) -> Result<Token, LexError> {
        let start = self.span();
        let ch = self.advance();

        let kind = match ch {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '=' => {
                if self.peek() == '=' {
                    self.advance();
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.peek() == '=' {
                    self.advance();
                    TokenKind::BangEq
                } else {
                    return Err(self.error(LexErrorKind::UnexpectedChar(ch), start.line, start.col));
                }
            }
            '<' => {
                if self.peek() == '=' {
                    self.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == '=' {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            ':' => TokenKind::Colon,
            _ => {
                return Err(self.error(LexErrorKind::UnexpectedChar(ch), start.line, start.col));
            }
        };

        Ok(Token::new(kind, start))
    }
}

// =============================================================================
// 단위 테스트
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 토큰 종류만 추출 (Eof + Newline 제외) — 기존 테스트 호환용
    fn kinds_no_eof(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        lexer.tokenize()
            .expect("렉싱 실패")
            .into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::Eof | TokenKind::Newline))
            .map(|t| t.kind)
            .collect()
    }

    /// 토큰 종류만 추출 (Eof 제외, Newline 포함) — Newline 방출 테스트용
    fn kinds_with_newlines(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        lexer.tokenize()
            .expect("렉싱 실패")
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| t.kind)
            .collect()
    }

    /// 토큰 종류 추출 (Eof 포함)
    fn kinds(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        lexer.tokenize()
            .expect("렉싱 실패")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    // =================================================================
    // 리터럴
    // =================================================================

    #[test]
    fn test_integer_literals() {
        assert_eq!(
            kinds_no_eof("42 0 1234567890"),
            vec![TokenKind::Int(42), TokenKind::Int(0), TokenKind::Int(1234567890)]
        );
    }

    #[test]
    fn test_float_literals() {
        assert_eq!(
            kinds_no_eof("1.23 0.5 100.0"),
            vec![TokenKind::Float(1.23), TokenKind::Float(0.5), TokenKind::Float(100.0)]
        );
    }

    #[test]
    fn test_float_vs_dot_method() {
        assert_eq!(
            kinds_no_eof("42.foo"),
            vec![TokenKind::Int(42), TokenKind::Dot, TokenKind::Ident("foo".into())]
        );
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(
            kinds_no_eof(r#""hello world""#),
            vec![TokenKind::Str("hello world".into())]
        );
    }

    #[test]
    fn test_string_escapes() {
        assert_eq!(
            kinds_no_eof(r#""a\nb\tc\\d\"e""#),
            vec![TokenKind::Str("a\nb\tc\\d\"e".into())]
        );
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(kinds_no_eof(r#""""#), vec![TokenKind::Str(String::new())]);
    }

    #[test]
    fn test_boolean_nil() {
        assert_eq!(
            kinds_no_eof("true false nil"),
            vec![TokenKind::True, TokenKind::False, TokenKind::Nil]
        );
    }

    // =================================================================
    // 식별자
    // =================================================================

    #[test]
    fn test_identifiers() {
        assert_eq!(
            kinds_no_eof("foo bar_baz _x X1 camelCase"),
            vec![
                TokenKind::Ident("foo".into()),
                TokenKind::Ident("bar_baz".into()),
                TokenKind::Ident("_x".into()),
                TokenKind::Ident("X1".into()),
                TokenKind::Ident("camelCase".into()),
            ]
        );
    }

    // =================================================================
    // 키워드
    // =================================================================

    #[test]
    fn test_keywords() {
        assert_eq!(
            kinds_no_eof("let fn if else while for in return"),
            vec![
                TokenKind::Let, TokenKind::Fn, TokenKind::If, TokenKind::Else,
                TokenKind::While, TokenKind::For, TokenKind::In, TokenKind::Return,
            ]
        );
    }

    #[test]
    fn test_spawn_parallel_are_keywords() {
        assert_eq!(
            kinds_no_eof("spawn parallel"),
            vec![TokenKind::Spawn, TokenKind::Parallel]
        );
    }

    #[test]
    fn test_keyword_prefix_is_ident() {
        assert_eq!(
            kinds_no_eof("letter iffy"),
            vec![TokenKind::Ident("letter".into()), TokenKind::Ident("iffy".into())]
        );
    }

    // =================================================================
    // 연산자
    // =================================================================

    #[test]
    fn test_arithmetic_operators() {
        assert_eq!(
            kinds_no_eof("+ - * / %"),
            vec![TokenKind::Plus, TokenKind::Minus, TokenKind::Star, TokenKind::Slash, TokenKind::Percent]
        );
    }

    #[test]
    fn test_eq_vs_eqeq() {
        assert_eq!(
            kinds_no_eof("= == ="),
            vec![TokenKind::Eq, TokenKind::EqEq, TokenKind::Eq]
        );
    }

    #[test]
    fn test_bang_eq() {
        assert_eq!(kinds_no_eof("!="), vec![TokenKind::BangEq]);
    }

    #[test]
    fn test_lt_vs_lteq() {
        assert_eq!(
            kinds_no_eof("< <= <"),
            vec![TokenKind::Lt, TokenKind::LtEq, TokenKind::Lt]
        );
    }

    #[test]
    fn test_gt_vs_gteq() {
        assert_eq!(
            kinds_no_eof("> >= >"),
            vec![TokenKind::Gt, TokenKind::GtEq, TokenKind::Gt]
        );
    }

    #[test]
    fn test_comparison_all() {
        assert_eq!(
            kinds_no_eof("== != < <= > >="),
            vec![
                TokenKind::EqEq, TokenKind::BangEq,
                TokenKind::Lt, TokenKind::LtEq,
                TokenKind::Gt, TokenKind::GtEq,
            ]
        );
    }

    #[test]
    fn test_operators_no_spaces() {
        assert_eq!(
            kinds_no_eof("1+2*3"),
            vec![TokenKind::Int(1), TokenKind::Plus, TokenKind::Int(2), TokenKind::Star, TokenKind::Int(3)]
        );
    }

    // =================================================================
    // 구두점
    // =================================================================

    #[test]
    fn test_punctuation() {
        assert_eq!(
            kinds_no_eof("( ) { } [ ] , . :"),
            vec![
                TokenKind::LParen, TokenKind::RParen,
                TokenKind::LBrace, TokenKind::RBrace,
                TokenKind::LBracket, TokenKind::RBracket,
                TokenKind::Comma, TokenKind::Dot, TokenKind::Colon,
            ]
        );
    }

    // =================================================================
    // 주석
    // =================================================================

    #[test]
    fn test_line_comment_skipped() {
        // 주석 뒤 \n 은 x (Ident, terminable) 이후 Newline 토큰을 방출.
        // y 뒤 EOF 에서도 Newline 방출 (terminable).
        assert_eq!(
            kinds_with_newlines("x // 이것은 주석\ny"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Newline,
                TokenKind::Ident("y".into()),
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn test_comment_at_start() {
        // 주석만 있는 첫 줄: 종결 가능 토큰 없으므로 Newline 방출 안 됨
        assert_eq!(
            kinds_no_eof("// 전체 주석 줄\n42"),
            vec![TokenKind::Int(42)]
        );
    }

    #[test]
    fn test_multiple_comments() {
        assert_eq!(
            kinds_no_eof("// a\n// b\n// c\nx"),
            vec![TokenKind::Ident("x".into())]
        );
    }

    #[test]
    fn test_slash_is_not_comment() {
        assert_eq!(
            kinds_no_eof("10 / 2"),
            vec![TokenKind::Int(10), TokenKind::Slash, TokenKind::Int(2)]
        );
    }

    // =================================================================
    // 공백 · Eof
    // =================================================================

    #[test]
    fn test_empty_source() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn test_only_whitespace() {
        assert_eq!(kinds("   \n\t\n  "), vec![TokenKind::Eof]);
    }

    #[test]
    fn test_multiline() {
        assert_eq!(
            kinds_no_eof("let x = 1\nlet y = 2"),
            vec![
                TokenKind::Let, TokenKind::Ident("x".into()), TokenKind::Eq, TokenKind::Int(1),
                TokenKind::Let, TokenKind::Ident("y".into()), TokenKind::Eq, TokenKind::Int(2),
            ]
        );
    }

    // =================================================================
    // Newline 방출 규칙
    // =================================================================

    #[test]
    fn test_newline_after_terminable() {
        // Int, Ident, RParen, RBracket, RBrace 뒤 \n → Newline
        assert_eq!(
            kinds_with_newlines("42\nx\n)\n]\n}"),
            vec![
                TokenKind::Int(42), TokenKind::Newline,
                TokenKind::Ident("x".into()), TokenKind::Newline,
                TokenKind::RParen, TokenKind::Newline,
                TokenKind::RBracket, TokenKind::Newline,
                TokenKind::RBrace, TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn test_newline_suppressed_after_operator() {
        // 이항 연산자 뒤 \n → 중간 Newline 방출 안 됨 (EOF 후행 Newline은 방출)
        // "1 +\n2" == "1 + 2" — 동일한 토큰 스트림
        assert_eq!(
            kinds_with_newlines("1 +\n2"),
            vec![
                TokenKind::Int(1),
                TokenKind::Plus,
                TokenKind::Int(2),
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn test_newline_suppressed_after_comma() {
        // 콤마 뒤 \n → 중간 Newline 방출 안 됨 (EOF 후행 Newline은 방출)
        assert_eq!(
            kinds_with_newlines("a,\nb"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Comma,
                TokenKind::Ident("b".into()),
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn test_newline_suppressed_inside_parens() {
        // ( 안에서는 \n 억제
        assert_eq!(
            kinds_with_newlines("f(\na,\nb\n)"),
            vec![
                TokenKind::Ident("f".into()),
                TokenKind::LParen,
                TokenKind::Ident("a".into()),
                TokenKind::Comma,
                TokenKind::Ident("b".into()),
                TokenKind::RParen,
                TokenKind::Newline, // ) 뒤 \n → Newline (이제 depth=0)
            ]
        );
    }

    #[test]
    fn test_newline_suppressed_inside_brackets() {
        // [ 안에서는 \n 억제
        assert_eq!(
            kinds_with_newlines("[1,\n2\n]"),
            vec![
                TokenKind::LBracket,
                TokenKind::Int(1), TokenKind::Comma,
                TokenKind::Int(2),
                TokenKind::RBracket,
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn test_newline_not_suppressed_inside_braces() {
        // { 안에서는 \n 억제하지 않음 (블록 문장 구분 필요)
        assert_eq!(
            kinds_with_newlines("{\nx\n}"),
            vec![
                TokenKind::LBrace,
                // LBrace 뒤 \n: LBrace 는 종결 불가 → Newline 없음
                TokenKind::Ident("x".into()),
                TokenKind::Newline, // x 뒤 \n → Newline
                TokenKind::RBrace,
                TokenKind::Newline, // } 뒤 \n → Newline
            ]
        );
    }

    #[test]
    fn test_consecutive_newlines_merged() {
        // 연속 줄바꿈 → 하나의 Newline. y 뒤 EOF 에서도 Newline.
        assert_eq!(
            kinds_with_newlines("x\n\n\ny"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Newline,
                TokenKind::Ident("y".into()),
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn test_newline_after_return_break_continue() {
        // continue 뒤 EOF 에서도 Newline 방출 (terminable).
        assert_eq!(
            kinds_with_newlines("return\nbreak\ncontinue"),
            vec![
                TokenKind::Return, TokenKind::Newline,
                TokenKind::Break, TokenKind::Newline,
                TokenKind::Continue, TokenKind::Newline,
            ]
        );
    }

    // =================================================================
    // Span 정확성
    // =================================================================

    #[test]
    fn test_span_accuracy() {
        let mut lexer = Lexer::new("let x = 42");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].span, Span::new(1, 1)); // let @ 1:1
        assert_eq!(tokens[1].span, Span::new(1, 5)); // x   @ 1:5
        assert_eq!(tokens[2].span, Span::new(1, 7)); // =   @ 1:7
        assert_eq!(tokens[3].span, Span::new(1, 9)); // 42  @ 1:9
    }

    #[test]
    fn test_span_multiline() {
        let mut lexer = Lexer::new("x\ny");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].span, Span::new(1, 1)); // x       @ 1:1
        assert_eq!(tokens[1].kind, TokenKind::Newline); // Newline
        assert_eq!(tokens[2].span, Span::new(2, 1)); // y       @ 2:1
    }

    // =================================================================
    // 에러 케이스
    // =================================================================

    #[test]
    fn test_unterminated_string() {
        let mut lexer = Lexer::new(r#""hello"#);
        let err = lexer.tokenize().unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].kind, LexErrorKind::UnterminatedString);
        assert_eq!(err[0].line, 1);
        assert_eq!(err[0].col, 1);
    }

    #[test]
    fn test_unterminated_string_newline() {
        let mut lexer = Lexer::new("\"hello\nworld\"");
        let err = lexer.tokenize().unwrap_err();
        assert!(matches!(err[0].kind, LexErrorKind::UnterminatedString));
    }

    #[test]
    fn test_invalid_escape() {
        let mut lexer = Lexer::new(r#""\q""#);
        let err = lexer.tokenize().unwrap_err();
        assert_eq!(err[0].kind, LexErrorKind::InvalidEscape('q'));
    }

    #[test]
    fn test_unexpected_char() {
        let mut lexer = Lexer::new("x ~ y");
        let err = lexer.tokenize().unwrap_err();
        assert_eq!(err[0].kind, LexErrorKind::UnexpectedChar('~'));
        assert_eq!(err[0].line, 1);
        assert_eq!(err[0].col, 3);
    }

    #[test]
    fn test_bang_alone_is_error() {
        let mut lexer = Lexer::new("! x");
        let err = lexer.tokenize().unwrap_err();
        assert_eq!(err[0].kind, LexErrorKind::UnexpectedChar('!'));
    }

    // =================================================================
    // 복합 예제
    // =================================================================

    #[test]
    fn test_function_definition() {
        assert_eq!(
            kinds_no_eof("fn add(a, b) { return a + b }"),
            vec![
                TokenKind::Fn, TokenKind::Ident("add".into()),
                TokenKind::LParen, TokenKind::Ident("a".into()), TokenKind::Comma,
                TokenKind::Ident("b".into()), TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::Return, TokenKind::Ident("a".into()),
                TokenKind::Plus, TokenKind::Ident("b".into()),
                TokenKind::RBrace,
            ]
        );
    }

    #[test]
    fn test_if_else() {
        assert_eq!(
            kinds_no_eof("if x > 0 { y } else { z }"),
            vec![
                TokenKind::If, TokenKind::Ident("x".into()), TokenKind::Gt, TokenKind::Int(0),
                TokenKind::LBrace, TokenKind::Ident("y".into()), TokenKind::RBrace,
                TokenKind::Else,
                TokenKind::LBrace, TokenKind::Ident("z".into()), TokenKind::RBrace,
            ]
        );
    }

    #[test]
    fn test_spawn_expression() {
        assert_eq!(
            kinds_no_eof("let result = spawn fetch(url)"),
            vec![
                TokenKind::Let, TokenKind::Ident("result".into()), TokenKind::Eq,
                TokenKind::Spawn, TokenKind::Ident("fetch".into()),
                TokenKind::LParen, TokenKind::Ident("url".into()), TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn test_parallel_block() {
        assert_eq!(
            kinds_no_eof("parallel { spawn work() }"),
            vec![
                TokenKind::Parallel, TokenKind::LBrace,
                TokenKind::Spawn, TokenKind::Ident("work".into()),
                TokenKind::LParen, TokenKind::RParen,
                TokenKind::RBrace,
            ]
        );
    }

    // =================================================================
    // 논리 · 제어 키워드
    // =================================================================

    #[test]
    fn test_logical_keywords() {
        assert_eq!(
            kinds_no_eof("and or not"),
            vec![TokenKind::And, TokenKind::Or, TokenKind::Not]
        );
    }

    #[test]
    fn test_break_continue_keywords() {
        assert_eq!(
            kinds_no_eof("break continue"),
            vec![TokenKind::Break, TokenKind::Continue]
        );
    }

    #[test]
    fn test_logical_in_expression() {
        assert_eq!(
            kinds_no_eof("not x and y or z"),
            vec![
                TokenKind::Not, TokenKind::Ident("x".into()),
                TokenKind::And, TokenKind::Ident("y".into()),
                TokenKind::Or, TokenKind::Ident("z".into()),
            ]
        );
    }

    // =================================================================
    // 스냅샷: fibonacci.bang 전체 토큰화
    // =================================================================

    #[test]
    fn test_fibonacci_snapshot() {
        let source = include_str!("../../examples/fibonacci.bang");
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("fibonacci.bang 토큰화 실패");

        let fn_count = tokens.iter().filter(|t| t.kind == TokenKind::Fn).count();
        assert!(fn_count >= 1);

        let lbrace = tokens.iter().filter(|t| t.kind == TokenKind::LBrace).count();
        let rbrace = tokens.iter().filter(|t| t.kind == TokenKind::RBrace).count();
        assert_eq!(lbrace, rbrace);

        assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
    }
}
