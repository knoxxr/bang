// Bang — Phase 5: AST → Bytecode Compiler

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::*;
use crate::lexer::token::Span;
use crate::vm::{builtin_index, Chunk, CompiledFn, VmValue};

// ============================================================================
// Opcode constants (imported by vm.rs)
// ============================================================================

pub const OP_POP:          u8 = 0;
pub const OP_NIL:          u8 = 1;
pub const OP_TRUE:         u8 = 2;
pub const OP_FALSE:        u8 = 3;
pub const OP_CONST:        u8 = 4;
pub const OP_DUP:          u8 = 5;

pub const OP_ADD:          u8 = 10;
pub const OP_SUB:          u8 = 11;
pub const OP_MUL:          u8 = 12;
pub const OP_DIV:          u8 = 13;
pub const OP_MOD:          u8 = 14;
pub const OP_NEG:          u8 = 15;

pub const OP_EQ:           u8 = 20;
pub const OP_NE:           u8 = 21;
pub const OP_LT:           u8 = 22;
pub const OP_LE:           u8 = 23;
pub const OP_GT:           u8 = 24;
pub const OP_GE:           u8 = 25;
pub const OP_NOT:          u8 = 26;

pub const OP_JUMP:         u8 = 30;
pub const OP_JUMP_FALSE:   u8 = 31;
pub const OP_JUMP_AND:     u8 = 32;
pub const OP_JUMP_OR:      u8 = 33;

pub const OP_LOAD_LOCAL:   u8 = 40;
pub const OP_STORE_LOCAL:  u8 = 41;
pub const OP_LOAD_UPVAL:   u8 = 42;
pub const OP_STORE_UPVAL:  u8 = 43;
pub const OP_LOAD_GLOBAL:  u8 = 44;
pub const OP_STORE_GLOBAL: u8 = 45;
pub const OP_LOAD_BUILTIN: u8 = 46;

pub const OP_CLOSURE:      u8 = 50;
pub const OP_CALL:         u8 = 51;
pub const OP_RETURN:       u8 = 52;

pub const OP_MAKE_LIST:    u8 = 60;
pub const OP_MAKE_MAP:     u8 = 61;
pub const OP_INDEX_GET:    u8 = 62;
pub const OP_INDEX_SET:    u8 = 63;
pub const OP_FIELD_GET:    u8 = 64;

pub const OP_MAKE_ITER:       u8 = 70;
pub const OP_FOR_ITER:        u8 = 71;
pub const OP_SPAWN:           u8 = 72;
pub const OP_PARALLEL_ENTER:  u8 = 73;
pub const OP_PARALLEL_EXIT:   u8 = 74;
pub const OP_SETUP_TRY:       u8 = 75; // operand: u16 절대 catch ip
pub const OP_POP_TRY:         u8 = 76; // try 본문 정상 종료 시 핸들러 제거
pub const OP_THROW:           u8 = 77; // 스택 top 값을 예외로 던짐
pub const OP_CHECK_TYPE:      u8 = 78; // operand: u8 타입 태그. 스택 top 타입 검증(peek)
/// 경로 대입 `v[k1][k2].f = val` (Phase 45). operand: u16 slot, u8 kind(0=local,1=upval,2=global), u8 depth,
/// u8 binop(0=단순 대입, 그 외 OP_ADD..OP_MOD = 복합 대입 `v[k] op= val`, Phase 46).
/// 스택: [root 복사본, k1..kN, val] → [val]. 변수 슬롯을 제자리 변경해 COW 복사를 피한다.
pub const OP_SET_PATH:        u8 = 79;
/// 갱신 대입 `x = push(x, e)` / `x = x + e` (Phase 45). operand: u16 slot, u8 kind, u8 op(0=push,1=add).
/// 스택: [x 복사본, e] → [새 x]. 슬롯이 같은 값이면 제자리 변경.
pub const OP_UPDATE_VAR:      u8 = 80;
/// 슬라이스 `t[s:e]` (Phase 46). 스택: [t, s|nil, e|nil] → [부분]. 음수는 끝에서부터, 범위는 잘라냄.
pub const OP_SLICE:           u8 = 81;
/// `for a, b in x` 용 반복자 (Phase 46): 맵은 [키, 값] 쌍, 그 외는 OP_MAKE_ITER와 같다.
pub const OP_MAKE_ITER_PAIRS: u8 = 82;
/// 포함 검사 `x in c` (Phase 46). 스택: [x, c] → [bool].
pub const OP_IN:              u8 = 83;
/// 분해 (Phase 46). operand: u8 n. 스택: [list(길이 n)] → [e0, .., e(n-1)].
pub const OP_UNPACK:          u8 = 84;
/// 기본값 매개변수 (Phase 47). operand: u8 매개변수 인덱스, i16 오프셋.
/// 호출 시 그 인자가 전달됐으면 점프(기본값 계산 건너뜀).
pub const OP_JUMP_IF_ARG:     u8 = 85;
/// 메서드 호출 `x.f(args)` (Phase 47). operand: u16 이름 상수, u8 argc, u8 discarded(문장 위치면 1).
/// 스택: [x, args..] → [결과]. x가 맵이고 키 f가 있으면 그 값을 호출, 아니면 빌트인 f(x, args..).
pub const OP_METHOD_CALL:     u8 = 86;
/// 형식 지정 (Phase 50). operand: u16 형식 문자열 상수. 스택 top 값 → 형식화된 Str.
pub const OP_FMT:             u8 = 87;
/// 문자열 조립 (Phase 50). operand: u8 n. 스택의 n개 값을 표시 형식(str())으로 이어 붙인 Str.
pub const OP_BUILD_STR:       u8 = 88;

// ============================================================================
// Compile error
// ============================================================================

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
}

impl CompileError {
    fn new(msg: impl Into<String>, span: Span) -> Self {
        Self { message: msg.into(), span }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] 컴파일 오류: {}", self.span.line, self.span.col, self.message)
    }
}

// ============================================================================
// Internal compiler types
// ============================================================================

#[derive(Debug)]
struct Local {
    name: String,
    scope_depth: usize, // nesting level within the function
    slot: usize,
}

#[derive(Debug, Clone, Copy)]
struct UpvalueDesc {
    is_local: bool, // true = from immediately enclosing fn's locals
    index: u8,      // if is_local: local slot; else: upvalue index in enclosing fn
}

struct LoopInfo {
    loop_start: usize,       // target for continue / for-loop top
    is_for:     bool,        // for-loops have an iterator on expression stack
    break_patches: Vec<usize>,
    try_depth_at_entry: usize, // break/continue가 빠져나가야 할 try 핸들러 수 계산용
}

struct FnCompiler {
    chunk: Chunk,
    name: Option<String>,
    arity: usize,
    /// 기본값 없는(필수) 매개변수 수 (Phase 47)
    min_arity: usize,
    file: Option<Arc<str>>,
    locals: Vec<Local>,
    upvalues: Vec<UpvalueDesc>,
    loops: Vec<LoopInfo>,
    scope_depth: usize,
    local_peak: usize, // maximum locals used (= local_count for CompiledFn)
    try_depth: usize,  // 현재 활성화된 try 핸들러(OP_SETUP_TRY) 수
    ret_type: Option<TypeAnn>, // 반환 타입 힌트 (return 문에서 검증)
}

impl FnCompiler {
    fn new(name: Option<String>, arity: usize) -> Self {
        Self {
            chunk: Chunk::new(),
            name,
            arity,
            min_arity: arity,
            file: None,
            locals: Vec::new(),
            upvalues: Vec::new(),
            loops: Vec::new(),
            scope_depth: 0,
            local_peak: 0,
            try_depth: 0,
            ret_type: None,
        }
    }

    fn emit(&mut self, byte: u8, span: Span) -> usize {
        self.chunk.emit(byte, span)
    }

    fn emit_u8(&mut self, op: u8, operand: u8, span: Span) {
        self.chunk.emit_u8(op, operand, span);
    }

    fn emit_u16(&mut self, op: u8, operand: u16, span: Span) {
        self.chunk.emit_u16(op, operand, span);
    }

    fn emit_jump(&mut self, op: u8, span: Span) -> usize {
        self.chunk.emit_jump(op, span)
    }

    fn patch_jump(&mut self, pos: usize) {
        self.chunk.patch_jump(pos);
    }

    fn current_pos(&self) -> usize {
        self.chunk.current_pos()
    }

    fn add_constant(&mut self, val: VmValue) -> u16 {
        self.chunk.add_constant(val)
    }

    fn begin_scope(&mut self) { self.scope_depth += 1; }

    fn end_scope(&mut self) {
        self.locals.retain(|l| l.scope_depth < self.scope_depth);
        self.scope_depth -= 1;
    }

    fn declare_local(&mut self, name: &str) -> u8 {
        let slot = self.locals.len();
        self.locals.push(Local {
            name: name.to_string(),
            scope_depth: self.scope_depth,
            slot,
        });
        if slot + 1 > self.local_peak { self.local_peak = slot + 1; }
        slot as u8
    }

    fn find_local(&self, name: &str) -> Option<u8> {
        self.locals.iter().rev()
            .find(|l| l.name == name)
            .map(|l| l.slot as u8)
    }

    fn into_compiled(self) -> Arc<CompiledFn> {
        Arc::new(CompiledFn {
            name: self.name,
            arity: self.arity,
            min_arity: self.min_arity,
            file: self.file,
            chunk: self.chunk,
            upvalue_count: self.upvalues.len(),
            local_count: self.local_peak,
        })
    }
}

// ============================================================================
// Compiler
// ============================================================================

struct Compiler {
    /// 소스 파일 경로 (Phase 56) — import 모듈이면 그 경로, 메인 프로그램이면 None
    file: Option<Arc<str>>,
    fn_stack: Vec<FnCompiler>,
    globals: HashMap<String, u16>, // top-level name → global slot index
    global_count: u16,
    errors: Vec<CompileError>,
}

impl Compiler {
    fn new() -> Self {
        Self {
            file: None,
            fn_stack: Vec::new(),
            globals: HashMap::new(),
            global_count: 0,
            errors: Vec::new(),
        }
    }

    fn error(&mut self, msg: impl Into<String>, span: Span) {
        self.errors.push(CompileError::new(msg, span));
    }

    // ---- current function helpers ----

    fn cur(&self) -> &FnCompiler {
        self.fn_stack.last().expect("fn_stack empty")
    }

    fn cur_mut(&mut self) -> &mut FnCompiler {
        self.fn_stack.last_mut().expect("fn_stack empty")
    }

    fn emit(&mut self, byte: u8, span: Span) -> usize {
        self.cur_mut().emit(byte, span)
    }

    fn emit_u8(&mut self, op: u8, operand: u8, span: Span) {
        self.cur_mut().emit_u8(op, operand, span);
    }

    fn emit_u16(&mut self, op: u8, operand: u16, span: Span) {
        self.cur_mut().emit_u16(op, operand, span);
    }

    fn emit_jump(&mut self, op: u8, span: Span) -> usize {
        self.cur_mut().emit_jump(op, span)
    }

    fn patch_jump(&mut self, pos: usize) {
        self.cur_mut().patch_jump(pos);
    }

    fn current_pos(&self) -> usize { self.cur().current_pos() }

    // ---- constant pool ----

    fn add_const(&mut self, val: VmValue) -> u16 {
        self.cur_mut().add_constant(val)
    }

    fn emit_const(&mut self, val: VmValue, span: Span) {
        let idx = self.add_const(val);
        self.emit_u16(OP_CONST, idx, span); // 상수 인덱스는 u16 (256개 초과 지원)
    }

    // ---- name resolution ----

    /// Resolve `name` to a VarLocation for the current function.
    fn resolve_name(&mut self, name: &str) -> VarLocation {
        let fn_idx = self.fn_stack.len() - 1;

        // 1. Current function locals
        if let Some(slot) = self.fn_stack[fn_idx].find_local(name) {
            return VarLocation::Local(slot);
        }

        // 2. Upvalue (search enclosing functions)
        if fn_idx > 0 {
            if let Some(uv_idx) = self.resolve_upvalue(fn_idx, name) {
                return VarLocation::Upval(uv_idx);
            }
        }

        // 3. Global
        if let Some(&slot) = self.globals.get(name) {
            return VarLocation::Global(slot);
        }

        // 4. Builtin
        if let Some(idx) = builtin_index(name) {
            return VarLocation::Builtin(idx as u8);
        }

        VarLocation::Unknown
    }

    /// Recursively find and create upvalue chain.
    /// Returns upvalue index in fn_stack[fn_idx], or None.
    fn resolve_upvalue(&mut self, fn_idx: usize, name: &str) -> Option<u8> {
        if fn_idx == 0 { return None; }

        // Is it a local in the immediately enclosing function?
        let local_slot = self.fn_stack[fn_idx - 1].find_local(name);
        if let Some(slot) = local_slot {
            return Some(self.add_upvalue(fn_idx, true, slot));
        }

        // Is it an upvalue in the enclosing function?
        let uv_in_enclosing = self.resolve_upvalue(fn_idx - 1, name)?;
        Some(self.add_upvalue(fn_idx, false, uv_in_enclosing))
    }

    fn add_upvalue(&mut self, fn_idx: usize, is_local: bool, index: u8) -> u8 {
        let fns = &mut self.fn_stack[fn_idx];
        for (i, uv) in fns.upvalues.iter().enumerate() {
            if uv.is_local == is_local && uv.index == index {
                return i as u8;
            }
        }
        let i = fns.upvalues.len() as u8;
        fns.upvalues.push(UpvalueDesc { is_local, index });
        i
    }

    // ---- LOAD / STORE helpers ----

    fn emit_load(&mut self, loc: VarLocation, span: Span) {
        match loc {
            VarLocation::Local(s)   => self.emit_u8(OP_LOAD_LOCAL, s, span),
            VarLocation::Upval(i)   => self.emit_u8(OP_LOAD_UPVAL, i, span),
            VarLocation::Global(s)  => self.emit_u16(OP_LOAD_GLOBAL, s, span),
            VarLocation::Builtin(i) => self.emit_u8(OP_LOAD_BUILTIN, i, span),
            VarLocation::Unknown    => {
                // 정상 경로는 resolver가 먼저 걸러 도달하지 않는다.
                // REPL(resolver 생략) 경로에서 오타가 조용히 nil이 되지 않게 에러로.
                self.error("정의되지 않은 변수", span);
                self.emit(OP_NIL, span);
            }
        }
    }

    fn emit_store(&mut self, loc: VarLocation, span: Span) {
        match loc {
            VarLocation::Local(s)  => self.emit_u8(OP_STORE_LOCAL, s, span),
            VarLocation::Upval(i)  => self.emit_u8(OP_STORE_UPVAL, i, span),
            VarLocation::Global(s) => self.emit_u16(OP_STORE_GLOBAL, s, span),
            _ => { self.error("대입 불가 대상", span); self.emit(OP_POP, span); }
        }
    }

    // ============================================================
    // Pre-scan top-level lets to assign global slots
    // ============================================================

    fn prescan_globals(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            let names: Vec<&String> = match &stmt.kind {
                StmtKind::Let { name, .. } => vec![name],
                StmtKind::LetUnpack { names, .. } => names.iter().collect(),
                _ => continue,
            };
            for name in names {
                if !self.globals.contains_key(name.as_str()) {
                    let slot = self.global_count;
                    self.globals.insert(name.clone(), slot);
                    self.global_count += 1;
                }
            }
        }
    }

    // ============================================================
    // Statement compilation
    // ============================================================

    fn compile_stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts { self.compile_stmt(s); }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        let sp = stmt.span;
        match &stmt.kind.clone() {
            StmtKind::Let { name, ty, value } => {
                // 지역 이름 있는 함수(fn f() { ... f() ... })는 본문이 자기 이름을 보도록 슬롯을 먼저
                // 선언한다 (최상위는 전역 prescan이 이미 처리). 이전에는 중첩 재귀가 컴파일 에러였다.
                let fn_idx = self.fn_stack.len() - 1;
                let is_global = fn_idx == 0 && self.globals.contains_key(name.as_str());
                if !is_global {
                    if let ExprKind::Function { name: Some(fname), .. } = &value.kind {
                        if fname == name {
                            let slot = self.cur_mut().declare_local(name);
                            self.compile_expr(value);
                            if let Some(t) = ty {
                                self.emit_u8(OP_CHECK_TYPE, t.as_u8(), sp);
                            }
                            self.emit_u8(OP_STORE_LOCAL, slot, sp);
                            return;
                        }
                    }
                }
                self.compile_expr(value);
                // 선택적 타입 힌트: 값이 스택 top에 있을 때 런타임 검증 (peek)
                if let Some(t) = ty {
                    self.emit_u8(OP_CHECK_TYPE, t.as_u8(), sp);
                }
                let fn_idx = self.fn_stack.len() - 1;
                if fn_idx == 0 && self.globals.contains_key(name.as_str()) {
                    // Top-level: store to global slot
                    let slot = *self.globals.get(name.as_str()).unwrap();
                    self.emit_u16(OP_STORE_GLOBAL, slot, sp);
                } else {
                    // Inside function: store to new local slot
                    let slot = self.cur_mut().declare_local(name);
                    self.emit_u8(OP_STORE_LOCAL, slot, sp);
                }
            }

            StmtKind::LetUnpack { names, value } => {
                self.compile_expr(value);
                self.emit_u8(OP_UNPACK, names.len().min(255) as u8, sp);
                if names.len() > 255 {
                    self.error("분해 선언의 이름이 너무 많습니다", sp);
                }
                // UNPACK은 원소를 순서대로 push → 마지막 이름부터 저장
                let fn_idx = self.fn_stack.len() - 1;
                let slots: Vec<VarLocation> = names.iter().map(|name| {
                    match self.globals.get(name.as_str()) {
                        Some(&slot) if fn_idx == 0 => VarLocation::Global(slot),
                        _ => VarLocation::Local(self.cur_mut().declare_local(name)),
                    }
                }).collect();
                for loc in slots.into_iter().rev() {
                    self.emit_store(loc, sp);
                }
            }

            StmtKind::Expr(expr) => {
                // 문장 위치의 메서드 호출은 "결과 버림" 표시 (순수 빌트인이면 VM이 에러)
                if let ExprKind::Call { callee, args } = &expr.kind {
                    if let ExprKind::Field { target, name } = &callee.kind {
                        if builtin_index(name).is_some() {
                            self.compile_method_call(target, name, args, true, sp);
                            self.emit(OP_POP, sp);
                            return;
                        }
                    }
                }
                self.compile_expr(expr);
                // Every expression statement discards the result.
                // Assign leaves the value on stack (via DUP+STORE), so we POP it too.
                self.emit(OP_POP, sp);
            }

            StmtKind::Return(val) => {
                match val {
                    Some(v) => self.compile_expr(v),
                    None    => { self.emit(OP_NIL, sp); }
                }
                // 반환 타입 힌트 런타임 검증 (명시적 return의 반환값)
                if let Some(t) = self.cur().ret_type {
                    self.emit_u8(OP_CHECK_TYPE, t.as_u8(), sp);
                }
                self.emit(OP_RETURN, sp);
            }

            StmtKind::If { cond, then, else_ } => {
                self.compile_expr(cond);
                let else_jump = self.emit_jump(OP_JUMP_FALSE, sp);

                self.cur_mut().begin_scope();
                self.compile_stmts(&then.stmts);
                self.cur_mut().end_scope();

                if let Some(el) = else_ {
                    let end_jump = self.emit_jump(OP_JUMP, sp);
                    self.patch_jump(else_jump);
                    self.cur_mut().begin_scope();
                    self.compile_stmts(&el.stmts);
                    self.cur_mut().end_scope();
                    self.patch_jump(end_jump);
                } else {
                    self.patch_jump(else_jump);
                }
            }

            StmtKind::While { cond, body } => {
                let loop_start = self.current_pos();
                let td = self.cur().try_depth;
                self.cur_mut().loops.push(LoopInfo {
                    loop_start,
                    is_for: false,
                    break_patches: Vec::new(),
                    try_depth_at_entry: td,
                });

                self.compile_expr(cond);
                let exit_jump = self.emit_jump(OP_JUMP_FALSE, sp);

                self.cur_mut().begin_scope();
                self.compile_stmts(&body.stmts);
                self.cur_mut().end_scope();

                // Jump back to condition
                let back_offset = -(((self.current_pos() + 3) as isize) - loop_start as isize) as i16;
                let cur_sp = sp;
                self.emit_u16(OP_JUMP, back_offset as u16, cur_sp);

                self.patch_jump(exit_jump);

                let loop_info = self.cur_mut().loops.pop().unwrap();
                for patch in loop_info.break_patches {
                    self.patch_jump(patch);
                }
            }

            StmtKind::For { vars, iter, body } => {
                self.compile_expr(iter);
                self.emit(if vars.len() > 1 { OP_MAKE_ITER_PAIRS } else { OP_MAKE_ITER }, sp);

                // Declare loop variable(s) as local. 여러 개면 원소를 숨은 슬롯에 받아 분해한다.
                self.cur_mut().begin_scope();
                let var_slot = if vars.len() == 1 {
                    self.cur_mut().declare_local(&vars[0])
                } else {
                    self.cur_mut().declare_local(" for-item")
                };
                let var_slots: Vec<u8> = if vars.len() > 1 {
                    vars.iter().map(|v| self.cur_mut().declare_local(v)).collect()
                } else {
                    Vec::new()
                };

                let loop_start = self.current_pos();
                let td = self.cur().try_depth;
                self.cur_mut().loops.push(LoopInfo {
                    loop_start,
                    is_for: true,
                    break_patches: Vec::new(),
                    try_depth_at_entry: td,
                });

                // FOR_ITER var_slot jump_done(i16)
                // Layout: [OP_FOR_ITER][var_slot u8][offset lo][offset hi]
                self.emit(OP_FOR_ITER, sp);
                self.cur_mut().chunk.emit(var_slot, sp);
                let done_patch = self.cur_mut().chunk.code.len();
                self.cur_mut().chunk.emit(0xff, sp);
                self.cur_mut().chunk.emit(0xff, sp);

                if !var_slots.is_empty() {
                    self.emit_u8(OP_LOAD_LOCAL, var_slot, sp);
                    self.emit_u8(OP_UNPACK, var_slots.len() as u8, sp);
                    for &slot in var_slots.iter().rev() {
                        self.emit_u8(OP_STORE_LOCAL, slot, sp);
                    }
                }

                self.cur_mut().begin_scope();
                self.compile_stmts(&body.stmts);
                self.cur_mut().end_scope();

                // Jump back to FOR_ITER
                let back_offset = -(((self.current_pos() + 3) as isize) - loop_start as isize) as i16;
                self.emit_u16(OP_JUMP, back_offset as u16, sp);

                // Patch the done jump
                let done_pos = self.current_pos();
                let done_offset = (done_pos as isize - done_patch as isize - 2) as i16;
                {
                    let chunk = &mut self.cur_mut().chunk;
                    chunk.code[done_patch]     = (done_offset as u16 & 0xff) as u8;
                    chunk.code[done_patch + 1] = (done_offset as u16 >> 8) as u8;
                }

                self.cur_mut().end_scope(); // loop var scope

                let loop_info = self.cur_mut().loops.pop().unwrap();
                for patch in loop_info.break_patches {
                    self.patch_jump(patch);
                }
            }

            StmtKind::Block(block) => {
                self.cur_mut().begin_scope();
                self.compile_stmts(&block.stmts);
                self.cur_mut().end_scope();
            }

            StmtKind::Parallel(block) => {
                // Part B: 진짜 병렬 실행 — spawn 스코프 push/pop
                self.emit(OP_PARALLEL_ENTER, sp);
                self.cur_mut().begin_scope();
                self.compile_stmts(&block.stmts);
                self.cur_mut().end_scope();
                self.emit(OP_PARALLEL_EXIT, sp);
            }

            StmtKind::Break => {
                let sp2 = sp;
                if self.cur().loops.is_empty() {
                    self.error("break: 루프 밖", sp);
                    return;
                }
                let is_for = self.cur().loops.last().map(|l| l.is_for).unwrap_or(false);
                // 빠져나가는 try 핸들러 제거 (try 본문 안에서의 break)
                let escaped = self.cur().try_depth
                    - self.cur().loops.last().unwrap().try_depth_at_entry;
                for _ in 0..escaped { self.emit(OP_POP_TRY, sp2); }
                if is_for {
                    self.emit(OP_POP, sp2); // pop iterator
                }
                let patch = self.emit_jump(OP_JUMP, sp2);
                self.cur_mut().loops.last_mut().unwrap().break_patches.push(patch);
            }

            StmtKind::Continue => {
                let sp2 = sp;
                if self.cur().loops.is_empty() {
                    self.error("continue: 루프 밖", sp);
                    return;
                }
                let escaped = self.cur().try_depth
                    - self.cur().loops.last().unwrap().try_depth_at_entry;
                for _ in 0..escaped { self.emit(OP_POP_TRY, sp2); }
                let loop_start = self.cur().loops.last().unwrap().loop_start;
                let back_offset = -(((self.current_pos() + 3) as isize) - loop_start as isize) as i16;
                self.emit_u16(OP_JUMP, back_offset as u16, sp2);
            }

            StmtKind::Try { body, catch_var, handler } => {
                // OP_SETUP_TRY <abs catch_ip:u16> (자리표시자 후 패치)
                self.emit(OP_SETUP_TRY, sp);
                let operand_pos = self.current_pos();
                self.cur_mut().chunk.emit(0xff, sp);
                self.cur_mut().chunk.emit(0xff, sp);
                self.cur_mut().try_depth += 1;

                // try 본문
                self.cur_mut().begin_scope();
                self.compile_stmts(&body.stmts);
                self.cur_mut().end_scope();

                // 정상 종료: 핸들러 제거 후 catch 건너뜀
                self.cur_mut().try_depth -= 1;
                self.emit(OP_POP_TRY, sp);
                let end_jump = self.emit_jump(OP_JUMP, sp);

                // catch 진입점 — SETUP_TRY 피연산자를 절대 위치로 패치
                let catch_ip = self.current_pos();
                {
                    let chunk = &mut self.cur_mut().chunk;
                    chunk.code[operand_pos]     = (catch_ip as u16 & 0xff) as u8;
                    chunk.code[operand_pos + 1] = (catch_ip as u16 >> 8) as u8;
                }
                // 던져진 값을 catch 변수에 바인딩 후 핸들러 실행
                self.cur_mut().begin_scope();
                let slot = self.cur_mut().declare_local(catch_var);
                self.emit_u8(OP_STORE_LOCAL, slot, sp);
                self.compile_stmts(&handler.stmts);
                self.cur_mut().end_scope();

                self.patch_jump(end_jump);
            }

            StmtKind::Throw(expr) => {
                self.compile_expr(expr);
                self.emit(OP_THROW, sp);
            }
        }
    }

    // ============================================================
    // Expression compilation
    // ============================================================

    fn compile_expr(&mut self, expr: &Expr) {
        let sp = expr.span;
        // Phase 8: 상수 폴딩 — 컴파일 시간에 평가 가능한 식은 상수로 내보냄
        if let Some(v) = const_eval(expr) {
            match &v {
                VmValue::Nil       => { self.emit(OP_NIL,   sp); }
                VmValue::Bool(b)   => { self.emit(if *b { OP_TRUE } else { OP_FALSE }, sp); }
                _                  => { self.emit_const(v,  sp); }
            }
            return;
        }
        match &expr.kind.clone() {
            ExprKind::Nil        => { self.emit(OP_NIL, sp); }
            ExprKind::Bool(b)    => { self.emit(if *b { OP_TRUE } else { OP_FALSE }, sp); }
            ExprKind::Int(n)     => { self.emit_const(VmValue::Int(*n), sp); }
            ExprKind::Float(n)   => { self.emit_const(VmValue::Float(*n), sp); }
            ExprKind::Str(s)     => { self.emit_const(VmValue::Str(s.clone()), sp); }

            ExprKind::Ident(name) => {
                let loc = self.resolve_name(name);
                if loc == VarLocation::Unknown {
                    self.error(format!("정의되지 않은 변수: '{name}'"), sp);
                    self.emit(OP_NIL, sp);
                } else {
                    self.emit_load(loc, sp);
                }
            }

            ExprKind::List(items) => {
                let count = items.len() as u16;
                for item in items { self.compile_expr(item); }
                self.emit_u16(OP_MAKE_LIST, count, sp);
            }

            ExprKind::Map(entries) => {
                let count = entries.len() as u16;
                for (key, val) in entries {
                    let k = match key {
                        MapKey::Int(n) => VmValue::Int(*n),
                        other => VmValue::Str(other.as_str()),
                    };
                    self.emit_const(k, sp);
                    self.compile_expr(val);
                }
                self.emit_u16(OP_MAKE_MAP, count, sp);
            }

            ExprKind::Unary { op, expr: inner } => {
                self.compile_expr(inner);
                match op {
                    UnaryOp::Neg => self.emit(OP_NEG, sp),
                    UnaryOp::Not => self.emit(OP_NOT, sp),
                };
            }

            ExprKind::Binary { op, left, right } => {
                match op {
                    BinaryOp::And => {
                        self.compile_expr(left);
                        let patch = self.emit_jump(OP_JUMP_AND, sp);
                        self.emit(OP_POP, sp);
                        self.compile_expr(right);
                        self.patch_jump(patch);
                    }
                    BinaryOp::Or => {
                        self.compile_expr(left);
                        let patch = self.emit_jump(OP_JUMP_OR, sp);
                        self.emit(OP_POP, sp);
                        self.compile_expr(right);
                        self.patch_jump(patch);
                    }
                    _ => {
                        self.compile_expr(left);
                        self.compile_expr(right);
                        let op_byte = match op {
                            BinaryOp::Add => OP_ADD, BinaryOp::Sub => OP_SUB,
                            BinaryOp::Mul => OP_MUL, BinaryOp::Div => OP_DIV,
                            BinaryOp::Mod => OP_MOD,
                            BinaryOp::Eq  => OP_EQ,  BinaryOp::Ne  => OP_NE,
                            BinaryOp::Lt  => OP_LT,  BinaryOp::Le  => OP_LE,
                            BinaryOp::Gt  => OP_GT,  BinaryOp::Ge  => OP_GE,
                            BinaryOp::In => OP_IN,
                            BinaryOp::NotIn => {
                                self.emit(OP_IN, sp);
                                OP_NOT
                            }
                            BinaryOp::And | BinaryOp::Or => unreachable!(),
                        };
                        self.emit(op_byte, sp);
                    }
                }
            }

            ExprKind::Call { callee, args } => {
                // x.f(args) — f가 빌트인 이름이면 메서드 호출(UFCS). 맵의 키 f가 우선.
                if let ExprKind::Field { target, name } = &callee.kind {
                    if builtin_index(name).is_some() {
                        self.compile_method_call(target, name, args, false, sp);
                        return;
                    }
                }
                self.compile_expr(callee);
                let argc = args.len() as u8;
                for arg in args { self.compile_expr(arg); }
                self.emit_u8(OP_CALL, argc, sp);
            }

            ExprKind::FString(parts) => {
                // 조각을 차례로 스택에 쌓고 OP_BUILD_STR로 한 번에 이어 붙인다 (255개씩 묶음)
                let mut pending = 0u8;
                let mut have_acc = false;
                for p in parts {
                    match p {
                        FStringPart::Lit(s) => {
                            let idx = self.add_const(VmValue::Str(s.clone()));
                            self.emit_u16(OP_CONST, idx, sp);
                        }
                        FStringPart::Expr(e, spec) => {
                            self.compile_expr(e);
                            if let Some(spec) = spec {
                                let idx = self.add_const(VmValue::Str(spec.clone()));
                                self.emit_u16(OP_FMT, idx, sp);
                            }
                        }
                    }
                    pending += 1;
                    if pending == 254 {
                        self.emit_u8(OP_BUILD_STR, pending + have_acc as u8, sp);
                        have_acc = true;
                        pending = 0;
                    }
                }
                if pending > 0 || !have_acc {
                    self.emit_u8(OP_BUILD_STR, pending + have_acc as u8, sp);
                }
            }

            ExprKind::IfExpr { cond, then, else_ } => {
                self.compile_expr(cond);
                let else_jump = self.emit_jump(OP_JUMP_FALSE, sp);
                self.compile_block_value(then, sp);
                let end_jump = self.emit_jump(OP_JUMP, sp);
                self.patch_jump(else_jump);
                self.compile_block_value(else_, sp);
                self.patch_jump(end_jump);
            }

            ExprKind::Index { target, index } => {
                self.compile_expr(target);
                self.compile_expr(index);
                self.emit(OP_INDEX_GET, sp);
            }

            ExprKind::Field { target, name } => {
                self.compile_expr(target);
                let name_idx = self.add_const(VmValue::Str(name.clone()));
                self.emit_u16(OP_FIELD_GET, name_idx, sp);
            }

            ExprKind::Function { name, params, param_types, param_defaults, ret_type, body } => {
                self.compile_fn(name.clone(), params, param_types, param_defaults, *ret_type, body, sp);
            }

            ExprKind::Spawn(inner) => {
                // Part B: 진짜 spawn — OS 스레드로 실행
                // 메서드 호출(spawn x.f())은 아래 thunk 경로로 (OP_SPAWN은 클로저 callee만 받음)
                // 빌트인 직접 호출(spawn len(x))도 thunk 경로로 — OP_SPAWN은 클로저만 받는다
                let is_method = match &inner.kind {
                    ExprKind::Call { callee, .. } => match &callee.kind {
                        ExprKind::Field { name, .. } => builtin_index(name).is_some(),
                        ExprKind::Ident(name) => matches!(self.resolve_name(name), VarLocation::Builtin(_)),
                        _ => false,
                    },
                    _ => false,
                };
                if let (ExprKind::Call { callee, args }, false) = (&inner.kind, is_method) {
                    // spawn foo(a, b) → callee + args + OP_SPAWN(argc)
                    self.compile_expr(callee);
                    let argc = args.len() as u8;
                    for arg in args { self.compile_expr(arg); }
                    self.emit_u8(OP_SPAWN, argc, sp);
                } else {
                    // spawn <non-call expr> → implicit 0-arg thunk + OP_SPAWN(0)
                    // 내부 식을 캡처하는 익명 클로저로 래핑
                    let inner_clone = inner.as_ref().clone();
                    let body = crate::ast::Block {
                        stmts: vec![crate::ast::Stmt {
                            kind: crate::ast::StmtKind::Return(Some(inner_clone)),
                            span: sp,
                        }],
                        span: sp,
                    };
                    self.compile_fn(None, &[], &[], &[], None, &body, sp);
                    self.emit_u8(OP_SPAWN, 0, sp);
                }
            }

            ExprKind::Assign { target, value } => {
                self.compile_assign(target, value, sp);
            }

            ExprKind::CompoundAssign { op, target, value } => {
                self.compile_compound_assign(*op, target, value, sp);
            }

            ExprKind::Slice { target, start, end } => {
                self.compile_expr(target);
                for b in [start, end] {
                    match b {
                        Some(e) => self.compile_expr(e),
                        None => { self.emit(OP_NIL, sp); }
                    }
                }
                self.emit(OP_SLICE, sp);
            }
        }
    }

    /// `x = push(x, e)` → (0, e), `x = x + e` → (1, e). push가 빌트인일 때만.
    fn update_pattern<'a>(&mut self, name: &str, value: &'a Expr) -> Option<(u8, &'a Expr)> {
        match &value.kind {
            ExprKind::Call { callee, args } if args.len() == 2 => {
                let is_push = matches!(&callee.kind, ExprKind::Ident(c) if c == "push")
                    && matches!(&args[0].kind, ExprKind::Ident(a) if a == name);
                if is_push && builtin_index("push").map(|i| VarLocation::Builtin(i as u8))
                    == Some(self.resolve_name("push"))
                {
                    return Some((0, &args[1]));
                }
                None
            }
            ExprKind::Binary { op: BinaryOp::Add, left, right }
                if matches!(&left.kind, ExprKind::Ident(a) if a == name) =>
            {
                Some((1, right))
            }
            _ => None,
        }
    }

    /// 메서드 호출 `target.name(args)` → OP_METHOD_CALL (빌트인 이름일 때만).
    fn compile_method_call(&mut self, target: &Expr, name: &str, args: &[Expr], discarded: bool, sp: Span) {
        self.compile_expr(target);
        for arg in args { self.compile_expr(arg); }
        let name_idx = self.add_const(VmValue::Str(name.to_string()));
        self.emit_u16(OP_METHOD_CALL, name_idx, sp);
        self.emit(args.len() as u8, sp);
        self.emit(discarded as u8, sp);
    }

    /// 블록을 값으로 컴파일: 마지막 문장은 식이어야 하며 그 값이 스택에 남는다 (if 식용).
    fn compile_block_value(&mut self, block: &Block, sp: Span) {
        self.cur_mut().begin_scope();
        match block.stmts.split_last() {
            Some((last, init)) => {
                self.compile_stmts(init);
                match &last.kind {
                    StmtKind::Expr(e) => self.compile_expr(e),
                    _ => {
                        self.error("if 식의 각 갈래는 값(식)으로 끝나야 합니다", last.span);
                        self.emit(OP_NIL, sp);
                    }
                }
            }
            None => {
                self.error("if 식의 갈래가 비어 있습니다 — 값이 필요합니다", sp);
                self.emit(OP_NIL, sp);
            }
        }
        self.cur_mut().end_scope();
    }

    /// `x op= e` → `x = x op e` (변수: 갱신 패턴 재사용), 경로는 OP_SET_PATH의 binop로 (키 1회 평가).
    fn compile_compound_assign(&mut self, op: BinaryOp, target: &Expr, value: &Expr, sp: Span) {
        let op_byte = match op {
            BinaryOp::Add => OP_ADD, BinaryOp::Sub => OP_SUB, BinaryOp::Mul => OP_MUL,
            BinaryOp::Div => OP_DIV, BinaryOp::Mod => OP_MOD,
            _ => {
                self.error("지원하지 않는 복합 대입 연산자", sp);
                self.emit(OP_NIL, sp);
                return;
            }
        };
        match &target.kind {
            ExprKind::Ident(_) => {
                let desugared = Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(target.clone()),
                        right: Box::new(value.clone()),
                    },
                    span: value.span,
                };
                self.compile_assign(target, &desugared, sp);
            }
            ExprKind::Index { .. } | ExprKind::Field { .. } => {
                self.compile_path_assign(target, value, op_byte, sp);
            }
            _ => {
                self.error("유효하지 않은 대입 대상", sp);
                self.emit(OP_NIL, sp);
            }
        }
    }

    /// 경로 대입: 루트는 반드시 변수. v[a][b].c = val (binop≠0이면 v[a][b].c op= val)
    fn compile_path_assign(&mut self, target: &Expr, value: &Expr, binop: u8, sp: Span) {
        let mut keys: Vec<PathKey> = Vec::new();
        let root = match collect_assign_path(target, &mut keys) {
            Some(r) => r.to_string(),
            None => {
                self.error("대입 대상은 변수에서 시작해야 합니다 (예: v[i], v.f, v[i][j])", sp);
                self.emit(OP_NIL, sp);
                return;
            }
        };
        let loc = self.resolve_name(&root);
        let Some((kind, slot)) = var_operand(&loc) else {
            self.error(format!("정의되지 않은 변수: '{root}'"), sp);
            self.emit(OP_NIL, sp);
            return;
        };
        if keys.len() > u8::MAX as usize {
            self.error("대입 경로가 너무 깊습니다", sp);
            self.emit(OP_NIL, sp);
            return;
        }
        self.emit_load(loc, sp);
        for key in &keys {
            match key {
                PathKey::Expr(e) => self.compile_expr(e),
                PathKey::Name(n) => {
                    let idx = self.add_const(VmValue::Str(n.to_string()));
                    self.emit_u16(OP_CONST, idx, sp);
                }
            }
        }
        self.compile_expr(value);
        self.emit_u16(OP_SET_PATH, slot, sp);
        self.emit(kind, sp);
        self.emit(keys.len() as u8, sp);
        self.emit(binop, sp);
    }

    fn compile_assign(&mut self, target: &Expr, value: &Expr, sp: Span) {
        match &target.kind.clone() {
            ExprKind::Ident(name) => {
                // 갱신 패턴(x = push(x, e) / x = x + e)은 제자리 변경 opcode로 (Phase 45)
                let loc = self.resolve_name(name);
                if let Some((kind, slot)) = var_operand(&loc) {
                    if let Some((op, rhs)) = self.update_pattern(name, value) {
                        self.emit_load(loc, sp);
                        self.compile_expr(rhs);
                        self.emit_u16(OP_UPDATE_VAR, slot, sp);
                        self.emit(kind, sp);
                        self.emit(op, sp);
                        return;
                    }
                }
                self.compile_expr(value);
                if loc == VarLocation::Unknown {
                    self.error(format!("정의되지 않은 변수: '{name}'"), sp);
                    self.emit(OP_POP, sp);
                } else {
                    // DUP so the assigned value remains on stack as expr result
                    self.emit(OP_DUP, sp);
                    self.emit_store(loc, sp);
                }
            }
            ExprKind::Index { .. } | ExprKind::Field { .. } => {
                self.compile_path_assign(target, value, 0, sp);
            }
            _ => {
                self.error("유효하지 않은 대입 대상", sp);
                self.emit(OP_NIL, sp);
            }
        }
    }

    // ============================================================
    // Function / closure compilation
    // ============================================================

    #[allow(clippy::too_many_arguments)]
    fn compile_fn(
        &mut self,
        name: Option<String>,
        params: &[String],
        param_types: &[Option<TypeAnn>],
        param_defaults: &[Option<Expr>],
        ret_type: Option<TypeAnn>,
        body: &Block,
        sp: Span,
    ) {
        // Push a new FnCompiler
        let mut fn_comp = FnCompiler::new(name.clone(), params.len());
        fn_comp.file = self.file.clone();
        fn_comp.min_arity = param_defaults.iter().take_while(|d| d.is_none()).count().min(params.len());
        fn_comp.ret_type = ret_type;

        // Parameters are locals at scope_depth=1
        fn_comp.scope_depth = 1;
        for param in params {
            let slot = fn_comp.locals.len();
            fn_comp.locals.push(Local {
                name: param.clone(),
                scope_depth: 1,
                slot,
            });
            if slot + 1 > fn_comp.local_peak { fn_comp.local_peak = slot + 1; }
        }

        // If named fn, the name itself is pre-declared in the enclosing scope
        // (handled by StmtKind::Let + fn name). Inside the body, self-reference
        // finds the name via the upvalue/global mechanism — nothing special needed here.

        self.fn_stack.push(fn_comp);
        self.cur_mut().begin_scope(); // body block scope

        // 기본값: 인자가 생략된 매개변수만 호출 시점에 계산해 채운다 (Phase 47)
        for (i, d) in param_defaults.iter().enumerate() {
            if let Some(d) = d {
                self.emit_u8(OP_JUMP_IF_ARG, i as u8, sp);
                let patch = self.cur_mut().chunk.code.len();
                self.cur_mut().chunk.emit(0xff, sp);
                self.cur_mut().chunk.emit(0xff, sp);
                self.compile_expr(d);
                self.emit_u8(OP_STORE_LOCAL, i as u8, sp);
                let target = self.current_pos();
                let off = (target as isize - patch as isize - 2) as i16;
                let chunk = &mut self.cur_mut().chunk;
                chunk.code[patch] = (off as u16 & 0xff) as u8;
                chunk.code[patch + 1] = (off as u16 >> 8) as u8;
            }
        }

        // 파라미터 타입 힌트 런타임 검증 (본문 시작 시)
        for (i, pt) in param_types.iter().enumerate() {
            if let Some(t) = pt {
                self.emit_u8(OP_LOAD_LOCAL, i as u8, sp);
                self.emit_u8(OP_CHECK_TYPE, t.as_u8(), sp);
                self.emit(OP_POP, sp);
            }
        }

        self.compile_stmts(&body.stmts);

        // Implicit nil return
        let body_sp = body.span;
        self.emit(OP_NIL, body_sp);
        self.emit(OP_RETURN, body_sp);

        self.cur_mut().end_scope();

        let finished = self.fn_stack.pop().unwrap();
        let upvalue_descs = finished.upvalues.clone();
        let compiled_fn = finished.into_compiled();

        // Emit OP_CLOSURE in the enclosing function's chunk
        let fn_const_idx = self.add_const(VmValue::Function(compiled_fn));
        let uv_count = upvalue_descs.len() as u8;

        self.emit_u16(OP_CLOSURE, fn_const_idx, sp); // fn 상수 인덱스도 u16
        self.emit(uv_count, sp);
        for uv in &upvalue_descs {
            self.emit(uv.is_local as u8, sp);
            self.emit(uv.index, sp);
        }
    }

    // ============================================================
    // Top-level compile
    // ============================================================

    fn compile_program(&mut self, prog: &Program) {
        // Pre-scan globals
        self.prescan_globals(&prog.stmts);

        // Wrap top-level code in a "main" function
        let mut main_fn = FnCompiler::new(Some("main".to_string()), 0);
        main_fn.file = self.file.clone();
        self.fn_stack.push(main_fn);
        self.cur_mut().begin_scope();

        self.compile_stmts(&prog.stmts);

        self.cur_mut().end_scope();

        // Implicit return nil at end of main
        let sp = Span::new(0, 0);
        self.emit(OP_NIL, sp);
        self.emit(OP_RETURN, sp);

        // fn_stack should have exactly 1 element (main)
    }
}

// ============================================================================
// VarLocation
// ============================================================================

/// 경로 대입의 한 단계: `[식]` 또는 `.이름`.
enum PathKey<'a> {
    Expr(&'a Expr),
    Name(&'a str),
}

/// `v[a].b[c]` → 루트 변수 이름을 반환하고 keys에 [a, "b", c]를 채운다.
fn collect_assign_path<'a>(e: &'a Expr, keys: &mut Vec<PathKey<'a>>) -> Option<&'a str> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n.as_str()),
        ExprKind::Index { target, index } => {
            let root = collect_assign_path(target, keys)?;
            keys.push(PathKey::Expr(index));
            Some(root)
        }
        ExprKind::Field { target, name } => {
            let root = collect_assign_path(target, keys)?;
            keys.push(PathKey::Name(name));
            Some(root)
        }
        _ => None,
    }
}

/// 변수 위치 → (kind, slot) operand. kind: 0=local, 1=upval, 2=global.
fn var_operand(loc: &VarLocation) -> Option<(u8, u16)> {
    match loc {
        VarLocation::Local(s) => Some((0, *s as u16)),
        VarLocation::Upval(i) => Some((1, *i as u16)),
        VarLocation::Global(s) => Some((2, *s)),
        _ => None,
    }
}

#[derive(Clone, PartialEq, Debug)]
enum VarLocation {
    Local(u8),
    Upval(u8),
    Global(u16),
    Builtin(u8),
    Unknown,
}

// ============================================================================
// Public entry point
// ============================================================================

pub struct CompileOutput {
    pub main_fn: Arc<CompiledFn>,
    pub global_count: u16,
    pub global_names: HashMap<String, u16>,
}

/// Compile a parsed program into a main CompiledFn.
/// Returns Err if any compile errors occurred.
pub fn compile(prog: &Program) -> Result<CompileOutput, Vec<CompileError>> {
    compile_with_file(prog, None)
}

/// 소스 파일 경로를 기록하며 컴파일 (import 모듈용 — 런타임 에러가 모듈 파일 위치를 가리키게).
pub fn compile_with_file(prog: &Program, file: Option<&str>) -> Result<CompileOutput, Vec<CompileError>> {
    let mut compiler = Compiler::new();
    compiler.file = file.map(Arc::from);
    compiler.compile_program(prog);

    if !compiler.errors.is_empty() {
        return Err(compiler.errors);
    }

    let global_names = compiler.globals.clone();
    let main_fn = compiler.fn_stack.pop().unwrap().into_compiled();
    let global_count = compiler.global_count;
    Ok(CompileOutput { main_fn, global_count, global_names })
}

/// REPL용 증분 컴파일: 이전 스니펫들이 만든 전역(name→slot)을 시드로 이어받아
/// 같은 슬롯 배치를 유지한 채 새 스니펫을 컴파일한다.
/// 반환된 global_names에는 시드 + 새로 선언된 전역이 모두 담긴다.
pub fn compile_repl(
    prog: &Program,
    seed_globals: &HashMap<String, u16>,
) -> Result<CompileOutput, Vec<CompileError>> {
    let mut compiler = Compiler::new();
    compiler.globals = seed_globals.clone();
    compiler.global_count = seed_globals.values().map(|s| s + 1).max().unwrap_or(0);
    compiler.compile_program(prog);

    if !compiler.errors.is_empty() {
        return Err(compiler.errors);
    }

    let global_names = compiler.globals.clone();
    let main_fn = compiler.fn_stack.pop().unwrap().into_compiled();
    let global_count = compiler.global_count;
    Ok(CompileOutput { main_fn, global_count, global_names })
}

// ============================================================================
// Phase 8: 상수 폴딩 (const_eval + const_fold_value)
// ============================================================================

fn const_fold_value(op: &BinaryOp, l: VmValue, r: VmValue) -> Option<VmValue> {
    match (op, l, r) {
        // Int 산술은 checked: 오버플로면 폴딩 포기(None) → 런타임에서 잡히는 에러로 일관
        (BinaryOp::Add, VmValue::Int(a),   VmValue::Int(b))   => a.checked_add(b).map(VmValue::Int),
        (BinaryOp::Add, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a + b)),
        (BinaryOp::Add, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 + b)),
        (BinaryOp::Add, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a + b as f64)),
        (BinaryOp::Add, VmValue::Str(a),   VmValue::Str(b))   => Some(VmValue::Str(a + &b)),
        (BinaryOp::Sub, VmValue::Int(a),   VmValue::Int(b))   => a.checked_sub(b).map(VmValue::Int),
        (BinaryOp::Sub, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a - b)),
        (BinaryOp::Sub, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 - b)),
        (BinaryOp::Sub, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a - b as f64)),
        (BinaryOp::Mul, VmValue::Int(a),   VmValue::Int(b))   => a.checked_mul(b).map(VmValue::Int),
        (BinaryOp::Mul, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a * b)),
        (BinaryOp::Mul, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 * b)),
        (BinaryOp::Mul, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a * b as f64)),
        (BinaryOp::Div, VmValue::Int(a),   VmValue::Int(b)) if b != 0 => {
            // `/`는 항상 float (Phase 57 — VM과 같게)
            Some(VmValue::Float(a as f64 / b as f64))
        }
        (BinaryOp::Div, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a / b)),
        (BinaryOp::Div, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 / b)),
        (BinaryOp::Div, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a / b as f64)),
        (BinaryOp::Mod, VmValue::Int(a),   VmValue::Int(b)) if b != 0 => a.checked_rem(b).map(VmValue::Int),
        (BinaryOp::Eq,  VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Bool(a == b)),
        (BinaryOp::Eq,  VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Bool(a == b)),
        (BinaryOp::Eq,  VmValue::Bool(a),  VmValue::Bool(b))  => Some(VmValue::Bool(a == b)),
        (BinaryOp::Eq,  VmValue::Str(a),   VmValue::Str(b))   => Some(VmValue::Bool(a == b)),
        (BinaryOp::Eq,  VmValue::Nil,      VmValue::Nil)      => Some(VmValue::Bool(true)),
        (BinaryOp::Ne,  VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Bool(a != b)),
        (BinaryOp::Ne,  VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Bool(a != b)),
        (BinaryOp::Ne,  VmValue::Bool(a),  VmValue::Bool(b))  => Some(VmValue::Bool(a != b)),
        (BinaryOp::Ne,  VmValue::Str(a),   VmValue::Str(b))   => Some(VmValue::Bool(a != b)),
        (BinaryOp::Lt,  VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Bool(a < b)),
        (BinaryOp::Lt,  VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Bool(a < b)),
        (BinaryOp::Le,  VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Bool(a <= b)),
        (BinaryOp::Le,  VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Bool(a <= b)),
        (BinaryOp::Gt,  VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Bool(a > b)),
        (BinaryOp::Gt,  VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Bool(a > b)),
        (BinaryOp::Ge,  VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Bool(a >= b)),
        (BinaryOp::Ge,  VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Bool(a >= b)),
        (BinaryOp::And, VmValue::Bool(a),  VmValue::Bool(b))  => Some(VmValue::Bool(a && b)),
        (BinaryOp::Or,  VmValue::Bool(a),  VmValue::Bool(b))  => Some(VmValue::Bool(a || b)),
        _ => None,
    }
}

fn const_eval(expr: &Expr) -> Option<VmValue> {
    match &expr.kind {
        ExprKind::Int(n)   => Some(VmValue::Int(*n)),
        ExprKind::Float(n) => Some(VmValue::Float(*n)),
        ExprKind::Str(s)   => Some(VmValue::Str(s.clone())),
        ExprKind::Bool(b)  => Some(VmValue::Bool(*b)),
        ExprKind::Nil      => Some(VmValue::Nil),
        ExprKind::Unary { op, expr } => {
            let v = const_eval(expr)?;
            match (op, v) {
                (UnaryOp::Neg, VmValue::Int(n))   => n.checked_neg().map(VmValue::Int),
                (UnaryOp::Neg, VmValue::Float(n)) => Some(VmValue::Float(-n)),
                (UnaryOp::Not, v) => Some(VmValue::Bool(!v.is_truthy())),
                _ => None,
            }
        }
        ExprKind::Binary { op, left, right } => {
            if matches!(op, BinaryOp::And | BinaryOp::Or) { return None; }
            let l = const_eval(left)?;
            let r = const_eval(right)?;
            const_fold_value(op, l, r)
        }
        _ => None,
    }
}
