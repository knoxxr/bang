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

/// 파서 재귀(식·블록 중첩) 깊이 상한. 재귀 하강 파서가 악의적/우발적 깊은
/// 중첩(예: 수만 개의 '(' 또는 '{')에서 Rust 스택 오버플로로 abort하지 않게 한다.
/// 주의: spawn 워커/테스트 스레드는 스택이 2MB라(메인 8MB) 한도를 보수적으로 잡는다.
/// (실사용 코드의 중첩은 수십 수준 — 128이면 충분히 관대함)
const MAX_NEST_DEPTH: usize = 128;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub errors: Vec<ParseError>,
    /// 현재 식/블록 중첩 깊이 (MAX_NEST_DEPTH 초과 시 파스 에러)
    depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, errors: Vec::new(), depth: 0 }
    }

    /// 중첩 깊이 증가. 상한 초과면 에러 기록 후 false (호출자는 None 반환).
    fn enter_nest(&mut self) -> bool {
        self.depth += 1;
        if self.depth > MAX_NEST_DEPTH {
            let span = self.peek_span();
            self.error_at(span, "중첩이 너무 깊습니다 (식/블록 중첩 한도 초과)");
            return false;
        }
        true
    }

    fn exit_nest(&mut self) {
        self.depth = self.depth.saturating_sub(1);
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
            let before = self.pos;
            match self.parse_stmt() {
                Some(s) => stmts.push(s),
                None => { /* 에러 복구 후 계속 */ }
            }
            // 진행 보장: parse_stmt가 토큰을 하나도 소비하지 못했으면(예: 떠도는 '}')
            // 무한루프가 되므로 에러를 남기고 한 토큰을 강제로 건너뛴다.
            if self.pos == before {
                let sp = self.peek_span();
                let msg = format!("예상치 못한 토큰: '{}'", self.peek());
                self.error_at(sp, &msg);
                self.advance();
            }
        }
        stmts
    }

    fn parse_block(&mut self) -> Option<Block> {
        // 깊이 가드: 깊은 '{' 중첩에서 스택 오버플로 방지
        if !self.enter_nest() {
            self.exit_nest();
            // 진행 보장: 여는 '{'를 소비해 무한루프 방지
            if matches!(self.peek(), TokenKind::LBrace) { self.advance(); }
            return None;
        }
        let r = self.parse_block_inner();
        self.exit_nest();
        r
    }

    fn parse_block_inner(&mut self) -> Option<Block> {
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
            let before = self.pos;
            if let Some(s) = self.parse_stmt() {
                stmts.push(s);
            }
            // 진행 보장: 토큰을 소비하지 못한 에러 상황에서 무한루프 방지.
            if self.pos == before {
                let sp = self.peek_span();
                let msg = format!("예상치 못한 토큰: '{}'", self.peek());
                self.error_at(sp, &msg);
                self.advance();
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
            TokenKind::Try      => self.parse_try(span),
            TokenKind::Throw    => self.parse_throw(span),
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
        // 분해 선언: let [a, b] = expr
        if matches!(self.peek(), TokenKind::LBracket) {
            self.advance(); // [
            let mut names = Vec::new();
            while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                names.push(self.expect_ident()?);
                if matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(&TokenKind::RBracket)?;
            if names.is_empty() {
                self.error_at(span, "분해 선언에는 이름이 하나 이상 필요합니다");
                return None;
            }
            self.expect(&TokenKind::Eq)?;
            let value = self.parse_expr()?;
            self.consume_stmt_end();
            return Some(Stmt { kind: StmtKind::LetUnpack { names, value }, span });
        }
        let name = self.expect_ident()?;
        // 선택적 타입 힌트: `: type`
        let ty = if matches!(self.peek(), TokenKind::Colon) {
            self.advance(); // :
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Let { name, ty, value }, span })
    }

    /// `fn name(params) [-> ret] { body }` → `let name = Function { name: Some(name), ... }`
    fn parse_fn_decl(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // fn
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let (params, param_types, param_defaults) = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let ret_type = self.parse_ret_type()?;
        let body = self.parse_block()?;
        self.consume_stmt_end();
        let func = Expr {
            kind: ExprKind::Function {
                name: Some(name.clone()), params, param_types, param_defaults, ret_type, body,
            },
            span,
        };
        Some(Stmt { kind: StmtKind::Let { name, ty: None, value: func }, span })
    }

    fn parse_if(&mut self, span: Span) -> Option<Stmt> {
        let stmt = self.parse_if_no_end(span)?;
        // 종결 소비는 최외곽 if에서 한 번만. else-if 체인은 parse_if_no_end로
        // 재귀하므로, 내부 if가 종결 줄바꿈을 먹어 외부가 다시 못 찾던 버그를 피한다.
        self.consume_stmt_end();
        Some(stmt)
    }

    /// if 문 파싱(종결 소비 없음). else-if 체인 재귀에 재사용.
    fn parse_if_no_end(&mut self, span: Span) -> Option<Stmt> {
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

        Some(Stmt { kind: StmtKind::If { cond, then, else_ }, span })
    }

    /// `else` 를 이미 소비한 후 호출. `if` 이면 else-if 체인, 그 외이면 블록.
    fn parse_else_body(&mut self) -> Option<Block> {
        if matches!(self.peek(), TokenKind::If) {
            let span = self.peek_span();
            let inner = self.parse_if_no_end(span)?; // 종결 소비 없이 재귀
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
        let mut vars = vec![self.expect_ident()?];
        while matches!(self.peek(), TokenKind::Comma) {
            self.advance(); // ,
            vars.push(self.expect_ident()?);
        }
        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::For { vars, iter, body }, span })
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

    fn parse_try(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // try
        let body = self.parse_block()?;
        // catch — 같은 줄 또는 Newline 하나 건너도 허용
        if matches!(self.peek(), TokenKind::Newline)
            && matches!(self.peek_n(1), TokenKind::Catch)
        {
            self.advance(); // newline
        }
        self.expect(&TokenKind::Catch)?;
        let catch_var = self.expect_ident()?;
        let handler = self.parse_block()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Try { body, catch_var, handler }, span })
    }

    fn parse_throw(&mut self, span: Span) -> Option<Stmt> {
        self.advance(); // throw
        let expr = self.parse_expr()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Throw(expr), span })
    }

    fn parse_expr_stmt(&mut self, span: Span) -> Option<Stmt> {
        let expr = self.parse_expr()?;
        self.consume_stmt_end();
        Some(Stmt { kind: StmtKind::Expr(expr), span })
    }

    // =========================================================================
    // 파라미터 목록
    // =========================================================================

    /// 파라미터 목록 — 각 파라미터는 선택적 `: type` 힌트와 `= 기본값`을 가질 수 있다.
    /// 기본값이 있는 매개변수 뒤에는 기본값 없는 매개변수가 올 수 없다.
    #[allow(clippy::type_complexity)]
    fn parse_params(&mut self) -> Option<(Vec<String>, Vec<Option<TypeAnn>>, Vec<Option<Expr>>)> {
        let mut params = Vec::new();
        let mut types = Vec::new();
        let mut defaults = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            let name_span = self.peek_span();
            let name = self.expect_ident()?;
            params.push(name);
            // 선택적 타입 힌트: `: type`
            if matches!(self.peek(), TokenKind::Colon) {
                self.advance(); // :
                types.push(Some(self.parse_type()?));
            } else {
                types.push(None);
            }
            // 선택적 기본값: `= expr`
            if matches!(self.peek(), TokenKind::Eq) {
                self.advance(); // =
                defaults.push(Some(self.parse_expr()?));
            } else {
                if defaults.iter().any(|d: &Option<Expr>| d.is_some()) {
                    self.error_at(name_span, "기본값이 있는 매개변수 뒤에는 기본값 없는 매개변수를 둘 수 없습니다");
                }
                defaults.push(None);
            }
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Some((params, types, defaults))
    }

    /// 타입 이름 파싱 (int/float/bool/str/nil/list/map/fn/any).
    fn parse_type(&mut self) -> Option<TypeAnn> {
        let span = self.peek_span();
        let name = self.expect_ident()?;
        match TypeAnn::from_name(&name) {
            Some(t) => Some(t),
            None => {
                self.error_at(span, &format!("알 수 없는 타입: '{name}'"));
                None
            }
        }
    }

    /// 선택적 반환 타입: `-> type`.
    fn parse_ret_type(&mut self) -> Option<Option<TypeAnn>> {
        if matches!(self.peek(), TokenKind::Arrow) {
            self.advance(); // ->
            Some(Some(self.parse_type()?))
        } else {
            Some(None)
        }
    }

    // =========================================================================
    // 표현식 파서 — Pratt
    // =========================================================================

    pub fn parse_expr(&mut self) -> Option<Expr> {
        // 깊이 가드: 깊은 '(' 중첩 등에서 스택 오버플로 방지
        if !self.enter_nest() {
            self.exit_nest();
            return None;
        }
        let r = self.parse_expr_bp(0);
        self.exit_nest();
        r
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
                        lhs = self.parse_index_or_slice(lhs)?;
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

            // `not in` (비교 수준 이항 연산자 — 두 토큰)
            if matches!(self.peek(), TokenKind::Not) && matches!(self.peek_n(1), TokenKind::In) {
                if 9 < min_bp {
                    break;
                }
                lhs = self.parse_not_in(lhs)?;
                continue;
            }

            // 이항 연산자
            let Some((l_bp, r_bp)) = infix_bp(self.peek()) else { break };
            if l_bp < min_bp {
                break;
            }

            let op_span = self.peek_span();
            let op_tok = self.advance().kind.clone();

            if let Some(op) = compound_assign_op(&op_tok) {
                lhs = self.parse_compound_assign(lhs, op, op_span, r_bp)?;
            } else if op_tok == TokenKind::Eq {
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

    // 아래 보조 함수들은 parse_expr_bp(깊이 재귀)의 스택 프레임을 작게 유지하려고 분리한다.

    /// `[` 다음: 인덱스 `a[i]` 또는 슬라이스 `a[s:e]` / `a[:e]` / `a[s:]` / `a[:]`
    #[inline(never)]
    fn parse_index_or_slice(&mut self, lhs: Expr) -> Option<Expr> {
        let span = lhs.span;
        self.advance(); // [
        let start = if matches!(self.peek(), TokenKind::Colon) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        if matches!(self.peek(), TokenKind::Colon) {
            self.advance(); // :
            let end = if matches!(self.peek(), TokenKind::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expr()?))
            };
            self.expect(&TokenKind::RBracket)?;
            return Some(Expr { kind: ExprKind::Slice { target: Box::new(lhs), start, end }, span });
        }
        self.expect(&TokenKind::RBracket)?;
        let index = start?;
        Some(Expr { kind: ExprKind::Index { target: Box::new(lhs), index }, span })
    }

    /// `lhs not in rhs`
    #[inline(never)]
    fn parse_not_in(&mut self, lhs: Expr) -> Option<Expr> {
        self.advance(); // not
        self.advance(); // in
        let rhs = self.parse_expr_bp(10)?;
        let span = lhs.span;
        Some(Expr {
            kind: ExprKind::Binary { op: BinaryOp::NotIn, left: Box::new(lhs), right: Box::new(rhs) },
            span,
        })
    }

    /// 복합 대입: x += e, v[i] -= e, m.f *= e
    #[inline(never)]
    fn parse_compound_assign(&mut self, lhs: Expr, op: BinaryOp, op_span: Span, r_bp: u8) -> Option<Expr> {
        if !is_assign_target(&lhs) {
            self.error_at(op_span, "대입 대상은 변수·인덱스·필드만 허용됩니다");
        }
        let value = self.parse_expr_bp(r_bp)?;
        Some(Expr {
            kind: ExprKind::CompoundAssign { op, target: Box::new(lhs), value: Box::new(value) },
            span: op_span,
        })
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
            TokenKind::If => self.parse_if_expr(span),
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
            TokenKind::FStr(parts) => {
                self.advance();
                self.parse_fstring(parts, span)
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

    /// f-문자열 조각들 → FString 식. 각 `{식}`의 소스를 따로 렉싱·파싱하고, 토큰 위치를
    /// 원본 파일 기준으로 옮겨 에러 위치가 정확하게 나오게 한다. 형식 지정자는 여기서 검증한다.
    #[inline(never)]
    fn parse_fstring(&mut self, parts: Vec<crate::lexer::token::FStrPart>, span: Span) -> Option<Expr> {
        use crate::lexer::token::FStrPart;
        let mut out = Vec::new();
        for p in parts {
            match p {
                FStrPart::Lit(s) => out.push(FStringPart::Lit(s)),
                FStrPart::Expr { src, line, col, spec } => {
                    let at = Span::new(line, col);
                    if let Some(sp) = &spec {
                        if let Err(m) = crate::stdlib::parse_format_spec(sp) {
                            self.error_at(at, &format!("f-문자열 형식 지정자 '{sp}': {m}"));
                            return None;
                        }
                    }
                    let mut tokens = match crate::lexer::Lexer::new(&src).tokenize() {
                        Ok(t) => t,
                        Err(errs) => {
                            let m = errs.first().map(|e| e.to_string()).unwrap_or_default();
                            self.error_at(at, &format!("f-문자열 식 오류: {m}"));
                            return None;
                        }
                    };
                    for t in tokens.iter_mut() {
                        if t.span.line == 1 {
                            t.span.col += col - 1;
                        }
                        t.span.line += line - 1;
                    }
                    let mut sub = Parser::new(tokens);
                    sub.depth = self.depth;
                    let e = sub.parse_expr();
                    while matches!(sub.peek(), TokenKind::Newline) {
                        sub.advance();
                    }
                    if !matches!(sub.peek(), TokenKind::Eof) {
                        let s = sub.peek_span();
                        sub.error_at(s, "f-문자열의 {} 안에는 식 하나만 쓸 수 있습니다");
                    }
                    let failed = !sub.errors.is_empty();
                    self.errors.extend(sub.errors);
                    match e {
                        Some(e) if !failed => out.push(FStringPart::Expr(Box::new(e), spec)),
                        _ => return None,
                    }
                }
            }
        }
        Some(Expr { kind: ExprKind::FString(out), span })
    }

    /// `if c { a } else { b }` 식 (Phase 47). else-if 체인 허용, else 필수.
    #[inline(never)]
    fn parse_if_expr(&mut self, span: Span) -> Option<Expr> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;
        if !matches!(self.peek(), TokenKind::Else) {
            self.error_at(span, "if 식에는 else 갈래가 필요합니다 (값을 내야 하므로)");
            return None;
        }
        self.advance(); // else
        let else_ = if matches!(self.peek(), TokenKind::If) {
            let inner_span = self.peek_span();
            let inner = self.parse_if_expr(inner_span)?;
            Block { stmts: vec![Stmt { kind: StmtKind::Expr(inner), span: inner_span }], span: inner_span }
        } else {
            self.parse_block()?
        };
        Some(Expr { kind: ExprKind::IfExpr { cond: Box::new(cond), then, else_ }, span })
    }

    fn parse_lambda(&mut self, span: Span) -> Option<Expr> {
        self.advance(); // fn
        self.expect(&TokenKind::LParen)?;
        let (params, param_types, param_defaults) = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let ret_type = self.parse_ret_type()?;
        let body = self.parse_block()?;
        Some(Expr { kind: ExprKind::Function {
            name: None, params, param_types, param_defaults, ret_type, body,
        }, span })
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
        TokenKind::Eq | TokenKind::PlusEq | TokenKind::MinusEq
        | TokenKind::StarEq | TokenKind::SlashEq
        | TokenKind::PercentEq                           => Some((2, 1)),   // 우결합
        TokenKind::Or                                    => Some((3, 4)),
        TokenKind::And                                   => Some((5, 6)),
        TokenKind::EqEq | TokenKind::BangEq              => Some((7, 8)),
        TokenKind::Lt | TokenKind::LtEq
        | TokenKind::Gt | TokenKind::GtEq | TokenKind::In => Some((9, 10)),
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
        TokenKind::In      => BinaryOp::In,
        _ => unreachable!("token_to_binary_op: {tok:?}"),
    }
}

/// 복합 대입 토큰 → 연산자 (`+=` → Add ...). 복합 대입이 아니면 None.
fn compound_assign_op(tok: &TokenKind) -> Option<BinaryOp> {
    match tok {
        TokenKind::PlusEq    => Some(BinaryOp::Add),
        TokenKind::MinusEq   => Some(BinaryOp::Sub),
        TokenKind::StarEq    => Some(BinaryOp::Mul),
        TokenKind::SlashEq   => Some(BinaryOp::Div),
        TokenKind::PercentEq => Some(BinaryOp::Mod),
        _ => None,
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

    fn try_parse(src: &str) -> Result<Program, Vec<ParseError>> {
        let toks = Lexer::new(src).tokenize().expect("렉싱 실패");
        Parser::new(toks).parse()
    }

    // 회귀: 파서 에러 복구의 진행 보장 (무한루프 종료성 버그).
    // 떠도는 '}' 가 무한루프 없이 파스 에러로 즉시 종료해야 한다.
    // (이 테스트가 끝까지 도는 것 자체가 무한루프가 없다는 증거)
    #[test]
    fn test_stray_rbrace_terminates_with_error() {
        assert!(try_parse("}\n").is_err(), "떠도는 '}}' 는 파스 에러여야 함");
        assert!(try_parse("let x = 1\n}\nlet y = 2\n").is_err());
        assert!(try_parse("}\nlet x = 1\n").is_err());
    }

    // 회귀: else-if 체인 뒤에 다음 문장이 와도 파스된다.
    // (이전엔 내부 if가 종결 줄바꿈을 소비해 외부가 "줄바꿈 기대" 오류를 냈다.)
    #[test]
    fn test_else_if_chain_then_next_statement() {
        let src = "\
let n = 5
if n < 0 {
    print(\"neg\")
} else if n == 0 {
    print(\"zero\")
} else {
    print(\"pos\")
}
print(\"next\")
";
        let prog = parse(src);
        assert_eq!(prog.stmts.len(), 3, "let, if, print 3개 문");
        // 단일 라인 else-if 도
        assert!(try_parse("if 1<0 {print(1)} else if 1>0 {print(2)} else {print(3)}\nprint(4)\n").is_ok());
    }

    // 회귀: 아주 깊은 괄호/블록 중첩이 스택 오버플로(abort) 대신 파스 에러로 종료.
    #[test]
    fn test_deep_nesting_errors_not_crash() {
        let deep_parens = format!("let x = {}1{}", "(".repeat(5000), ")".repeat(5000));
        assert!(try_parse(&deep_parens).is_err(), "깊은 괄호 → 파스 에러");
        let deep_braces = format!("fn f() {}{}", "{".repeat(5000), "}".repeat(5000));
        assert!(try_parse(&deep_braces).is_err(), "깊은 블록 → 파스 에러");
        // 정상 깊이는 통과
        let ok = format!("let x = {}1{}", "(".repeat(100), ")".repeat(100));
        assert!(try_parse(&ok).is_ok(), "100겹은 정상");
    }

    // 회귀: 함수 인자로 넘기는 다중문 클로저(본문에 let)가 파싱돼야 한다.
    // 괄호 안 '{ }' 블록에서 줄바꿈이 유효해야 함 (렉서 브래킷 스택).
    #[test]
    fn test_multiline_closure_as_call_argument() {
        let prog = parse("register(\"/x\", fn(req) {\n    let d = req\n    return d\n})\n");
        assert_eq!(prog.stmts.len(), 1, "호출식 문 1개");
    }

    // 회귀: 다중라인 맵 리터럴(인자)도 여전히 파싱돼야 한다 (렉서 변경 부작용 없음).
    #[test]
    fn test_multiline_map_literal_as_argument() {
        let prog = parse("f({\n    \"a\": 1,\n    \"b\": 2\n})\n");
        assert_eq!(prog.stmts.len(), 1);
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
            StmtKind::Let { name, value, .. } => {
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
            StmtKind::Let { name, value, .. } => {
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
            StmtKind::For { vars, .. } => assert_eq!(vars, &vec!["item".to_string()]),
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
