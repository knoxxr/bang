// Bang — Phase 4: Resolver (정적 분석 패스)
//
// 실행 전에 수행하는 이름 해석 및 구조적 검사.
// - 변수를 (depth, slot) 쌍으로 해석 → Phase 5 VM 슬롯 준비
// - 미정의 변수, 중복 let, 초기화 전 자기 참조, return 위치 오류 검출
// - 도달 불가 코드 / 미사용 변수 경고 (lint)

use std::collections::HashMap;
use std::fmt;

use crate::ast::{Block, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::lexer::token::Span;

// ============================================================================
// Public 타입
// ============================================================================

#[derive(Debug, Clone)]
pub struct ResolveError {
    pub message: String,
    pub span: Span,
}

impl ResolveError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self { message: message.into(), span }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}:{}] 리졸브 오류: {}", self.span.line, self.span.col, self.message)
    }
}

#[derive(Debug, Clone)]
pub struct ResolveWarning {
    pub message: String,
    pub span: Span,
}

impl ResolveWarning {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self { message: message.into(), span }
    }
}

impl fmt::Display for ResolveWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}:{}] 경고: {}", self.span.line, self.span.col, self.message)
    }
}

/// 변수 참조의 렉시컬 위치.
/// depth = 0: 현재 스코프, 1: 부모, ...
/// slot: 해당 스코프 내 선언 순서 번호
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarRef {
    pub depth: usize,
    pub slot: usize,
}

/// 이름 해석 결과 테이블
#[derive(Debug, Default)]
pub struct ResolveTable {
    /// Ident 사용 위치(Span) → VarRef
    pub ident_refs: HashMap<Span, VarRef>,
    /// Let 선언 위치(Stmt.span) → slot 번호
    pub let_slots: HashMap<Span, usize>,
}

#[derive(Debug)]
pub struct ResolveResult {
    pub table: ResolveTable,
    pub errors: Vec<ResolveError>,
    pub warnings: Vec<ResolveWarning>,
}

// ============================================================================
// 내부 타입
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarState {
    /// `let name = <init>` — 선언됐으나 초기화식 평가 전 (two-phase let)
    Declared,
    /// 초기화 완료, 참조 가능
    Defined,
}

#[derive(Debug, Clone)]
struct VarInfo {
    slot: usize,
    state: VarState,
    used: bool,
    span: Span,
    /// fn 리터럴 직접 바인딩이면 파라미터 수
    arity: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    /// 내장 함수 전용 프레임 — 사용자 전역 스코프 아래에 위치
    Builtin,
    Global,
    Block,
    Function,
    Parallel,
}

#[derive(Debug)]
struct ScopeFrame {
    kind: FrameKind,
    vars: Vec<(String, VarInfo)>,
}

impl ScopeFrame {
    fn new(kind: FrameKind) -> Self {
        Self { kind, vars: Vec::new() }
    }

    fn find(&self, name: &str) -> Option<&VarInfo> {
        self.vars.iter().rev().find(|(n, _)| n == name).map(|(_, i)| i)
    }

    fn find_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        self.vars.iter_mut().rev().find(|(n, _)| n == name).map(|(_, i)| i)
    }
}

// ============================================================================
// Resolver
// ============================================================================

struct Resolver {
    scopes: Vec<ScopeFrame>,
    /// 현재 함수 중첩 깊이. 0 = 전역.
    fn_depth: usize,
    /// spawn 식 직계 하위에 있으면 true
    in_spawn_direct: bool,
    /// spawn 진입 시점의 scopes.len() — 이 인덱스 미만 프레임에 Assign = 오류
    spawn_scope_depth: usize,
    errors: Vec<ResolveError>,
    warnings: Vec<ResolveWarning>,
    table: ResolveTable,
    /// 전역에서 본문 해석 전에 미리 선언된 함수 이름들(전방 참조·상호 재귀 허용).
    /// 이 이름의 Let은 본문 해석 시 재선언을 건너뛴다.
    global_prescan: std::collections::HashSet<String>,
}

impl Resolver {
    fn new() -> Self {
        Self {
            scopes: Vec::new(),
            fn_depth: 0,
            in_spawn_direct: false,
            spawn_scope_depth: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            table: ResolveTable::default(),
            global_prescan: std::collections::HashSet::new(),
        }
    }

    // --- 스코프 관리 ---

    fn push_scope(&mut self, kind: FrameKind) {
        self.scopes.push(ScopeFrame::new(kind));
    }

    /// 스코프 종료. warn_unused=true 이면 `_` 로 시작하지 않는 미사용 변수 경고.
    fn pop_scope(&mut self, warn_unused: bool) {
        if let Some(frame) = self.scopes.pop() {
            if warn_unused
                && frame.kind != FrameKind::Global
                && frame.kind != FrameKind::Builtin
            {
                for (name, info) in &frame.vars {
                    if !info.used && !name.starts_with('_') {
                        self.warnings.push(ResolveWarning::new(
                            format!("사용되지 않은 변수: '{name}'"),
                            info.span,
                        ));
                    }
                }
            }
        }
    }

    /// 현재 스코프에 변수 선언(Declared 상태). 중복이면 오류 후 None.
    fn declare(&mut self, name: &str, span: Span) -> Option<usize> {
        let frame = self.scopes.last_mut().expect("스코프 스택이 비어있음");
        if frame.find(name).is_some() {
            self.errors.push(ResolveError::new(
                format!("'{name}' 이(가) 같은 스코프에서 이미 선언됨"),
                span,
            ));
            return None;
        }
        let slot = frame.vars.len();
        frame.vars.push((name.to_string(), VarInfo {
            slot,
            state: VarState::Declared,
            used: false,
            span,
            arity: None,
        }));
        Some(slot)
    }

    /// 현재 스코프의 변수를 Defined 상태로 전환.
    fn define(&mut self, name: &str) {
        if let Some(frame) = self.scopes.last_mut() {
            if let Some(info) = frame.find_mut(name) {
                info.state = VarState::Defined;
            }
        }
    }

    /// 현재 스코프의 변수에 arity 저장.
    fn set_arity(&mut self, name: &str, arity: usize) {
        if let Some(frame) = self.scopes.last_mut() {
            if let Some(info) = frame.find_mut(name) {
                info.arity = Some(arity);
            }
        }
    }

    /// 이름을 스코프 스택에서 검색.
    /// 반환: (depth, slot, frame_abs_index)
    ///   depth: 현재 스코프로부터의 거리 (0=현재)
    ///   frame_abs_index: self.scopes 내 절대 인덱스
    fn lookup_raw(&self, name: &str) -> Option<(usize, usize, usize)> {
        let top = self.scopes.len();
        for (rev_idx, frame) in self.scopes.iter().rev().enumerate() {
            if let Some(info) = frame.find(name) {
                let frame_abs = top - 1 - rev_idx;
                return Some((rev_idx, info.slot, frame_abs));
            }
        }
        None
    }

    /// 스코프 체인에서 해당 이름의 used 플래그를 true로 설정.
    fn mark_used(&mut self, name: &str) {
        for frame in self.scopes.iter_mut().rev() {
            if let Some(info) = frame.find_mut(name) {
                info.used = true;
                return;
            }
        }
    }

    // --- 해석 진입점 ---

    fn resolve_program(&mut self, prog: &Program) {
        // 내장 함수 스코프(아래) + 사용자 전역 스코프(위)를 분리
        // 사용자가 let keys = [...] 처럼 내장 이름을 재선언해도 충돌 없음
        self.push_scope(FrameKind::Builtin);
        self.register_builtins();
        self.push_scope(FrameKind::Global);
        // 전방 참조·상호 재귀 허용: 최상위 "함수" 바인딩을 본문 해석 전에 미리 선언한다.
        // (값 바인딩은 제외 — 초기화 순서상 런타임에 Nil을 읽는 함정을 피하려고 함수만.)
        for stmt in &prog.stmts {
            if let StmtKind::Let { name, value, .. } = &stmt.kind {
                // arity는 파라미터 수로 직접 계산한다 — 본문을 여기서 해석하면
                // 아직 prescan 안 된 이름을 못 찾으므로, 본문 해석은 아래 resolve_stmts로 미룬다.
                if let ExprKind::Function { params, .. } = &value.kind {
                    if let Some(slot) = self.declare(name, stmt.span) {
                        self.define(name);
                        self.table.let_slots.insert(stmt.span, slot);
                        self.set_arity(name, params.len());
                        self.global_prescan.insert(name.clone());
                    }
                }
            }
        }
        self.resolve_stmts(&prog.stmts);
        self.pop_scope(false); // Global: 미사용 경고 없음
        self.pop_scope(false); // Builtin: 미사용 경고 없음
    }

    /// 내장 함수들을 전역 스코프에 등록 (단일 테이블 crate::stdlib::BUILTINS에서 이름·인자 수).
    fn register_builtins(&mut self) {
        let frame = self.scopes.last_mut().unwrap();
        for b in crate::stdlib::BUILTINS {
            let slot = frame.vars.len();
            frame.vars.push((b.name.to_string(), VarInfo {
                slot,
                state: VarState::Defined,
                used: true,
                span: Span::new(0, 0),
                arity: b.arity,
            }));
        }
    }

    /// 문장 목록 해석. true = 이 경로가 항상 return.
    fn resolve_stmts(&mut self, stmts: &[Stmt]) -> bool {
        let mut always_returns = false;
        for stmt in stmts {
            if always_returns {
                self.warnings.push(ResolveWarning::new(
                    "return 이후 도달 불가 코드",
                    stmt.span,
                ));
            }
            let returns = self.resolve_stmt(stmt);
            if returns {
                always_returns = true;
            }
        }
        always_returns
    }

    /// 블록을 새 Block 스코프로 래핑해 해석. true = 항상 return.
    fn resolve_block(&mut self, block: &Block) -> bool {
        self.push_scope(FrameKind::Block);
        let r = self.resolve_stmts(&block.stmts);
        self.pop_scope(true);
        r
    }

    /// 문장 해석. true = 이 문장이 반드시 return.
    fn resolve_stmt(&mut self, stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                // 전역에서 이미 prescan으로 선언·정의된 함수 이름은 재선언을 건너뛰고
                // 본문만 해석한다(전방 참조·상호 재귀 허용).
                if self.fn_depth == 0 && self.global_prescan.contains(name) {
                    self.resolve_expr(value);
                    return false;
                }
                // two-phase: declare → 초기화식 해석 → define
                let slot = self.declare(name, stmt.span);
                let arity = self.resolve_expr_arity(value);
                if let Some(s) = slot {
                    self.define(name);
                    self.table.let_slots.insert(stmt.span, s);
                    if let Some(a) = arity {
                        self.set_arity(name, a);
                    }
                }
                false
            }

            StmtKind::Expr(expr) => {
                self.resolve_expr(expr);
                false
            }

            StmtKind::Return(val) => {
                if self.fn_depth == 0 {
                    self.errors.push(ResolveError::new(
                        "함수 외부에서 return 사용",
                        stmt.span,
                    ));
                }
                if let Some(v) = val {
                    self.resolve_expr(v);
                }
                true
            }

            StmtKind::If { cond, then, else_ } => {
                self.resolve_expr(cond);
                let then_ret = self.resolve_block(then);
                let else_ret = match else_ {
                    Some(el) => self.resolve_block(el),
                    None => false,
                };
                then_ret && else_ret
            }

            StmtKind::While { cond, body } => {
                self.resolve_expr(cond);
                self.resolve_block(body);
                false
            }

            StmtKind::For { vars, iter, body } => {
                self.resolve_expr(iter);
                // 루프 변수(들)를 body와 같은 새 스코프에 선언
                self.push_scope(FrameKind::Block);
                for var in vars {
                    let slot = self.scopes.last().map(|f| f.vars.len()).unwrap_or(0);
                    if let Some(frame) = self.scopes.last_mut() {
                        frame.vars.push((var.clone(), VarInfo {
                            slot,
                            state: VarState::Defined,
                            used: false,
                            span: stmt.span,
                            arity: None,
                        }));
                    }
                }
                self.resolve_stmts(&body.stmts);
                self.pop_scope(true);
                false
            }

            StmtKind::Block(block) => self.resolve_block(block),

            StmtKind::Parallel(block) => {
                self.push_scope(FrameKind::Parallel);
                let r = self.resolve_stmts(&block.stmts);
                self.pop_scope(true);
                r
            }

            StmtKind::Break | StmtKind::Continue => false,

            StmtKind::Try { body, catch_var, handler } => {
                // try 본문은 자체 스코프
                self.resolve_block(body);
                // catch 변수는 handler와 같은 새 스코프에 선언
                self.push_scope(FrameKind::Block);
                let slot = self.scopes.last().map(|f| f.vars.len()).unwrap_or(0);
                self.scopes.last_mut().unwrap().vars.push((catch_var.clone(), VarInfo {
                    slot,
                    state: VarState::Defined,
                    // catch 변수는 미사용이어도 경고하지 않음 (관례)
                    used: true,
                    span: stmt.span,
                    arity: None,
                }));
                self.resolve_stmts(&handler.stmts);
                self.pop_scope(true);
                false
            }

            StmtKind::Throw(expr) => {
                self.resolve_expr(expr);
                false
            }
        }
    }

    /// 식 해석 (arity 반환 없음).
    fn resolve_expr(&mut self, expr: &Expr) {
        self.resolve_expr_arity(expr);
    }

    /// 식 해석. fn 리터럴이면 Some(arity), 그 외 None.
    fn resolve_expr_arity(&mut self, expr: &Expr) -> Option<usize> {
        match &expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Nil => None,

            ExprKind::Ident(name) => self.resolve_ident(name, expr.span),

            ExprKind::List(items) => {
                for item in items {
                    self.resolve_expr(item);
                }
                None
            }

            ExprKind::Map(entries) => {
                for (_, val) in entries {
                    self.resolve_expr(val);
                }
                None
            }

            ExprKind::Unary { expr: inner, .. } => {
                self.resolve_expr(inner);
                None
            }

            ExprKind::Binary { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
                None
            }

            ExprKind::Call { callee, args } => {
                for arg in args {
                    self.resolve_expr(arg);
                }
                let callee_arity = self.resolve_expr_arity(callee);

                // 리터럴 비함수 호출 검사
                match &callee.kind {
                    ExprKind::Int(_)
                    | ExprKind::Float(_)
                    | ExprKind::Bool(_)
                    | ExprKind::Str(_)
                    | ExprKind::Nil
                    | ExprKind::List(_)
                    | ExprKind::Map(_) => {
                        self.errors.push(ResolveError::new(
                            "함수가 아닌 값을 호출했습니다",
                            expr.span,
                        ));
                    }
                    _ => {}
                }

                // 인자 수 검사 (arity 알고 있을 때만)
                if let Some(arity) = callee_arity {
                    if args.len() != arity {
                        self.errors.push(ResolveError::new(
                            format!(
                                "인자 수 불일치: 함수는 {arity}개를 필요로 하지만 {}개가 제공됨",
                                args.len()
                            ),
                            expr.span,
                        ));
                    }
                }
                None
            }

            ExprKind::Index { target, index } => {
                self.resolve_expr(target);
                self.resolve_expr(index);
                None
            }

            ExprKind::Field { target, .. } => {
                self.resolve_expr(target);
                None
            }

            ExprKind::Slice { target, start, end } => {
                self.resolve_expr(target);
                for e in [start, end].into_iter().flatten() {
                    self.resolve_expr(e);
                }
                None
            }

            ExprKind::Function { name, params, body, .. } => {
                let arity = params.len();
                self.push_scope(FrameKind::Function);
                self.fn_depth += 1;

                // 함수 경계 진입 시 spawn 컨텍스트 초기화
                let saved_spawn = self.in_spawn_direct;
                let saved_spawn_depth = self.spawn_scope_depth;
                self.in_spawn_direct = false;

                // 파라미터 등록 (Defined 상태)
                let param_span = body.span;
                for param in params {
                    let slot = self.scopes.last().map(|f| f.vars.len()).unwrap_or(0);
                    self.scopes.last_mut().unwrap().vars.push((param.clone(), VarInfo {
                        slot,
                        state: VarState::Defined,
                        used: false,
                        span: param_span,
                        arity: None,
                    }));
                }

                // named fn: 외부 스코프에서 해당 이름을 used로 표시
                // 재귀 참조는 depth≥1 에서 발생하므로 자기 참조 오류 없음
                if let Some(fname) = name {
                    let frames = self.scopes.len();
                    if frames >= 2 {
                        let parent_idx = frames - 2;
                        if let Some(info) = self.scopes[parent_idx].find_mut(fname) {
                            info.used = true;
                        }
                    }
                }

                self.resolve_stmts(&body.stmts);

                self.in_spawn_direct = saved_spawn;
                self.spawn_scope_depth = saved_spawn_depth;
                self.fn_depth -= 1;
                self.pop_scope(true);
                Some(arity)
            }

            ExprKind::Spawn(inner) => {
                let saved_spawn = self.in_spawn_direct;
                let saved_spawn_depth = self.spawn_scope_depth;
                self.in_spawn_direct = true;
                self.spawn_scope_depth = self.scopes.len();
                self.resolve_expr(inner);
                self.in_spawn_direct = saved_spawn;
                self.spawn_scope_depth = saved_spawn_depth;
                None
            }

            ExprKind::Assign { target, value } => {
                self.resolve_expr(value);
                self.resolve_assign_target(target);
                None
            }

            ExprKind::CompoundAssign { target, value, .. } => {
                self.resolve_expr(value);
                self.resolve_assign_target(target);
                None
            }
        }
    }

    /// Ident 조회 + VarRef 기록.
    fn resolve_ident(&mut self, name: &str, span: Span) -> Option<usize> {
        match self.lookup_raw(name) {
            None => {
                self.errors.push(ResolveError::new(
                    format!("정의되지 않은 변수: '{name}'"),
                    span,
                ));
                None
            }
            Some((depth, slot, frame_abs)) => {
                let state = self.scopes[frame_abs].find(name).map(|i| i.state);
                let arity = self.scopes[frame_abs].find(name).and_then(|i| i.arity);

                // 자기 참조: 같은 스코프, Declared 상태 → 초기화 전 참조
                if state == Some(VarState::Declared) && depth == 0 {
                    self.errors.push(ResolveError::new(
                        format!("'{name}' 은(는) 초기화 전에 참조됩니다"),
                        span,
                    ));
                    return None;
                }

                self.mark_used(name);
                self.table.ident_refs.insert(span, VarRef { depth, slot });
                arity
            }
        }
    }

    /// 대입 대상(Ident / Index / Field) 해석. spawn 직계 시 외부 변수 대입 검사.
    fn resolve_assign_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Ident(name) => {
                match self.lookup_raw(name) {
                    None => {
                        self.errors.push(ResolveError::new(
                            format!("정의되지 않은 변수: '{name}'"),
                            target.span,
                        ));
                    }
                    Some((depth, slot, frame_abs)) => {
                        if self.in_spawn_direct && frame_abs < self.spawn_scope_depth {
                            self.errors.push(ResolveError::new(
                                format!(
                                    "spawn 식 안에서 바깥 변수 '{name}'에 직접 대입할 수 없습니다"
                                ),
                                target.span,
                            ));
                        }
                        self.mark_used(name);
                        self.table.ident_refs.insert(target.span, VarRef { depth, slot });
                    }
                }
            }
            ExprKind::Index { target: inner, index } => {
                self.resolve_assign_target(inner);
                self.resolve_expr(index);
            }
            ExprKind::Field { target: inner, .. } => {
                self.resolve_assign_target(inner);
            }
            _ => {
                self.errors.push(ResolveError::new(
                    "대입 대상이 유효하지 않습니다",
                    target.span,
                ));
            }
        }
    }
}

// ============================================================================
// 공개 진입점
// ============================================================================

/// AST 전체를 정적 분석한다.
/// 오류가 있으면 result.errors 에 모아 반환한다 (첫 오류에서 중단하지 않음).
pub fn resolve(prog: &Program) -> ResolveResult {
    let mut resolver = Resolver::new();
    resolver.resolve_program(prog);
    ResolveResult {
        table: resolver.table,
        errors: resolver.errors,
        warnings: resolver.warnings,
    }
}
