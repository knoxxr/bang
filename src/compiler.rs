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
}

struct FnCompiler {
    chunk: Chunk,
    name: Option<String>,
    arity: usize,
    locals: Vec<Local>,
    upvalues: Vec<UpvalueDesc>,
    loops: Vec<LoopInfo>,
    scope_depth: usize,
    local_peak: usize, // maximum locals used (= local_count for CompiledFn)
}

impl FnCompiler {
    fn new(name: Option<String>, arity: usize) -> Self {
        Self {
            chunk: Chunk::new(),
            name,
            arity,
            locals: Vec::new(),
            upvalues: Vec::new(),
            loops: Vec::new(),
            scope_depth: 0,
            local_peak: 0,
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

    fn add_constant(&mut self, val: VmValue) -> u8 {
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
    fn_stack: Vec<FnCompiler>,
    globals: HashMap<String, u16>, // top-level name → global slot index
    global_count: u16,
    errors: Vec<CompileError>,
}

impl Compiler {
    fn new() -> Self {
        Self {
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

    fn add_const(&mut self, val: VmValue) -> u8 {
        self.cur_mut().add_constant(val)
    }

    fn emit_const(&mut self, val: VmValue, span: Span) {
        let idx = self.add_const(val);
        self.emit_u8(OP_CONST, idx, span);
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
            VarLocation::Unknown    => { self.emit(OP_NIL, span); }
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
            if let StmtKind::Let { name, .. } = &stmt.kind {
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
            StmtKind::Let { name, value } => {
                self.compile_expr(value);
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

            StmtKind::Expr(expr) => {
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
                self.cur_mut().loops.push(LoopInfo {
                    loop_start,
                    is_for: false,
                    break_patches: Vec::new(),
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

            StmtKind::For { var, iter, body } => {
                self.compile_expr(iter);
                self.emit(OP_MAKE_ITER, sp);

                // Declare loop variable as local
                self.cur_mut().begin_scope();
                let var_slot = self.cur_mut().declare_local(var);

                let loop_start = self.current_pos();
                self.cur_mut().loops.push(LoopInfo {
                    loop_start,
                    is_for: true,
                    break_patches: Vec::new(),
                });

                // FOR_ITER var_slot jump_done(i16)
                // Layout: [OP_FOR_ITER][var_slot u8][offset lo][offset hi]
                self.emit(OP_FOR_ITER, sp);
                self.cur_mut().chunk.emit(var_slot, sp);
                let done_patch = self.cur_mut().chunk.code.len();
                self.cur_mut().chunk.emit(0xff, sp);
                self.cur_mut().chunk.emit(0xff, sp);

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
                let is_for = self.cur().loops.last().map(|l| l.is_for).unwrap_or(false);
                if self.cur().loops.is_empty() {
                    self.error("break: 루프 밖", sp);
                    return;
                }
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
                let loop_start = self.cur().loops.last().unwrap().loop_start;
                let back_offset = -(((self.current_pos() + 3) as isize) - loop_start as isize) as i16;
                self.emit_u16(OP_JUMP, back_offset as u16, sp2);
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
                    self.emit_const(VmValue::Str(key.as_str().to_string()), sp);
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
                            BinaryOp::And | BinaryOp::Or => unreachable!(),
                        };
                        self.emit(op_byte, sp);
                    }
                }
            }

            ExprKind::Call { callee, args } => {
                self.compile_expr(callee);
                let argc = args.len() as u8;
                for arg in args { self.compile_expr(arg); }
                self.emit_u8(OP_CALL, argc, sp);
            }

            ExprKind::Index { target, index } => {
                self.compile_expr(target);
                self.compile_expr(index);
                self.emit(OP_INDEX_GET, sp);
            }

            ExprKind::Field { target, name } => {
                self.compile_expr(target);
                let name_idx = self.add_const(VmValue::Str(name.clone()));
                self.emit_u8(OP_FIELD_GET, name_idx, sp);
            }

            ExprKind::Function { name, params, body } => {
                self.compile_fn(name.clone(), params, body, sp);
            }

            ExprKind::Spawn(inner) => {
                // Part B: 진짜 spawn — OS 스레드로 실행
                if let ExprKind::Call { callee, args } = &inner.kind {
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
                    self.compile_fn(None, &[], &body, sp);
                    self.emit_u8(OP_SPAWN, 0, sp);
                }
            }

            ExprKind::Assign { target, value } => {
                self.compile_assign(target, value, sp);
            }
        }
    }

    fn compile_assign(&mut self, target: &Expr, value: &Expr, sp: Span) {
        match &target.kind.clone() {
            ExprKind::Ident(name) => {
                self.compile_expr(value);
                let loc = self.resolve_name(name);
                if loc == VarLocation::Unknown {
                    self.error(format!("정의되지 않은 변수: '{name}'"), sp);
                    self.emit(OP_POP, sp);
                } else {
                    // DUP so the assigned value remains on stack as expr result
                    self.emit(OP_DUP, sp);
                    self.emit_store(loc, sp);
                }
            }
            ExprKind::Index { target: container, index } => {
                if let ExprKind::Ident(name) = &container.kind {
                    let loc = self.resolve_name(name);
                    self.emit_load(loc.clone(), sp);
                    self.compile_expr(index);
                    self.compile_expr(value);
                    self.emit(OP_INDEX_SET, sp);
                    self.emit(OP_DUP, sp);
                    self.emit_store(loc, sp);
                } else {
                    self.error("복잡한 인덱스 대입 미지원", sp);
                    self.emit(OP_NIL, sp);
                }
            }
            ExprKind::Field { target: container, name: field } => {
                if let ExprKind::Ident(var_name) = &container.kind {
                    let loc = self.resolve_name(var_name);
                    self.emit_load(loc.clone(), sp);
                    let key_idx = self.add_const(VmValue::Str(field.clone()));
                    self.emit_u8(OP_CONST, key_idx, sp);
                    self.compile_expr(value);
                    self.emit(OP_INDEX_SET, sp);
                    self.emit(OP_DUP, sp);
                    self.emit_store(loc, sp);
                } else {
                    self.error("복잡한 필드 대입 미지원", sp);
                    self.emit(OP_NIL, sp);
                }
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

    fn compile_fn(
        &mut self,
        name: Option<String>,
        params: &[String],
        body: &Block,
        sp: Span,
    ) {
        // Push a new FnCompiler
        let mut fn_comp = FnCompiler::new(name.clone(), params.len());

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

        self.emit_u8(OP_CLOSURE, fn_const_idx, sp);
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
        let main_fn = FnCompiler::new(Some("main".to_string()), 0);
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
    let mut compiler = Compiler::new();
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
        (BinaryOp::Add, VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Int(a + b)),
        (BinaryOp::Add, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a + b)),
        (BinaryOp::Add, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 + b)),
        (BinaryOp::Add, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a + b as f64)),
        (BinaryOp::Add, VmValue::Str(a),   VmValue::Str(b))   => Some(VmValue::Str(a + &b)),
        (BinaryOp::Sub, VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Int(a - b)),
        (BinaryOp::Sub, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a - b)),
        (BinaryOp::Sub, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 - b)),
        (BinaryOp::Sub, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a - b as f64)),
        (BinaryOp::Mul, VmValue::Int(a),   VmValue::Int(b))   => Some(VmValue::Int(a * b)),
        (BinaryOp::Mul, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a * b)),
        (BinaryOp::Mul, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 * b)),
        (BinaryOp::Mul, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a * b as f64)),
        (BinaryOp::Div, VmValue::Int(a),   VmValue::Int(b)) if b != 0 => {
            if a % b == 0 { Some(VmValue::Int(a / b)) } else { Some(VmValue::Float(a as f64 / b as f64)) }
        }
        (BinaryOp::Div, VmValue::Float(a), VmValue::Float(b)) => Some(VmValue::Float(a / b)),
        (BinaryOp::Div, VmValue::Int(a),   VmValue::Float(b)) => Some(VmValue::Float(a as f64 / b)),
        (BinaryOp::Div, VmValue::Float(a), VmValue::Int(b))   => Some(VmValue::Float(a / b as f64)),
        (BinaryOp::Mod, VmValue::Int(a),   VmValue::Int(b)) if b != 0 => Some(VmValue::Int(a % b)),
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
                (UnaryOp::Neg, VmValue::Int(n))   => Some(VmValue::Int(-n)),
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
