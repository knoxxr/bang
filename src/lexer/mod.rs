// Bang 프로그래밍 언어 — 렉서(Lexer)
//
// 소스 코드 문자열을 Token 스트림으로 변환한다.
// 줄바꿈을 Go-style로 자동 삽입: 종결 가능한 토큰 뒤의 \n →
// Newline 토큰 하나로 병합. `(` `[` 안에서는 억제.

pub mod token;

use token::{FStrPart, Span, Token, TokenKind};

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
    UnterminatedComment,
    /// f-문자열 안의 `{ }` 짝이 맞지 않거나 식이 비었음
    BadFString(String),
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
            LexErrorKind::UnterminatedComment => "종료되지 않은 블록 주석 ('*/' 없음)".to_string(),
            LexErrorKind::BadFString(m) => format!("f-문자열 오류: {m}"),
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
    // 열린 괄호 스택 ('(' '[' '{'). 줄바꿈 억제는 "가장 안쪽" 괄호 종류로 결정한다:
    // 가장 안쪽이 '(' 또는 '[' 면 억제, '{'(블록/맵)이거나 비어 있으면 줄바꿈 유효.
    // → `(` 안에 있더라도 `{ }` 블록 안에서는 문장 줄바꿈이 살아 있다
    //   (예: 함수 인자로 넘기는 다중문 클로저).
    brackets: Vec<char>,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        // UTF-8 BOM(U+FEFF)은 무시한다 — Windows 편집기/PowerShell 5.1이 파일 앞에 붙인다.
        let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            brackets: Vec::new(),
        }
    }

    /// 현재 위치에서 줄바꿈이 문장 종결자로 유효한가
    /// (가장 안쪽 괄호가 '{'이거나 최상위면 유효, '(' / '[' 안이면 억제).
    fn newlines_active(&self) -> bool {
        matches!(self.brackets.last(), None | Some('{'))
    }

    /// 소스 코드를 토큰화하여 반환.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, Vec<LexError>> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut errors: Vec<LexError> = Vec::new();
        // 마지막으로 방출한 토큰 종류 (Newline 판단용)
        let mut last_kind: Option<TokenKind> = None;

        // shebang: 파일 첫 줄이 `#!`로 시작하면 무시한다.
        // (`./script.bang` 직접 실행 지원 — `#!/usr/bin/env bang`)
        // 줄바꿈(\n)은 남겨 두어 이후 줄 번호가 정확히 유지된다.
        if self.source.first() == Some(&'#') && self.source.get(1) == Some(&'!') {
            while !self.is_at_end() && self.peek() != '\n' {
                self.advance();
            }
        }

        loop {
            // 여러 줄 블록 주석은 줄바꿈 하나로 취급한다 (문장 종결 규칙 유지)
            let comment_nl = match self.skip_blanks_and_comments() {
                Ok(nl) => nl,
                Err(e) => {
                    errors.push(e);
                    false
                }
            };

            // 줄바꿈 처리
            if comment_nl || (!self.is_at_end() && self.peek() == '\n') {
                // 연속된 \n + 빈 줄 + 주석 줄을 모두 소비
                while !self.is_at_end() && self.peek() == '\n' {
                    self.advance();
                    if let Err(e) = self.skip_blanks_and_comments() {
                        errors.push(e);
                    }
                }
                // 종결 가능한 토큰 뒤이고 줄바꿈이 유효한 컨텍스트면 Newline 방출
                if self.newlines_active() {
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
                if self.newlines_active() {
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
                // f-문자열: f"..." / f"""...""" (Phase 50)
                'f' if self.peek_next() == '"' => {
                    self.advance(); // f
                    self.scan_fstring()
                }
                // 식별자: 유니코드 문자(한글 포함)·'_'로 시작 (Phase 46)
                c if c.is_alphabetic() || c == '_' => Ok(self.scan_identifier()),
                _ => self.scan_punct(),
            };

            match result {
                Ok(tok) => {
                    // 괄호 스택 추적 ('(' '[' '{' 모두). 닫을 때 짝이 맞을 때만 pop.
                    match tok.kind {
                        TokenKind::LParen   => self.brackets.push('('),
                        TokenKind::LBracket => self.brackets.push('['),
                        TokenKind::LBrace   => self.brackets.push('{'),
                        TokenKind::RParen   if self.brackets.last() == Some(&'(') => { self.brackets.pop(); }
                        TokenKind::RBracket if self.brackets.last() == Some(&'[') => { self.brackets.pop(); }
                        TokenKind::RBrace   if self.brackets.last() == Some(&'{') => { self.brackets.pop(); }
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
                | TokenKind::FStr(_)
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

    /// 공백·`//` 주석·`/* */` 블록 주석을 건너뛴다 (줄바꿈 자체는 소비하지 않음).
    /// 건너뛴 블록 주석 안에 줄바꿈이 있었으면 true (호출자가 줄바꿈으로 취급).
    fn skip_blanks_and_comments(&mut self) -> Result<bool, LexError> {
        let mut saw_newline = false;
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
            // `/* ... */` 블록 주석 (중첩 없음)
            if self.peek() == '/' && self.peek_next() == '*' {
                let start = self.span();
                self.advance();
                self.advance();
                loop {
                    if self.is_at_end() {
                        return Err(self.error(LexErrorKind::UnterminatedComment, start.line, start.col));
                    }
                    if self.peek() == '*' && self.peek_next() == '/' {
                        self.advance();
                        self.advance();
                        break;
                    }
                    if self.advance() == '\n' {
                        saw_newline = true;
                    }
                }
                continue;
            }
            break;
        }
        Ok(saw_newline)
    }

    // =========================================================================
    // 숫자 리터럴
    // =========================================================================

    /// 숫자 리터럴: `42`, `1_000`, `3.14`, `1e3`, `2.5e-3`, `0xff`, `0b1010`, `0o17`.
    /// `_`는 숫자 사이에서만 허용(가독성용 구분자).
    fn scan_number(&mut self) -> Result<Token, LexError> {
        let start = self.span();
        let invalid = |me: &Self, s: String| me.error(LexErrorKind::InvalidNumber(s), start.line, start.col);

        // 진법 접두사
        if self.peek() == '0' && matches!(self.peek_next(), 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
            self.advance();
            let p = self.advance().to_ascii_lowercase();
            let radix = match p { 'x' => 16, 'b' => 2, _ => 8 };
            let digits = self.scan_digits(|c| c.is_digit(radix));
            let text = format!("0{p}{digits}");
            if digits.is_empty() || self.peek().is_alphanumeric() {
                return Err(invalid(self, text));
            }
            return i64::from_str_radix(&digits, radix)
                .map(|n| Token::new(TokenKind::Int(n), start))
                .map_err(|_| invalid(self, text));
        }

        let mut num_str = self.scan_digits(|c| c.is_ascii_digit());
        let mut is_float = false;

        if self.peek() == '.' && self.peek_next().is_ascii_digit() {
            is_float = true;
            num_str.push(self.advance()); // '.'
            num_str.push_str(&self.scan_digits(|c| c.is_ascii_digit()));
        }

        // 지수: e/E 뒤에 숫자 또는 부호+숫자가 올 때만 (그 외엔 숫자 뒤 식별자)
        if matches!(self.peek(), 'e' | 'E') {
            let after = self.source.get(self.pos + 1).copied().unwrap_or('\0');
            let after2 = self.source.get(self.pos + 2).copied().unwrap_or('\0');
            if after.is_ascii_digit() || (matches!(after, '+' | '-') && after2.is_ascii_digit()) {
                is_float = true;
                num_str.push(self.advance()); // e
                if matches!(self.peek(), '+' | '-') {
                    num_str.push(self.advance());
                }
                num_str.push_str(&self.scan_digits(|c| c.is_ascii_digit()));
            }
        }

        if self.peek() == '_' || self.peek().is_alphabetic() {
            // 1_ / 1abc 같은 꼬리는 잘못된 리터럴
            while self.peek().is_alphanumeric() || self.peek() == '_' {
                num_str.push(self.advance());
            }
            return Err(invalid(self, num_str));
        }

        if is_float {
            num_str
                .parse::<f64>()
                .map(|n| Token::new(TokenKind::Float(n), start))
                .map_err(|_| invalid(self, num_str.clone()))
        } else {
            num_str
                .parse::<i64>()
                .map(|n| Token::new(TokenKind::Int(n), start))
                .map_err(|_| invalid(self, num_str.clone()))
        }
    }

    /// 숫자(조건 만족 문자)와 그 사이의 `_`를 읽어 `_`를 뺀 문자열을 돌려준다.
    fn scan_digits(&mut self, is_digit: impl Fn(char) -> bool) -> String {
        let mut out = String::new();
        loop {
            let c = self.peek();
            if is_digit(c) {
                out.push(self.advance());
            } else if c == '_' && !out.is_empty() && is_digit(self.peek_next()) {
                self.advance();
            } else {
                break;
            }
        }
        out
    }

    // =========================================================================
    // 문자열 리터럴
    // =========================================================================

    fn scan_string(&mut self) -> Result<Token, LexError> {
        let start = self.span();
        self.advance(); // '"'

        // `"""..."""` 여러 줄 문자열: 줄바꿈을 그대로 담는다 (이스케이프는 동일하게 처리)
        let triple = self.peek() == '"' && self.peek_next() == '"';
        if triple {
            self.advance();
            self.advance();
        }

        let mut value = String::new();

        loop {
            if self.is_at_end() || (!triple && self.peek() == '\n') {
                return Err(self.error(LexErrorKind::UnterminatedString, start.line, start.col));
            }

            let ch = self.advance();
            match ch {
                '"' if triple => {
                    if self.peek() == '"' && self.peek_next() == '"' {
                        self.advance();
                        self.advance();
                        return Ok(Token::new(TokenKind::Str(value), start));
                    }
                    value.push('"');
                }
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
                        'r' => value.push('\r'),
                        '0' => value.push('\0'),
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

    /// `f"..."` 스캔 (여는 `f`는 소비된 상태, 현재 위치는 `"`). 글자 부분은 일반 문자열과 같은
    /// 이스케이프를 처리하고, `{{`/`}}`는 중괄호 글자, `{식[:형식]}`은 식 조각이 된다.
    /// 식 안에서는 문자열·괄호 중첩을 추적해 `f"{m["k"]}"`, `f"{ {"a": 1}["a"] }"`도 허용한다.
    fn scan_fstring(&mut self) -> Result<Token, LexError> {
        let start = Span::new(self.line, self.col.saturating_sub(1));
        let bad = |me: &Self, m: &str| me.error(LexErrorKind::BadFString(m.to_string()), start.line, start.col);
        self.advance(); // "
        let triple = self.peek() == '"' && self.peek_next() == '"';
        if triple {
            self.advance();
            self.advance();
        }
        let mut parts: Vec<FStrPart> = Vec::new();
        let mut lit = String::new();
        loop {
            if self.is_at_end() || (!triple && self.peek() == '\n') {
                return Err(self.error(LexErrorKind::UnterminatedString, start.line, start.col));
            }
            let ch = self.advance();
            match ch {
                '"' if !triple => break,
                '"' if self.peek() == '"' && self.peek_next() == '"' => {
                    self.advance();
                    self.advance();
                    break;
                }
                '{' if self.peek() == '{' => {
                    self.advance();
                    lit.push('{');
                }
                '}' if self.peek() == '}' => {
                    self.advance();
                    lit.push('}');
                }
                '}' => return Err(bad(self, "짝 없는 '}' — 글자 중괄호는 '}}'로 쓰세요")),
                '{' => {
                    if !lit.is_empty() {
                        parts.push(FStrPart::Lit(std::mem::take(&mut lit)));
                    }
                    let (line, col) = (self.line, self.col);
                    let mut src = String::new();
                    let mut spec: Option<String> = None;
                    let mut depth = 0usize;
                    loop {
                        if self.is_at_end() || (!triple && self.peek() == '\n') {
                            return Err(bad(self, "'{' 가 닫히지 않음"));
                        }
                        let c = self.advance();
                        match c {
                            '"' => {
                                // 식 안의 문자열 리터럴: 그대로 복사
                                src.push('"');
                                loop {
                                    if self.is_at_end() || self.peek() == '\n' {
                                        return Err(bad(self, "'{' 가 닫히지 않음 (또는 식 안의 문자열이 닫히지 않음)"));
                                    }
                                    let d = self.advance();
                                    src.push(d);
                                    if d == '\\' && !self.is_at_end() {
                                        src.push(self.advance());
                                    } else if d == '"' {
                                        break;
                                    }
                                }
                            }
                            '{' | '(' | '[' => {
                                depth += 1;
                                src.push(c);
                            }
                            '}' if depth == 0 => break,
                            '}' | ')' | ']' => {
                                depth = depth.saturating_sub(1);
                                src.push(c);
                            }
                            ':' if depth == 0 => {
                                // 형식 지정자: 닫는 '}'까지
                                let mut sp = String::new();
                                loop {
                                    if self.is_at_end() || self.peek() == '\n' || self.peek() == '"' {
                                        return Err(bad(self, "형식 지정자 뒤 '}' 없음"));
                                    }
                                    let d = self.advance();
                                    if d == '}' {
                                        break;
                                    }
                                    sp.push(d);
                                }
                                spec = Some(sp);
                                break;
                            }
                            _ => src.push(c),
                        }
                    }
                    if src.trim().is_empty() {
                        return Err(bad(self, "빈 '{}' — 식이 필요합니다"));
                    }
                    parts.push(FStrPart::Expr { src, line, col, spec });
                }
                '\\' => {
                    if self.is_at_end() {
                        return Err(self.error(LexErrorKind::UnterminatedString, start.line, start.col));
                    }
                    let esc_span = self.span();
                    let esc = self.advance();
                    match esc {
                        'n' => lit.push('\n'),
                        't' => lit.push('\t'),
                        'r' => lit.push('\r'),
                        '0' => lit.push('\0'),
                        '\\' => lit.push('\\'),
                        '"' => lit.push('"'),
                        _ => return Err(self.error(LexErrorKind::InvalidEscape(esc), esc_span.line, esc_span.col)),
                    }
                }
                _ => lit.push(ch),
            }
        }
        if !lit.is_empty() || parts.is_empty() {
            parts.push(FStrPart::Lit(lit));
        }
        Ok(Token::new(TokenKind::FStr(parts), start))
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
            '+' if self.peek() == '=' => { self.advance(); TokenKind::PlusEq }
            '+' => TokenKind::Plus,
            '-' => {
                if self.peek() == '>' {
                    self.advance();
                    TokenKind::Arrow
                } else if self.peek() == '=' {
                    self.advance();
                    TokenKind::MinusEq
                } else {
                    TokenKind::Minus
                }
            }
            '*' if self.peek() == '=' => { self.advance(); TokenKind::StarEq }
            '*' => TokenKind::Star,
            '/' if self.peek() == '=' => { self.advance(); TokenKind::SlashEq }
            '/' => TokenKind::Slash,
            '%' if self.peek() == '=' => { self.advance(); TokenKind::PercentEq }
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

    // 회귀: `(` 안의 `{ }` 블록에서는 줄바꿈이 유효해야 한다(브래킷 스택).
    // foo({ a \n b }) → a 와 b 사이에 Newline 이 방출됨.
    #[test]
    fn test_newline_active_inside_braces_within_parens() {
        let ks = kinds_with_newlines("foo({\na\nb\n})");
        let has_nl = ks.windows(2).any(|w|
            matches!(&w[0], TokenKind::Ident(x) if x == "a")
                && w[1] == TokenKind::Newline);
        assert!(has_nl, "블록 안 'a' 뒤에 Newline 이 있어야 함: {ks:?}");
    }

    // 대조: `(` 안에서 `{ }` 없이 줄바꿈은 여전히 억제된다.
    // (끝의 ')' 뒤 EOF 자동 Newline은 정상이므로, a~b 사이 억제만 확인)
    #[test]
    fn test_newline_suppressed_inside_parens_only() {
        let ks = kinds_with_newlines("foo(a\nb)");
        let a_then_nl = ks.windows(2).any(|w|
            matches!(&w[0], TokenKind::Ident(x) if x == "a")
                && w[1] == TokenKind::Newline);
        assert!(!a_then_nl, "괄호 안(블록 없음) a~b 줄바꿈은 억제돼야 함: {ks:?}");
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

    // ---- Phase 46 ----

    #[test]
    fn test_number_literal_forms() {
        assert_eq!(kinds_no_eof("1_000"), vec![TokenKind::Int(1000)]);
        assert_eq!(kinds_no_eof("0xff 0b101 0o17"), vec![TokenKind::Int(255), TokenKind::Int(5), TokenKind::Int(15)]);
        assert_eq!(kinds_no_eof("1e3 2.5e-3 1E+2"), vec![TokenKind::Float(1000.0), TokenKind::Float(0.0025), TokenKind::Float(100.0)]);
        // 숫자 뒤 '.'+식별자는 기존처럼 분리 (예: 필드 접근 아님 — 숫자 + Dot)
        assert_eq!(kinds_no_eof("1.x"), vec![TokenKind::Int(1), TokenKind::Dot, TokenKind::Ident("x".into())]);
        for bad in ["1_", "0x", "12abc", "0xfg"] {
            assert!(Lexer::new(bad).tokenize().is_err(), "{bad}는 에러여야 함");
        }
    }

    #[test]
    fn test_compound_assign_tokens() {
        assert_eq!(
            kinds_no_eof("a += 1 -= *= /= %="),
            vec![TokenKind::Ident("a".into()), TokenKind::PlusEq, TokenKind::Int(1), TokenKind::MinusEq,
                 TokenKind::StarEq, TokenKind::SlashEq, TokenKind::PercentEq]
        );
        assert_eq!(kinds_no_eof("a->b"), vec![TokenKind::Ident("a".into()), TokenKind::Arrow, TokenKind::Ident("b".into())]);
    }

    #[test]
    fn test_block_comments() {
        assert_eq!(kinds_no_eof("1 /* x */ + 2"), vec![TokenKind::Int(1), TokenKind::Plus, TokenKind::Int(2)]);
        // 여러 줄 블록 주석은 줄바꿈 하나로 취급
        let ks = kinds_with_newlines("a /* x\n y */ b");
        assert_eq!(ks, vec![TokenKind::Ident("a".into()), TokenKind::Newline, TokenKind::Ident("b".into()), TokenKind::Newline]);
        assert!(Lexer::new("/* open").tokenize().is_err());
    }

    #[test]
    fn test_triple_quoted_string() {
        assert_eq!(kinds_no_eof("\"\"\"a\n\"b\"\\t\"\"\""), vec![TokenKind::Str("a\n\"b\"\t".into())]);
        assert_eq!(kinds_no_eof("\"\""), vec![TokenKind::Str(String::new())]);
        assert!(Lexer::new("\"\"\"never closed").tokenize().is_err());
    }

    #[test]
    fn test_unicode_identifiers() {
        assert_eq!(
            kinds_no_eof("let 이름_1 = 점수"),
            vec![TokenKind::Let, TokenKind::Ident("이름_1".into()), TokenKind::Eq, TokenKind::Ident("점수".into())]
        );
    }

    #[test]
    fn test_fstring_tokens() {
        let toks = kinds_no_eof("f\"a{x}b{y:.2f}{{c}}\"");
        assert_eq!(toks.len(), 1);
        let TokenKind::FStr(parts) = &toks[0] else { panic!("FStr 아님: {toks:?}") };
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], FStrPart::Lit("a".into()));
        assert!(matches!(&parts[1], FStrPart::Expr { src, spec: None, line: 1, col: 5 } if src == "x"));
        assert_eq!(parts[2], FStrPart::Lit("b".into()));
        assert!(matches!(&parts[3], FStrPart::Expr { src, spec: Some(sp), .. } if src == "y" && sp == ".2f"));
        assert_eq!(parts[4], FStrPart::Lit("{c}".into()));
        // 식 안의 문자열·중첩 괄호
        let toks = kinds_no_eof("f\"{m[\"k\"]} { {\"a\": 1}[\"a\"] }\"");
        let TokenKind::FStr(parts) = &toks[0] else { panic!() };
        assert!(matches!(&parts[0], FStrPart::Expr { src, .. } if src == "m[\"k\"]"));
        // f 다음에 따옴표가 아니면 그냥 식별자
        assert_eq!(kinds_no_eof("f (x)")[0], TokenKind::Ident("f".into()));
        for bad in ["f\"{\"", "f\"}\"", "f\"{}\"", "f\"{x\""] {
            assert!(Lexer::new(bad).tokenize().is_err(), "{bad}");
        }
    }

    #[test]
    fn test_utf8_bom_skipped() {
        assert_eq!(kinds_no_eof("\u{FEFF}42"), kinds_no_eof("42"));
        // BOM + shebang 조합도 허용, 열 번호는 BOM을 세지 않는다
        let mut lexer = Lexer::new("\u{FEFF}#!/usr/bin/env bang\nx");
        let toks = lexer.tokenize().unwrap();
        let x = toks.iter().find(|t| matches!(t.kind, TokenKind::Ident(_))).unwrap();
        assert_eq!((x.span.line, x.span.col), (2, 1));
    }

    #[test]
    fn test_shebang_skipped() {
        // 첫 줄 `#!`는 무시되고, 이후 코드만 토큰화된다.
        assert_eq!(
            kinds_no_eof("#!/usr/bin/env bang\n42"),
            vec![TokenKind::Int(42)]
        );
    }

    #[test]
    fn test_shebang_preserves_line_numbers() {
        // shebang 줄을 무시하되 줄 번호는 보존되어야 한다 (코드는 2번 줄).
        let mut lexer = Lexer::new("#!/usr/bin/env bang\nx");
        let tokens = lexer.tokenize().expect("tokenize 실패");
        let ident = tokens
            .iter()
            .find(|t| matches!(t.kind, TokenKind::Ident(_)))
            .expect("식별자 없음");
        assert_eq!(ident.span.line, 2);
    }

    #[test]
    fn test_hash_not_shebang_is_error() {
        // `#!`가 아닌 단독 `#`는 여전히 예상하지 못한 문자 오류.
        let mut lexer = Lexer::new("# 주석 아님\n42");
        assert!(lexer.tokenize().is_err());
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
