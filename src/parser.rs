// Bang 프로그래밍 언어 — 파서
//
// 토큰 스트림 → AST. 재귀 하강 + Pratt 파싱.
//
// 우선순위 (낮은 순):
//   1  =         (우결합)
//   2  or
//   3  and
//   4  == !=
//   5  < <= > >=
//   6  + -
//   7  * / %
//   8  - not spawn  (전위 단항, right_bp=15)
//   9  () [] .       (후위, left_bp=17)

use crate::ast::*;
use crate::lexer::token::{Span, Token, TokenKind};
use std::fmt;

// =============================================================================
// 파스 에러
// =============================================================================

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}:{}] 파스 오류: {}", self.span.line, self.span.col, self.message)
    }
}

impl std::error::Error for ParseError {}

// =============================================================================
// Parser
// =============================================================================

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, errors: Vec::new() }
    }

    pub fn parse(mut self) -> Result<Program, Vec<ParseError>> {
        let stmts = self.parse_program();
        if self.errors.is_empty() {
            Ok(Program { stmts })
        } else {
            Err(self.errors)
        }
    }

    // =========================================================================
    // 토큰 스트림 헬퍼
    // =========================================================================

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    /// pos+n 위치의 토큰 종류. 범위 초과 시 Eof 반환 (마지막이 항상 Eof).
    fn peek_n(&self, n: usize) -> &TokenKind {
        let idx = self.pos + n;
        if idx < self.tokens.len() {
            &self.tokens[idx].kind
        } else {
            &self.tokens[self.tokens.len() - 1].kind
        }
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
    }

    // =========================================================================
    // 에러 · 복구
    // =========================================================================

    fn error_at(&mut self, span: Span, msg: &str) {
        self.errors.push(ParseError { message: msg.to_string(), span });
    }

    /// 패닉 모드 복구: Newline 또는 `}` 까지 토큰을 버린다.
    fn synchronize(&mut self) {
        loop {
            match self.peek() {
                TokenKind::Newline => {
                    self.advance();
                    break;
                }
                TokenKind::RBrace | TokenKind::Eof => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// 특정 토큰을 기대하고 소비. 실패 시 에러 기록 후 None.
    fn expect(&mut self, expected: &TokenKind) -> Option<Span> {
        let span = self.peek_span();
        if self.peek() == expected {
            self.advance();
            Some(span)
        } else {
            self.error_at(
                span,
                &format!("'{expected}' 기대, '{}' 발견", self.peek()),
            );
            None
        }
    }

    fn expect_ident(&mut self) -> Option<String> {
        let span = self.peek_span();
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            Some(name)
        } else {
            self.error_at(span, &format!("식별자 기대, '{}' 발견", self.peek()));
            None
        }
    }

    /// 문장 종결자 소비: Newline → 소비, `}` / Eof → 소비 안 함, 그 외 → 에러+복구
    fn consume_stmt_end(&mut self) {
        match self.peek() {
            TokenKind::Newline => { self.advance(); }
            TokenKind::RBrace | TokenKind::Eof => {}
            _ => {
                let span = self.peek_span();
                self.error_at(span, &format!("문장 끝에 줄바꿈 기대, '{}' 발견", self.peek()));
                self.synchronize();
            }
        }
    }

    // =========================================================================
    // 최상위 / 블록
    // =========================================================================

    fn parse_program(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }
            match self.parse_stmt() {
                Some(s) => stmts.push(s),
                None => { /* 에러 복구 후 계속 */ }
            }
        }
        stmts
    }

    fn parse_block(&mut self) -> Option<Block> {
        let span = self.peek_span();
        if !matches!(self.peek(), TokenKind::LBrace) {
            self.error_at(span, &format!("블록 시작 '{{' 기대, '{}' 발견", self.peek()));
            return None;
        }
        self.advance(); // {

        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            if let Some(s) = self.parse_stmt() {
                stmts.push(s);
            }
        }

        if !matches!(self.peek(), TokenKind::RBrace) {
            self.error_at(self.peek_span(), "블록 종료 '}' 기대");
            return Some(Block { stmts, span });
        }
        self.advance(); // }
        Some(Block { stmts, span })
    }

    // =========================================================================
    // 문 파서
    // =========================================================================

    fn parse_stmt(&mut self) -> Option<Stmt> {
        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            return None;
        }

        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Let => self.parse_let(span),
            // fn 다음이 Ident 면 이름있는 함수 선언, LParen 이면 람다 표현식 문장
            TokenKind::Fn if matches!(self.peek_n(1), TokenKind::Ident(_)) => {
                self.parse_fn_decl(span)
            }
            TokenKind::If       => self.parse_if(span),
            TokenKind::While    => self.parse_while(span),
            TokenKind::For      => self.parse_for(span),
            TokenKind::Return   => self.parse_return(span),
            TokenKind::Break    => {
                self.advance();
                self.consume_stmt_end();
                Some(Stmt { kind: StmtKind::Break, span })
            }
            TokenKind::Continue => {
                self.advance();
                self.consume_stmt_end();
                Some(Stmt { kind: StmtKind::Continue, span })
            }
            TokenKind::Parallel => self.parse_parallel(span),
            // 문장 자리의 { 는 블록 (표현식 자리의 { 는 맵 리터럴)
            TokenKind::LBrace   => {
                let block = self.parse_block()?;
                self.consume_stmt_end();
                Some(Stmt { kind: StmtKind::Block(block), span })
            }
            _ => self.parse_expr_stmt(span),
        }
    }

    fn parse_let(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // let
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Let { name, value }, span })
    }

    /// `fn name(params) { body }` → `let name = Function { name: Some(name), ... }`
    fn parse_fn_decl(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // fn
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_block()?;
        self.consume_stmt_end();
        let func = Expr {
            kind: ExprKind::Function { name: Some(name.clone()), params, body },
            span,
        };
        Some(Stmt { kind: StmtKind::Let { name, value: func }, span })
    }

    fn parse_if(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;

        // else / else if — 같은 줄 또는 Newline 하나 건너도 허용
        let else_ = if matches!(self.peek(), TokenKind::Else) {
            self.advance(); // else
            Some(self.parse_else_body()?)
        } else if matches!(self.peek(), TokenKind::Newline)
            && matches!(self.peek_n(1), TokenKind::Else)
        {
            self.advance(); // Newline
            self.advance(); // else
            Some(self.parse_else_body()?)
        } else {
            None
        };

        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::If { cond, then, else_ }, span })
    }

    /// `else` 를 이미 소비한 후 호출. `if` 이면 else-if 체인, 그 외이면 블록.
    fn parse_else_body(&mut self) -> Option<Block> {
        if matches!(self.peek(), TokenKind::If) {
            let span = self.peek_span();
            let inner = self.parse_if(span)?;
            Some(Block { stmts: vec![inner], span })
        } else {
            self.parse_block()
        }
    }

    fn parse_while(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // while
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::While { cond, body }, span })
    }

    fn parse_for(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // for
        let var = self.expect_ident()?;
        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::For { var, iter, body }, span })
    }

    fn parse_return(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // return
        // 값 없는 return: 다음이 Newline / } / Eof
        let value = if matches!(self.peek(), TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Return(value), span })
    }

    fn parse_parallel(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // parallel
        let body = self.parse_block()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Parallel(body), span })
    }

    fn parse_expr_stmt(&mut self, span: Span) -> Option<Stmt> {
        let expr = self.parse_expr()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Expr(expr), span })
    }

    // =========================================================================
    // 파라미터 목록
    // =========================================================================

    fn parse_params(&mut self) -> Option<Vec<String>> {
        let mut params = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            let name = self.expect_ident()?;
            params.push(name);
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Some(params)
    }

    // =========================================================================
    // 표현식 파서 — Pratt
    // =========================================================================

    pub fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Expr> {
        let mut lhs = self.parse_prefix()?;

        loop {
            // 후위 연산자 (left_bp = 17)
            if 17 >= min_bp {
                match self.peek().clone() {
                    TokenKind::LParen => {
                        let span = lhs.span;
                        self.advance(); // (
                        let args = self.parse_args()?;
                        self.expect(&TokenKind::RParen)?;
                        lhs = Expr { kind: ExprKind::Call { callee: Box::new(lhs), args }, span };
                        continue;
                    }
                    TokenKind::LBracket => {
                        let span = lhs.span;
                        self.advance(); // [
                        let index = self.parse_expr()?;
                        self.expect(&TokenKind::RBracket)?;
                        lhs = Expr {
                            kind: ExprKind::Index { target: Box::new(lhs), index: Box::new(index) },
                            span,
                        };
                        continue;
                    }
                    TokenKind::Dot => {
                        let span = lhs.span;
                        self.advance(); // .
                        let name = self.expect_ident()?;
                        lhs = Expr { kind: ExprKind::Field { target: Box::new(lhs), name }, span };
                        continue;
                    }
                    _ => {}
                }
            }

            // 이항 연산자
            let Some((l_bp, r_bp)) = infix_bp(self.peek()) else { break };
            if l_bp < min_bp {
                break;
            }

            let op_span = self.peek_span();
            let op_tok = self.advance().kind.clone();

            if op_tok == TokenKind::Eq {
                // 대입: 좌변 유효성 검사
                if !is_assign_target(&lhs) {
                    self.error_at(op_span, "대입 대상은 변수·인덱스·필드만 허용됩니다");
                }
                let value = self.parse_expr_bp(r_bp)?;
                lhs = Expr {
                    kind: ExprKind::Assign { target: Box::new(lhs), value: Box::new(value) },
                    span: op_span,
                };
            } else {
                let op = token_to_binary_op(&op_tok);
                let rhs = self.parse_expr_bp(r_bp)?;
                let span = lhs.span;
                lhs = Expr {
                    kind: ExprKind::Binary { op, left: Box::new(lhs), right: Box::new(rhs) },
                    span,
                };
            }
        }

        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_expr_bp(15)?;
                Some(Expr { kind: ExprKind::Unary { op: UnaryOp::Neg, expr: Box::new(expr) }, span })
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_expr_bp(15)?;
                Some(Expr { kind: ExprKind::Unary { op: UnaryOp::Not, expr: Box::new(expr) }, span })
            }
            TokenKind::Spawn => {
                self.advance();
                let expr = self.parse_expr_bp(15)?;
                Some(Expr { kind: ExprKind::Spawn(Box::new(expr)), span })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Int(n) => {
                self.advance();
                Some(Expr { kind: ExprKind::Int(n), span })
            }
            TokenKind::Float(n) => {
                self.advance();
                Some(Expr { kind: ExprKind::Float(n), span })
            }
            TokenKind::Str(s) => {
                self.advance();
                Some(Expr { kind: ExprKind::Str(s), span })
            }
            TokenKind::True => {
                self.advance();
                Some(Expr { kind: ExprKind::Bool(true), span })
            }
            TokenKind::False => {
                self.advance();
                Some(Expr { kind: ExprKind::Bool(false), span })
            }
            TokenKind::Nil => {
                self.advance();
                Some(Expr { kind: ExprKind::Nil, span })
            }
            TokenKind::Ident(name) => {
                self.advance();
                Some(Expr { kind: ExprKind::Ident(name), span })
            }
            TokenKind::LParen => {
                self.advance(); // (
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Some(expr)
            }
            TokenKind::LBracket => self.parse_list(span),
            TokenKind::LBrace => self.parse_map(span),
            TokenKind::Fn => self.parse_lambda(span),
            _ => {
                self.error_at(span, &format!("표현식 시작 불가 토큰: '{}'", self.peek()));
                self.synchronize();
                None
            }
        }
    }

    // =========================================================================
    // 리스트 / 맵 / 람다
    // =========================================================================

    fn parse_list(&mut self, span: Span) -> Option<Expr> {
        self.advance(); // [
        let mut items = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            items.push(self.parse_expr()?);
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }
        self.skip_newlines();
        self.expect(&TokenKind::RBracket)?;
        Some(Expr { kind: ExprKind::List(items), span })
    }

    fn parse_map(&mut self, span: Span) -> Option<Expr> {
        self.advance(); // {
        let mut entries = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let key = match self.peek().clone() {
                TokenKind::Str(s) => { self.advance(); MapKey::Str(s) }
                TokenKind::Ident(name) => { self.advance(); MapKey::Ident(name) }
                _ => {
                    self.error_at(self.peek_span(), "맵 키는 문자열 리터럴 또는 식별자여야 합니다");
                    self.synchronize();
                    return None;
                }
            };
            self.expect(&TokenKind::Colon)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            entries.push((key, value));
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }
        self.skip_newlines();
        self.expect(&TokenKind::RBrace)?;
        Some(Expr { kind: ExprKind::Map(entries), span })
    }

    fn parse_lambda(&mut self, span: Span) -> Option<Expr> {
        self.advance(); // fn
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_block()?;
        Some(Expr { kind: ExprKind::Function { name: None, params, body }, span })
    }

    // =========================================================================
    // 인수 목록
    // =========================================================================

    fn parse_args(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            args.push(self.parse_expr()?);
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }
        Some(args)
    }
}

// =============================================================================
// Pratt 헬퍼
// =============================================================================

/// 이항 연산자의 (left_bp, right_bp). None 이면 이항 연산자 아님.
fn infix_bp(tok: &TokenKind) -> Option<(u8, u8)> {
    match tok {
        TokenKind::Eq                                    => Some((2, 1)),   // 우결합
        TokenKind::Or                                    => Some((3, 4)),
        TokenKind::And                                   => Some((5, 6)),
        TokenKind::EqEq | TokenKind::BangEq              => Some((7, 8)),
        TokenKind::Lt | TokenKind::LtEq
        | TokenKind::Gt | TokenKind::GtEq               => Some((9, 10)),
        TokenKind::Plus | TokenKind::Minus               => Some((11, 12)),
        TokenKind::Star | TokenKind::Slash
        | TokenKind::Percent                             => Some((13, 14)),
        _ => None,
    }
}

fn token_to_binary_op(tok: &TokenKind) -> BinaryOp {
    match tok {
        TokenKind::Plus    => BinaryOp::Add,
        TokenKind::Minus   => BinaryOp::Sub,
        TokenKind::Star    => BinaryOp::Mul,
        TokenKind::Slash   => BinaryOp::Div,
        TokenKind::Percent => BinaryOp::Mod,
        TokenKind::EqEq    => BinaryOp::Eq,
        TokenKind::BangEq  => BinaryOp::Ne,
        TokenKind::Lt      => BinaryOp::Lt,
        TokenKind::LtEq    => BinaryOp::Le,
        TokenKind::Gt      => BinaryOp::Gt,
        TokenKind::GtEq    => BinaryOp::Ge,
        TokenKind::And     => BinaryOp::And,
        TokenKind::Or      => BinaryOp::Or,
        _ => unreachable!("token_to_binary_op: {tok:?}"),
    }
}

fn is_assign_target(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Ident(_) | ExprKind::Index { .. } | ExprKind::Field { .. }
    )
}

// =============================================================================
// 단위 테스트
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Program {
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().expect("렉싱 실패");
        Parser::new(tokens).parse().unwrap_or_else(|errs| {
            for e in &errs { eprintln!("{e}"); }
            panic!("파싱 실패: {src:?}");
        })
    }

    fn parse_expr_src(src: &str) -> Expr {
        let prog = parse(src);
        match prog.stmts.into_iter().next().unwrap().kind {
            StmtKind::Expr(e) => e,
            other => panic!("표현식 문장 기대, 발견: {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // 우선순위 · 결합성
    // -------------------------------------------------------------------------

    #[test]
    fn test_add_mul_precedence() {
        // 1 + 2 * 3  →  1 + (2 * 3)
        let e = parse_expr_src("1 + 2 * 3");
        match &e.kind {
            ExprKind::Binary { op: BinaryOp::Add, left, right } => {
                assert!(matches!(left.kind, ExprKind::Int(1)));
                match &right.kind {
                    ExprKind::Binary { op: BinaryOp::Mul, left: l, right: r } => {
                        assert!(matches!(l.kind, ExprKind::Int(2)));
                        assert!(matches!(r.kind, ExprKind::Int(3)));
                    }
                    _ => panic!("right 는 Mul 이어야 함"),
                }
            }
            _ => panic!("Add 기대"),
        }
    }

    #[test]
    fn test_or_and_precedence() {
        // a || b && c  →  a || (b && c)
        let e = parse_expr_src("a or b and c");
        match &e.kind {
            ExprKind::Binary { op: BinaryOp::Or, right, .. } => {
                assert!(matches!(right.kind, ExprKind::Binary { op: BinaryOp::And, .. }));
            }
            _ => panic!("Or 기대"),
        }
    }

    #[test]
    fn test_assign_right_assoc() {
        // a = b = c  →  a = (b = c)
        let e = parse_expr_src("a = b = c");
        match &e.kind {
            ExprKind::Assign { target, value } => {
                assert!(matches!(target.kind, ExprKind::Ident(ref n) if n == "a"));
                assert!(matches!(value.kind, ExprKind::Assign { .. }));
            }
            _ => panic!("Assign 기대"),
        }
    }

    #[test]
    fn test_unary_neg_field_call() {
        // -x.f()  →  -(x.f())   (후위가 더 강함)
        let e = parse_expr_src("-x.f()");
        match &e.kind {
            ExprKind::Unary { op: UnaryOp::Neg, expr } => {
                match &expr.kind {
                    ExprKind::Call { callee, args } => {
                        assert!(args.is_empty());
                        assert!(matches!(callee.kind, ExprKind::Field { .. }));
                    }
                    _ => panic!("Call 기대"),
                }
            }
            _ => panic!("Neg 기대"),
        }
    }

    // -------------------------------------------------------------------------
    // 함수 · 호출 · 클로저
    // -------------------------------------------------------------------------

    #[test]
    fn test_fn_decl_desugared_to_let() {
        let prog = parse("fn add(a, b) {\n  return a + b\n}");
        match &prog.stmts[0].kind {
            StmtKind::Let { name, value } => {
                assert_eq!(name, "add");
                match &value.kind {
                    ExprKind::Function { name: Some(n), params, .. } => {
                        assert_eq!(n, "add");
                        assert_eq!(params, &["a", "b"]);
                    }
                    _ => panic!("Function 기대"),
                }
            }
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_anonymous_fn() {
        let e = parse_expr_src("fn(x) { return x }");
        match &e.kind {
            ExprKind::Function { name: None, params, .. } => {
                assert_eq!(params, &["x"]);
            }
            _ => panic!("익명 함수 기대"),
        }
    }

    #[test]
    fn test_closure_assigned() {
        let prog = parse("let add10 = make_adder(10)");
        match &prog.stmts[0].kind {
            StmtKind::Let { name, value } => {
                assert_eq!(name, "add10");
                assert!(matches!(value.kind, ExprKind::Call { .. }));
            }
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_method_call() {
        // ch.send(v)  →  Call { callee: Field(ch, "send"), args: [v] }
        let e = parse_expr_src("ch.send(v)");
        match &e.kind {
            ExprKind::Call { callee, args } => {
                assert_eq!(args.len(), 1);
                match &callee.kind {
                    ExprKind::Field { name, .. } => assert_eq!(name, "send"),
                    _ => panic!("Field 기대"),
                }
            }
            _ => panic!("Call 기대"),
        }
    }

    // -------------------------------------------------------------------------
    // 리스트 / 맵 / 인덱싱
    // -------------------------------------------------------------------------

    #[test]
    fn test_list_literal() {
        let e = parse_expr_src("[1, 2, 3]");
        match &e.kind {
            ExprKind::List(items) => {
                assert_eq!(items.len(), 3);
                assert!(matches!(items[0].kind, ExprKind::Int(1)));
            }
            _ => panic!("List 기대"),
        }
    }

    #[test]
    fn test_empty_list() {
        let e = parse_expr_src("[]");
        assert!(matches!(e.kind, ExprKind::List(ref v) if v.is_empty()));
    }

    // 맵은 문장 자리에서는 블록으로 파싱되므로, let 우변(표현식 자리)에서 테스트한다.

    #[test]
    fn test_map_literal_string_keys() {
        let prog = parse(r#"let m = {"name": "Alice", "age": 30}"#);
        match &prog.stmts[0].kind {
            StmtKind::Let { value, .. } => match &value.kind {
                ExprKind::Map(entries) => {
                    assert_eq!(entries.len(), 2);
                    assert!(matches!(entries[0].0, MapKey::Str(ref s) if s == "name"));
                }
                _ => panic!("Map 기대"),
            },
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_map_literal_ident_keys() {
        let prog = parse("let m = { user: name }");
        match &prog.stmts[0].kind {
            StmtKind::Let { value, .. } => match &value.kind {
                ExprKind::Map(entries) => {
                    assert_eq!(entries.len(), 1);
                    assert!(matches!(entries[0].0, MapKey::Ident(ref s) if s == "user"));
                }
                _ => panic!("Map 기대"),
            },
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_empty_map() {
        let prog = parse("let m = {}");
        match &prog.stmts[0].kind {
            StmtKind::Let { value, .. } => {
                assert!(matches!(value.kind, ExprKind::Map(ref v) if v.is_empty()));
            }
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_index_expr() {
        let e = parse_expr_src("arr[0]");
        assert!(matches!(e.kind, ExprKind::Index { .. }));
    }

    #[test]
    fn test_nested_index() {
        let e = parse_expr_src("a[b[0]]");
        assert!(matches!(e.kind, ExprKind::Index { .. }));
    }

    // -------------------------------------------------------------------------
    // 제어구문
    // -------------------------------------------------------------------------

    #[test]
    fn test_if_no_else() {
        let prog = parse("if x > 0 {\n  print(x)\n}");
        match &prog.stmts[0].kind {
            StmtKind::If { else_: None, .. } => {}
            _ => panic!("If (else 없음) 기대"),
        }
    }

    #[test]
    fn test_if_else() {
        let prog = parse("if x > 0 {\n  a\n} else {\n  b\n}");
        match &prog.stmts[0].kind {
            StmtKind::If { else_: Some(_), .. } => {}
            _ => panic!("If-else 기대"),
        }
    }

    #[test]
    fn test_else_if_chain() {
        let src = "if a {\n  x\n} else if b {\n  y\n} else {\n  z\n}";
        let prog = parse(src);
        match &prog.stmts[0].kind {
            StmtKind::If { else_: Some(else_block), .. } => {
                match &else_block.stmts[0].kind {
                    StmtKind::If { .. } => {}
                    _ => panic!("else if 는 If 노드여야 함"),
                }
            }
            _ => panic!("If-else-if 기대"),
        }
    }

    #[test]
    fn test_while_loop() {
        let prog = parse("while i < 10 {\n  i = i + 1\n}");
        assert!(matches!(prog.stmts[0].kind, StmtKind::While { .. }));
    }

    #[test]
    fn test_for_in_loop() {
        let prog = parse("for item in list {\n  print(item)\n}");
        match &prog.stmts[0].kind {
            StmtKind::For { var, .. } => assert_eq!(var, "item"),
            _ => panic!("For 기대"),
        }
    }

    #[test]
    fn test_return_with_value() {
        let prog = parse("fn f() {\n  return 42\n}");
        match &prog.stmts[0].kind {
            StmtKind::Let { value, .. } => {
                match &value.kind {
                    ExprKind::Function { body, .. } => {
                        match &body.stmts[0].kind {
                            StmtKind::Return(Some(e)) => {
                                assert!(matches!(e.kind, ExprKind::Int(42)));
                            }
                            _ => panic!("Return(Some) 기대"),
                        }
                    }
                    _ => panic!("Function 기대"),
                }
            }
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_return_nil() {
        let prog = parse("fn f() {\n  return\n}");
        match &prog.stmts[0].kind {
            StmtKind::Let { value, .. } => {
                match &value.kind {
                    ExprKind::Function { body, .. } => {
                        assert!(matches!(body.stmts[0].kind, StmtKind::Return(None)));
                    }
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_break_continue() {
        let prog = parse("while true {\n  break\n  continue\n}");
        match &prog.stmts[0].kind {
            StmtKind::While { body, .. } => {
                assert!(matches!(body.stmts[0].kind, StmtKind::Break));
                assert!(matches!(body.stmts[1].kind, StmtKind::Continue));
            }
            _ => panic!("While 기대"),
        }
    }

    // -------------------------------------------------------------------------
    // 동시성
    // -------------------------------------------------------------------------

    #[test]
    fn test_spawn_call() {
        // spawn f(x)  →  Spawn(Call(f, [x]))
        let e = parse_expr_src("spawn f(x)");
        match &e.kind {
            ExprKind::Spawn(inner) => {
                assert!(matches!(inner.kind, ExprKind::Call { .. }));
            }
            _ => panic!("Spawn 기대"),
        }
    }

    #[test]
    fn test_spawn_paren() {
        // spawn (a + b)  →  Spawn(Binary(+, a, b))
        let e = parse_expr_src("spawn (a + b)");
        match &e.kind {
            ExprKind::Spawn(inner) => {
                assert!(matches!(inner.kind, ExprKind::Binary { op: BinaryOp::Add, .. }));
            }
            _ => panic!("Spawn 기대"),
        }
    }

    #[test]
    fn test_spawn_binds_to_call_not_add() {
        // spawn f(x) + 1  →  (spawn f(x)) + 1
        let e = parse_expr_src("spawn f(x) + 1");
        match &e.kind {
            ExprKind::Binary { op: BinaryOp::Add, left, .. } => {
                assert!(matches!(left.kind, ExprKind::Spawn(_)));
            }
            _ => panic!("Add(Spawn, 1) 기대"),
        }
    }

    #[test]
    fn test_parallel_block() {
        let prog = parse("parallel {\n  spawn work()\n}");
        assert!(matches!(prog.stmts[0].kind, StmtKind::Parallel(_)));
    }

    // -------------------------------------------------------------------------
    // 맥락별 중괄호 모호성
    // -------------------------------------------------------------------------

    #[test]
    fn test_brace_as_map_in_expr() {
        // 표현식 자리(let 우변)의 { → 맵 리터럴
        let prog = parse(r#"let m = {"key": 1}"#);
        match &prog.stmts[0].kind {
            StmtKind::Let { value, .. } => {
                assert!(matches!(value.kind, ExprKind::Map(_)));
            }
            _ => panic!("Let 기대"),
        }
    }

    #[test]
    fn test_brace_as_block_in_stmt() {
        // 문장 자리의 { → 블록
        let prog = parse("{\n  let x = 1\n}");
        assert!(matches!(prog.stmts[0].kind, StmtKind::Block(_)));
    }

    // -------------------------------------------------------------------------
    // 에러 복구
    // -------------------------------------------------------------------------

    #[test]
    fn test_invalid_assign_target_reports_error() {
        let mut lex = Lexer::new("1 + 2 = 3");
        let tokens = lex.tokenize().unwrap();
        let parser = Parser::new(tokens);
        let result = parser.parse();
        // 에러가 보고되어야 함
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_errors_recovered() {
        // 첫 번째 줄 에러 후 두 번째 줄을 복구해서 계속 파싱
        let src = "let = 1\nlet y = 2\n";
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().unwrap();
        let result = Parser::new(tokens).parse();
        // 에러가 보고되어야 함
        assert!(result.is_err());
    }

    #[test]
    fn test_unterminated_paren_error() {
        let mut lex = Lexer::new("f(1, 2");
        let tokens = lex.tokenize().unwrap();
        let result = Parser::new(tokens).parse();
        assert!(result.is_err());
    }
}
