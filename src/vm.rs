// Bang — Phase 5: Bytecode VM (stack VM + upvalues, Part B multi-thread spawn)

#![allow(clippy::ptr_arg)]

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};

use crate::compiler::{
    OP_POP, OP_NIL, OP_TRUE, OP_FALSE, OP_CONST, OP_DUP,
    OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MOD, OP_NEG,
    OP_EQ, OP_NE, OP_LT, OP_LE, OP_GT, OP_GE, OP_NOT,
    OP_JUMP, OP_JUMP_FALSE, OP_JUMP_AND, OP_JUMP_OR,
    OP_LOAD_LOCAL, OP_STORE_LOCAL,
    OP_LOAD_UPVAL, OP_STORE_UPVAL,
    OP_LOAD_GLOBAL, OP_STORE_GLOBAL,
    OP_LOAD_BUILTIN,
    OP_CLOSURE, OP_CALL, OP_RETURN,
    OP_MAKE_LIST, OP_MAKE_MAP,
    OP_INDEX_GET, OP_INDEX_SET,
    OP_FIELD_GET,
    OP_MAKE_ITER, OP_FOR_ITER,
    OP_SPAWN, OP_PARALLEL_ENTER, OP_PARALLEL_EXIT,
    OP_SETUP_TRY, OP_POP_TRY, OP_THROW, OP_CHECK_TYPE,
};
use crate::ast::TypeAnn;
use crate::lexer::token::Span;
use crate::runtime::{BangChannel, RuntimeError};

// ============================================================================
// VmFuture — spawn 결과 핸들 (Condvar 기반)
// ============================================================================

pub struct VmFuture {
    result: Mutex<Option<Result<VmValue, RuntimeError>>>,
    ready:  Condvar,
}

impl VmFuture {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { result: Mutex::new(None), ready: Condvar::new() })
    }

    pub fn complete(&self, val: Result<VmValue, RuntimeError>) {
        *self.result.lock().unwrap() = Some(val);
        self.ready.notify_all();
    }

    pub fn resolve(&self) -> Result<VmValue, RuntimeError> {
        let mut g = self.result.lock().unwrap();
        loop {
            if let Some(r) = &*g { return r.clone(); }
            g = self.ready.wait(g).unwrap();
        }
    }
}

impl fmt::Debug for VmFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "<future>") }
}

// ============================================================================
// VmValue
// ============================================================================

#[derive(Clone, Debug)]
pub enum VmValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    // 값 의미론 + copy-on-write: clone은 Arc 공유(O(1)), 변경은 Arc::make_mut로
    // 공유 중일 때만 실제 복사. 관찰되는 의미는 깊은 복사와 동일.
    List(Arc<Vec<VmValue>>),
    Map(Arc<HashMap<String, VmValue>>),
    Closure(Arc<VmClosure>),
    Function(Arc<CompiledFn>),  // stored in constant pool only; not user-visible
    Builtin(usize),
    Channel(Arc<BangChannel>),
    Iter(Arc<Mutex<VmIter>>),   // internal — for for-loop iteration
    Future(Arc<VmFuture>),      // spawn 결과
}

// Safety: all shared mutable state is behind Arc<Mutex<>> or Arc<BangChannel>
unsafe impl Send for VmValue {}
unsafe impl Sync for VmValue {}

#[derive(Debug)]
pub enum VmIter {
    List { items: Arc<Vec<VmValue>>, idx: usize },
    Channel(Arc<BangChannel>),
}

impl VmValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            VmValue::Int(_)      => "Int",
            VmValue::Float(_)    => "Float",
            VmValue::Bool(_)     => "Bool",
            VmValue::Str(_)      => "Str",
            VmValue::Nil         => "Nil",
            VmValue::List(_)     => "List",
            VmValue::Map(_)      => "Map",
            VmValue::Closure(_)  => "Function",
            VmValue::Function(_) => "Function",
            VmValue::Builtin(_)  => "Builtin",
            VmValue::Channel(_)  => "Channel",
            VmValue::Iter(_)     => "Iter",
            VmValue::Future(_)   => "Future",
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, VmValue::Bool(false) | VmValue::Nil)
    }
}

impl fmt::Display for VmValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmValue::Int(n)    => write!(f, "{n}"),
            VmValue::Float(n)  => write!(f, "{n}"),
            VmValue::Bool(b)   => write!(f, "{b}"),
            VmValue::Str(s)    => write!(f, "{s}"),
            VmValue::Nil       => write!(f, "nil"),
            VmValue::List(v) => {
                write!(f, "[")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            VmValue::Map(m) => {
                write!(f, "{{")?;
                let mut pairs: Vec<_> = m.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            VmValue::Closure(c) => {
                if let Some(n) = &c.func.name { write!(f, "<fn {n}>") }
                else { write!(f, "<fn>") }
            }
            VmValue::Function(func) => {
                if let Some(n) = &func.name { write!(f, "<fn {n}>") }
                else { write!(f, "<fn>") }
            }
            VmValue::Builtin(i)  => write!(f, "<builtin {}>", BUILTINS.get(*i).unwrap_or(&"?")),
            VmValue::Channel(_)  => write!(f, "<channel>"),
            VmValue::Iter(_)     => write!(f, "<iter>"),
            VmValue::Future(_)   => write!(f, "<future>"),
        }
    }
}

// ============================================================================
// Chunk — compiled bytecode
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub constants: Vec<VmValue>,
    pub spans: Vec<Span>,   // parallel to code bytes
}

impl Chunk {
    pub fn new() -> Self { Self::default() }

    pub fn emit(&mut self, byte: u8, span: Span) -> usize {
        let pos = self.code.len();
        self.code.push(byte);
        self.spans.push(span);
        pos
    }

    pub fn emit_u8(&mut self, op: u8, operand: u8, span: Span) {
        self.emit(op, span);
        self.emit(operand, span);
    }

    pub fn emit_u16(&mut self, op: u8, operand: u16, span: Span) {
        self.emit(op, span);
        self.emit((operand & 0xff) as u8, span);
        self.emit((operand >> 8) as u8, span);
    }

    pub fn emit_i16(&mut self, op: u8, operand: i16, span: Span) {
        self.emit_u16(op, operand as u16, span);
    }

    /// Emit a jump with placeholder; returns position of the i16 operand.
    pub fn emit_jump(&mut self, op: u8, span: Span) -> usize {
        self.emit(op, span);
        let pos = self.code.len();
        self.emit(0xff, span);
        self.emit(0xff, span);
        pos
    }

    /// Patch a previously emitted jump to target the current end of code.
    pub fn patch_jump(&mut self, pos: usize) {
        let offset = (self.code.len() as i64 - pos as i64 - 2) as i16;
        self.code[pos]     = (offset as u16 & 0xff) as u8;
        self.code[pos + 1] = (offset as u16 >> 8) as u8;
    }

    pub fn add_constant(&mut self, val: VmValue) -> u8 {
        let idx = self.constants.len();
        self.constants.push(val);
        idx as u8
    }

    pub fn current_pos(&self) -> usize { self.code.len() }
}

// ============================================================================
// CompiledFn
// ============================================================================

#[derive(Debug, Clone)]
pub struct CompiledFn {
    pub name: Option<String>,
    pub arity: usize,
    pub chunk: Chunk,
    pub upvalue_count: usize,
    pub local_count: usize,
}

// ============================================================================
// Upvalue — shared mutable slot via Arc<Mutex<>>
// ============================================================================

pub struct Upvalue {
    pub locals: Arc<Mutex<Vec<VmValue>>>,
    pub slot: usize,
}

impl Upvalue {
    pub fn get(&self) -> VmValue {
        self.locals.lock().unwrap()[self.slot].clone()
    }
    pub fn set(&self, v: VmValue) {
        self.locals.lock().unwrap()[self.slot] = v;
    }
}

pub type UpvalueRef = Arc<Upvalue>;

// ============================================================================
// VmClosure
// ============================================================================

pub struct VmClosure {
    pub func: Arc<CompiledFn>,
    pub upvalues: Vec<UpvalueRef>,
    /// 이 클로저가 속한 모듈의 전역 배열. import된 모듈의 함수는
    /// 자기 모듈 전역을 들고 다니므로, 호출하는 VM이 달라도(메인 VM 등)
    /// OP_LOAD_GLOBAL/OP_STORE_GLOBAL이 올바른 모듈 전역을 가리킨다.
    pub globals: Arc<Mutex<Vec<VmValue>>>,
}

impl fmt::Debug for VmClosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VmClosure({:?})", self.func.name)
    }
}

// ============================================================================
// Builtin table (must match resolver.rs register_builtins order)
// ============================================================================

pub const BUILTINS: &[&str] = &[
    // 기본 (0-19)
    "print",        // 0
    "str",          // 1
    "int",          // 2
    "float",        // 3
    "bool",         // 4
    "len",          // 5
    "type",         // 6
    "channel",      // 7
    "send",         // 8
    "recv",         // 9
    "close",        // 10
    "parallel_map", // 11
    "wait",         // 12
    "push",         // 13
    "pop",          // 14
    "keys",         // 15
    "values",       // 16
    "range",        // 17
    "assert",       // 18
    "exit",         // 19
    // 문자열 (20-32)
    "split",        // 20
    "join",         // 21
    "trim",         // 22
    "trim_start",   // 23
    "trim_end",     // 24
    "replace",      // 25
    "contains",     // 26
    "starts_with",  // 27
    "ends_with",    // 28
    "upper",        // 29
    "lower",        // 30
    "find",         // 31
    "chars",        // 32
    // 리스트 (33-43)
    "sort",         // 33
    "reverse",      // 34
    "map",          // 35
    "filter",       // 36
    "reduce",       // 37
    "any",          // 38
    "all",          // 39
    "flat",         // 40
    "enumerate",    // 41
    "zip",          // 42
    "sum",          // 43
    // 수학 (44-51)
    "abs",          // 44
    "sqrt",         // 45
    "floor",        // 46
    "ceil",         // 47
    "round",        // 48
    "pow",          // 49
    "min",          // 50
    "max",          // 51
    // I/O (52-56)
    "read_file",    // 52
    "write_file",   // 53
    "input",        // 54
    "print_err",    // 55
    "args",         // 56
    // 모듈 (57)
    "import",       // 57
];

pub fn builtin_index(name: &str) -> Option<usize> {
    BUILTINS.iter().position(|&b| b == name)
}

// ============================================================================
// CallFrame
// ============================================================================

pub struct CallFrame {
    pub closure: Arc<VmClosure>,
    pub ip: usize,
    pub locals: Arc<Mutex<Vec<VmValue>>>,
}

/// try/catch 예외 핸들러. OP_SETUP_TRY 시점의 상태를 기록해 두었다가
/// 예외 발생 시 그 지점으로 되감는다.
#[derive(Clone)]
pub struct TryHandler {
    /// 핸들러 설정 시점의 프레임 수 (이 깊이까지 프레임을 되감는다).
    pub frame_depth: usize,
    /// 핸들러 설정 시점의 스택 높이 (이 높이로 자른 뒤 예외값을 push).
    pub stack_len: usize,
    /// catch 블록의 절대 코드 위치.
    pub catch_ip: usize,
}

// ============================================================================
// Vm
// ============================================================================

pub struct Vm {
    pub stack: Vec<VmValue>,
    pub frames: Vec<CallFrame>,
    /// 루트(메인) 모듈의 전역 배열. 실행 중 전역 접근은 현재 프레임
    /// 클로저의 globals를 쓰며, 이 필드는 루트 클로저 globals와 같은 Arc다.
    pub globals: Arc<Mutex<Vec<VmValue>>>,
    pub output: Arc<Mutex<Vec<String>>>,
    /// 구조적 동시성: spawn 스코프 스택.
    /// 각 항목은 이 스코프 안에서 spawn된 Future 목록.
    /// parallel {} 진입 시 push, 종료 시 pop + join.
    pub spawn_scopes: Vec<Vec<Arc<VmFuture>>>,
    /// 활성 try/catch 핸들러 스택 (innermost = top).
    pub handlers: Vec<TryHandler>,
    /// throw로 던져진 값(있으면). 빌트인/런타임 오류는 None → 메시지 문자열로 변환.
    pub pending_exception: Option<VmValue>,
}

impl Vm {
    pub fn new(global_count: usize, output: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            stack: Vec::with_capacity(256),
            frames: Vec::with_capacity(64),
            globals: Arc::new(Mutex::new(vec![VmValue::Nil; global_count])),
            output,
            spawn_scopes: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
        }
    }

    pub fn run(&mut self, main_fn: Arc<CompiledFn>) -> Result<(), RuntimeError> {
        // 프로그램 레벨 spawn 스코프
        self.spawn_scopes.push(Vec::new());

        let closure = Arc::new(VmClosure {
            func: main_fn.clone(),
            upvalues: Vec::new(),
            globals: self.globals.clone(),
        });
        let locals = Arc::new(Mutex::new(vec![VmValue::Nil; main_fn.local_count]));
        self.frames.push(CallFrame { closure, ip: 0, locals });
        self.exec_until(0)?;

        // 프로그램 종료 시 모든 잔여 spawn 조인 (누수 방지)
        let scope = self.spawn_scopes.pop().unwrap_or_default();
        for f in scope { let _ = f.resolve(); }

        Ok(())
    }

    /// spawned 클로저를 서브-VM에서 실행하고 결과 반환.
    /// std::thread::spawn 클로저 안에서 호출된다.
    pub fn run_spawned(
        output: Arc<Mutex<Vec<String>>>,
        closure: Arc<VmClosure>,
        args: Vec<VmValue>,
    ) -> Result<VmValue, RuntimeError> {
        // spawned 클로저는 이미 자기 모듈 전역의 독립 복사본을 들고 있다
        // (deep_clone_closure에서 깊은 복사). 그 Arc를 루트 전역으로 쓴다.
        let mut vm = Vm {
            stack: Vec::with_capacity(64),
            frames: Vec::with_capacity(16),
            globals: closure.globals.clone(),
            output,
            spawn_scopes: vec![Vec::new()],
            handlers: Vec::new(),
            pending_exception: None,
        };
        let local_count = closure.func.local_count;
        let mut locals_vec = vec![VmValue::Nil; local_count];
        for (i, arg) in args.into_iter().enumerate() {
            if i < locals_vec.len() { locals_vec[i] = arg; }
        }
        let locals = Arc::new(Mutex::new(locals_vec));
        vm.frames.push(CallFrame { closure, ip: 0, locals });
        vm.exec_until(0)?;

        // 서브-VM 내 잔여 spawn 조인
        let scope = vm.spawn_scopes.pop().unwrap_or_default();
        for f in scope { let _ = f.resolve(); }

        Ok(vm.stack.pop().unwrap_or(VmValue::Nil))
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn read_byte(&mut self) -> u8 {
        let fi = self.frames.len() - 1;
        let b = self.frames[fi].closure.func.chunk.code[self.frames[fi].ip];
        self.frames[fi].ip += 1;
        b
    }

    fn read_u16(&mut self) -> u16 {
        let lo = self.read_byte() as u16;
        let hi = self.read_byte() as u16;
        lo | (hi << 8)
    }

    fn read_i16(&mut self) -> i16 { self.read_u16() as i16 }

    fn get_constant(&self, idx: u8) -> VmValue {
        let fi = self.frames.len() - 1;
        self.frames[fi].closure.func.chunk.constants[idx as usize].clone()
    }

    fn current_span(&self) -> Span {
        let fi = self.frames.len() - 1;
        let ip = self.frames[fi].ip.saturating_sub(1);
        let spans = &self.frames[fi].closure.func.chunk.spans;
        if ip < spans.len() { spans[ip] } else { Span::new(0, 0) }
    }

    fn stack_pop(&mut self) -> VmValue {
        self.stack.pop().expect("VM: empty stack on pop")
    }

    fn stack_peek(&self) -> &VmValue {
        self.stack.last().expect("VM: empty stack on peek")
    }

    fn locals_get(&self, slot: usize) -> VmValue {
        let fi = self.frames.len() - 1;
        self.frames[fi].locals.lock().unwrap()[slot].clone()
    }

    fn locals_set(&self, slot: usize, val: VmValue) {
        let fi = self.frames.len() - 1;
        self.frames[fi].locals.lock().unwrap()[slot] = val;
    }

    fn jump_by(&mut self, offset: i16) {
        let fi = self.frames.len() - 1;
        self.frames[fi].ip = (self.frames[fi].ip as isize + offset as isize) as usize;
    }

    // -----------------------------------------------------------------------
    // Dispatch loop: runs until frames.len() <= stop_depth
    // stop_depth=0 means run until all frames complete (full program).
    // stop_depth=N means run until the call at depth N returns.
    // -----------------------------------------------------------------------

    /// 예외 처리 래퍼: exec_dispatch가 Err를 내면, 이 스코프(stop_depth) 안의
    /// 핸들러가 있으면 되감아 catch에서 재개하고, 없으면 Err를 전파한다.
    fn exec_until(&mut self, stop_depth: usize) -> Result<(), RuntimeError> {
        loop {
            match self.exec_dispatch(stop_depth) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if let Some(h) = self.take_handler_above(stop_depth) {
                        let exc = self.pending_exception.take()
                            .unwrap_or_else(|| VmValue::Str(e.message.clone()));
                        self.unwind_to(&h, exc);
                        // 루프 계속 → catch 블록에서 재개
                    } else if let Some(v) = self.pending_exception.take() {
                        // 잡히지 않은 사용자 throw → 던진 값을 메시지에 표시
                        return Err(RuntimeError::new(
                            format!("잡히지 않은 예외: {v}"), e.span));
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    /// 이 스코프(stop_depth보다 깊은 프레임에 설정된) 가장 안쪽 핸들러를 꺼낸다.
    fn take_handler_above(&mut self, stop_depth: usize) -> Option<TryHandler> {
        match self.handlers.last() {
            Some(h) if h.frame_depth > stop_depth => self.handlers.pop(),
            _ => None,
        }
    }

    /// 핸들러 지점으로 되감기: 프레임/스택 정리 후 예외값을 스택에 올리고 catch로 점프.
    fn unwind_to(&mut self, h: &TryHandler, exc: VmValue) {
        while self.frames.len() > h.frame_depth {
            self.frames.pop();
        }
        // 되감긴 프레임들에 남아있던 더 깊은 핸들러 제거
        self.handlers.retain(|x| x.frame_depth <= self.frames.len());
        if self.stack.len() > h.stack_len {
            self.stack.truncate(h.stack_len);
        }
        self.stack.push(exc);
        if let Some(frame) = self.frames.last_mut() {
            frame.ip = h.catch_ip;
        }
    }

    fn exec_dispatch(&mut self, stop_depth: usize) -> Result<(), RuntimeError> {
        loop {
            if self.frames.len() <= stop_depth { return Ok(()); }

            let opcode = {
                let fi = self.frames.len() - 1;
                let frame = &self.frames[fi];
                let ip = frame.ip;
                if ip >= frame.closure.func.chunk.code.len() {
                    // Implicit nil return
                    self.frames.pop();
                    // 빠져나간 프레임에 남은 try 핸들러 정리 (try 안에서의 암묵 반환)
                    self.handlers.retain(|h| h.frame_depth <= self.frames.len());
                    self.stack.push(VmValue::Nil);
                    if self.frames.len() <= stop_depth { return Ok(()); }
                    continue;
                }
                let op = frame.closure.func.chunk.code[ip];
                self.frames[fi].ip += 1;
                op
            };

            match opcode {
                OP_POP  => { self.stack_pop(); }
                OP_NIL  => { self.stack.push(VmValue::Nil); }
                OP_TRUE => { self.stack.push(VmValue::Bool(true)); }
                OP_FALSE=> { self.stack.push(VmValue::Bool(false)); }
                OP_DUP  => {
                    let v = self.stack_peek().clone();
                    self.stack.push(v);
                }

                OP_CONST => {
                    let idx = self.read_byte();
                    let v = self.get_constant(idx);
                    self.stack.push(v);
                }

                // --- Arithmetic (Future 자동 조인) ---
                OP_ADD => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(vm_add(l, r, span)?);
                }
                OP_SUB => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(vm_arith(l, r, span, '-')?);
                }
                OP_MUL => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(vm_arith(l, r, span, '*')?);
                }
                OP_DIV => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(vm_arith(l, r, span, '/')?);
                }
                OP_MOD => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(vm_arith(l, r, span, '%')?);
                }
                OP_NEG => {
                    let span = self.current_span();
                    let v = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(match v {
                        VmValue::Int(n)   => VmValue::Int(-n),
                        VmValue::Float(n) => VmValue::Float(-n),
                        other => return Err(RuntimeError::new(
                            format!("단항 -: 숫자 필요, {} 발견", other.type_name()), span)),
                    });
                }
                OP_NOT => {
                    let span = self.current_span();
                    let v = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(!v.is_truthy()));
                }

                // --- Comparison (Future 자동 조인) ---
                OP_EQ => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(vm_eq(&l, &r)));
                }
                OP_NE => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(!vm_eq(&l, &r)));
                }
                OP_LT => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(vm_cmp(&l, &r, span)? == Ordering::Less));
                }
                OP_LE => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(vm_cmp(&l, &r, span)? != Ordering::Greater));
                }
                OP_GT => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(vm_cmp(&l, &r, span)? == Ordering::Greater));
                }
                OP_GE => {
                    let span = self.current_span();
                    let r = auto_resolve(self.stack_pop(), span)?;
                    let l = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(vm_cmp(&l, &r, span)? != Ordering::Less));
                }

                // --- Jumps ---
                OP_JUMP => {
                    let offset = self.read_i16();
                    self.jump_by(offset);
                }
                OP_JUMP_FALSE => {
                    let span = self.current_span();
                    let offset = self.read_i16();
                    let v = auto_resolve(self.stack_pop(), span)?;
                    if !v.is_truthy() { self.jump_by(offset); }
                }
                OP_JUMP_AND => {
                    let span = self.current_span();
                    let offset = self.read_i16();
                    let top = auto_resolve(self.stack_peek().clone(), span)?;
                    if !top.is_truthy() {
                        self.stack_pop();
                        self.stack.push(top);
                        self.jump_by(offset);
                    }
                }
                OP_JUMP_OR => {
                    let span = self.current_span();
                    let offset = self.read_i16();
                    let top = auto_resolve(self.stack_peek().clone(), span)?;
                    if top.is_truthy() {
                        self.stack_pop();
                        self.stack.push(top);
                        self.jump_by(offset);
                    }
                }

                // --- Locals ---
                OP_LOAD_LOCAL => {
                    let slot = self.read_byte() as usize;
                    self.stack.push(self.locals_get(slot));
                }
                OP_STORE_LOCAL => {
                    let slot = self.read_byte() as usize;
                    let val = self.stack_pop();
                    self.locals_set(slot, val);
                }

                // --- Upvalues ---
                OP_LOAD_UPVAL => {
                    let idx = self.read_byte() as usize;
                    let fi = self.frames.len() - 1;
                    let val = self.frames[fi].closure.upvalues[idx].get();
                    self.stack.push(val);
                }
                OP_STORE_UPVAL => {
                    let idx = self.read_byte() as usize;
                    let val = self.stack_pop();
                    let fi = self.frames.len() - 1;
                    self.frames[fi].closure.upvalues[idx].set(val);
                }

                // --- Globals ---
                OP_LOAD_GLOBAL => {
                    let slot = self.read_u16() as usize;
                    let fi = self.frames.len() - 1;
                    let g_arc = self.frames[fi].closure.globals.clone();
                    let v = g_arc.lock().unwrap()[slot].clone();
                    self.stack.push(v);
                }
                OP_STORE_GLOBAL => {
                    let slot = self.read_u16() as usize;
                    let val = self.stack_pop();
                    let fi = self.frames.len() - 1;
                    let g_arc = self.frames[fi].closure.globals.clone();
                    g_arc.lock().unwrap()[slot] = val;
                }

                // --- Builtins ---
                OP_LOAD_BUILTIN => {
                    let idx = self.read_byte() as usize;
                    self.stack.push(VmValue::Builtin(idx));
                }

                // --- Closure ---
                OP_CLOSURE => {
                    let fn_const_idx = self.read_byte();
                    let uv_count = self.read_byte() as usize;

                    let compiled_fn = match self.get_constant(fn_const_idx) {
                        VmValue::Function(f) => f,
                        other => return Err(RuntimeError::no_span(
                            format!("OP_CLOSURE: Function constant 필요, {} 발견", other.type_name()))),
                    };

                    let mut upvalues: Vec<UpvalueRef> = Vec::with_capacity(uv_count);
                    for _ in 0..uv_count {
                        let is_local = self.read_byte() != 0;
                        let idx = self.read_byte() as usize;
                        let fi = self.frames.len() - 1;
                        if is_local {
                            upvalues.push(Arc::new(Upvalue {
                                locals: self.frames[fi].locals.clone(),
                                slot: idx,
                            }));
                        } else {
                            upvalues.push(self.frames[fi].closure.upvalues[idx].clone());
                        }
                    }

                    let fi = self.frames.len() - 1;
                    let globals = self.frames[fi].closure.globals.clone();
                    let closure = Arc::new(VmClosure { func: compiled_fn, upvalues, globals });
                    self.stack.push(VmValue::Closure(closure));
                }

                // --- Call / Return ---
                OP_CALL => {
                    let arg_count = self.read_byte() as usize;
                    let span = self.current_span();
                    self.do_call(arg_count, span, stop_depth)?;
                }

                OP_RETURN => {
                    let span = self.current_span();
                    let retval = auto_resolve(self.stack_pop(), span)?;
                    self.frames.pop();
                    // 빠져나간 프레임에 남은 try 핸들러 정리 (try 안에서의 return)
                    self.handlers.retain(|h| h.frame_depth <= self.frames.len());
                    self.stack.push(retval);
                    if self.frames.len() <= stop_depth { return Ok(()); }
                }

                // --- Collections ---
                OP_MAKE_LIST => {
                    let count = self.read_u16() as usize;
                    let start = self.stack.len() - count;
                    let items: Vec<VmValue> = self.stack.drain(start..).collect();
                    self.stack.push(VmValue::List(Arc::new(items)));
                }
                OP_MAKE_MAP => {
                    let pair_count = self.read_u16() as usize;
                    let start = self.stack.len() - pair_count * 2;
                    let flat: Vec<VmValue> = self.stack.drain(start..).collect();
                    let mut map = HashMap::new();
                    for pair in flat.chunks(2) {
                        let key = match &pair[0] {
                            VmValue::Str(s) => s.clone(),
                            other => other.to_string(),
                        };
                        map.insert(key, pair[1].clone());
                    }
                    self.stack.push(VmValue::Map(Arc::new(map)));
                }

                OP_INDEX_GET => {
                    let span = self.current_span();
                    let idx = self.stack_pop();
                    let target = self.stack_pop();
                    self.stack.push(vm_index_get(target, idx, span)?);
                }
                OP_INDEX_SET => {
                    // stack: [container, idx, val]  (val on top)
                    let span = self.current_span();
                    let val = self.stack_pop();
                    let idx = self.stack_pop();
                    let container = self.stack_pop();
                    self.stack.push(vm_index_set(container, idx, val, span)?);
                }
                OP_FIELD_GET => {
                    let span = self.current_span();
                    let name_idx = self.read_byte();
                    let name = match self.get_constant(name_idx) {
                        VmValue::Str(s) => s,
                        _ => return Err(RuntimeError::no_span("OP_FIELD_GET: non-string name")),
                    };
                    let target = self.stack_pop();
                    self.stack.push(vm_field_get(target, &name, span)?);
                }

                // --- Spawn / Parallel ---
                OP_SPAWN => {
                    let arg_count = self.read_byte() as usize;
                    let span = self.current_span();
                    let args: Vec<VmValue> =
                        self.stack.drain(self.stack.len() - arg_count..).collect();
                    let callee = self.stack_pop();
                    match callee {
                        VmValue::Closure(closure) => {
                            // 값 의미론: 인자·upvalue·모듈 전역을 spawn 경계에서 복제
                            // (deep_clone_closure가 클로저 전역을 깊은 복사한다)
                            let args_copy = args; // VmValue::clone() 이 올바르게 deep-copy
                            let closure_copy = deep_clone_closure(&closure);
                            let output_copy  = self.output.clone();
                            let future = VmFuture::new();
                            let future2 = future.clone();
                            // M:N 스케줄러에 태스크 제출 (Phase 9 Part A)
                            crate::scheduler::global().spawn_task(move || {
                                let result = Vm::run_spawned(
                                    output_copy, closure_copy, args_copy);
                                future2.complete(result);
                            });
                            // 현재 스코프에 등록
                            if let Some(scope) = self.spawn_scopes.last_mut() {
                                scope.push(future.clone());
                            }
                            self.stack.push(VmValue::Future(future));
                        }
                        other => {
                            return Err(RuntimeError::new(
                                format!("spawn: 클로저 필요, {} 발견", other.type_name()), span));
                        }
                    }
                }
                OP_PARALLEL_ENTER => {
                    self.spawn_scopes.push(Vec::new());
                }
                OP_PARALLEL_EXIT => {
                    let scope = self.spawn_scopes.pop().unwrap_or_default();
                    // 모든 spawn 조인 (구조적 동시성)
                    for f in scope { f.resolve()?; }
                }

                // --- try / catch / throw ---
                OP_SETUP_TRY => {
                    let catch_ip = self.read_u16() as usize; // 절대 위치
                    self.handlers.push(TryHandler {
                        frame_depth: self.frames.len(),
                        stack_len: self.stack.len(),
                        catch_ip,
                    });
                }
                OP_POP_TRY => {
                    // try 본문 정상 종료 → 핸들러 제거
                    self.handlers.pop();
                }
                OP_THROW => {
                    let span = self.current_span();
                    let val = auto_resolve(self.stack_pop(), span)?;
                    // 던진 값을 보관하고 Err로 신호 → exec_until 래퍼가 핸들러로 라우팅
                    self.pending_exception = Some(val);
                    return Err(RuntimeError::new("throw", span));
                }
                OP_CHECK_TYPE => {
                    let tag = self.read_byte();
                    let expected = TypeAnn::from_u8(tag);
                    // 값이 Future면 먼저 해소 후 검사
                    let span = self.current_span();
                    let top = self.stack_pop();
                    let v = auto_resolve(top, span)?;
                    let ok = match expected {
                        Some(TypeAnn::Any) | None => true,
                        Some(t) => value_matches_type(&v, t),
                    };
                    if !ok {
                        let exp = expected.map(|t| t.name()).unwrap_or("?");
                        // 타입 에러는 try/catch로 잡을 수 있는 런타임 에러
                        return Err(RuntimeError::new(
                            format!("타입 불일치: {exp} 기대, {} 받음", v.type_name()), span));
                    }
                    self.stack.push(v);
                }

                // --- For loop ---
                OP_MAKE_ITER => {
                    let span = self.current_span();
                    let val = auto_resolve(self.stack_pop(), span)?;
                    let iter = match val {
                        VmValue::List(items) => VmIter::List { items, idx: 0 },
                        VmValue::Channel(ch) => VmIter::Channel(ch),
                        other => return Err(RuntimeError::new(
                            format!("for-in: List 또는 Channel 필요, {} 발견", other.type_name()),
                            span)),
                    };
                    self.stack.push(VmValue::Iter(Arc::new(Mutex::new(iter))));
                }
                OP_FOR_ITER => {
                    let var_slot = self.read_byte() as usize;
                    let jump_offset = self.read_i16();
                    let next_val = {
                        let iter_val = self.stack.last_mut().expect("FOR_ITER: empty stack");
                        match iter_val {
                            VmValue::Iter(arc) => {
                                let mut it = arc.lock().unwrap();
                                match &mut *it {
                                    VmIter::List { items, idx } => {
                                        if *idx < items.len() {
                                            let v = items[*idx].clone();
                                            *idx += 1;
                                            Some(v)
                                        } else { None }
                                    }
                                    VmIter::Channel(ch) => ch.recv().map(from_runtime),
                                }
                            }
                            _ => return Err(RuntimeError::no_span("FOR_ITER: Iter 필요")),
                        }
                    };
                    match next_val {
                        Some(v) => { self.locals_set(var_slot, v); }
                        None    => {
                            self.stack_pop(); // pop exhausted iterator
                            self.jump_by(jump_offset);
                        }
                    }
                }

                other => {
                    let span = self.current_span();
                    return Err(RuntimeError::new(
                        format!("알 수 없는 opcode: {other}"), span));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Function call
    // -----------------------------------------------------------------------

    fn do_call(&mut self, arg_count: usize, span: Span, stop_depth: usize) -> Result<(), RuntimeError> {
        let callee_idx = self.stack.len() - arg_count - 1;
        let callee = self.stack[callee_idx].clone();

        match callee {
            VmValue::Closure(closure) => {
                if arg_count != closure.func.arity {
                    return Err(RuntimeError::new(
                        format!("인자 개수 불일치: {}개 기대, {}개 전달",
                            closure.func.arity, arg_count),
                        span));
                }
                let args: Vec<VmValue> =
                    self.stack.drain(self.stack.len() - arg_count..).collect();
                self.stack.pop(); // pop callee

                // Phase 9 Part B: JIT 호출 시도 (Int-only 함수에만 적용)
                #[cfg(feature = "jit")]
                if let Some(result) = crate::codegen::jit::try_jit_call(&closure.func, &args, span) {
                    self.stack.push(result?);
                    return Ok(());
                }

                let local_count = closure.func.local_count;
                let mut locals_vec = vec![VmValue::Nil; local_count];
                for (i, arg) in args.into_iter().enumerate() {
                    locals_vec[i] = arg;
                }
                let locals = Arc::new(Mutex::new(locals_vec));
                self.frames.push(CallFrame { closure, ip: 0, locals });
                // Execution continues in exec_until loop
            }

            VmValue::Builtin(idx) => {
                let raw_args: Vec<VmValue> =
                    self.stack.drain(self.stack.len() - arg_count..).collect();
                self.stack.pop(); // pop callee
                // 인자 자동 조인 (Future → 값)
                let mut args = Vec::with_capacity(raw_args.len());
                for a in raw_args { args.push(auto_resolve(a, span)?); }
                let result = self.call_builtin(idx, args, span, stop_depth)?;
                self.stack.push(result);
            }

            other => {
                return Err(RuntimeError::new(
                    format!("호출할 수 없는 값: {}", other.type_name()), span));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Builtin dispatch
    // -----------------------------------------------------------------------

    fn call_builtin(
        &mut self,
        idx: usize,
        args: Vec<VmValue>,
        span: Span,
        stop_depth: usize,
    ) -> Result<VmValue, RuntimeError> {
        match idx {
            0 => { // print — Future 깊이 해소
                let mut parts = Vec::with_capacity(args.len());
                for v in args {
                    let resolved = deep_resolve(v, span)?;
                    parts.push(format!("{resolved}"));
                }
                let line = parts.join(" ");
                self.output.lock().unwrap().push(line.clone());
                println!("{line}");
                Ok(VmValue::Nil)
            }
            1 => { // str(x)
                req_args("str", &args, 1, span)?;
                Ok(VmValue::Str(format!("{}", args[0])))
            }
            2 => { // int(x)
                req_args("int", &args, 1, span)?;
                match &args[0] {
                    VmValue::Int(n)   => Ok(VmValue::Int(*n)),
                    VmValue::Float(n) => Ok(VmValue::Int(*n as i64)),
                    VmValue::Bool(b)  => Ok(VmValue::Int(if *b { 1 } else { 0 })),
                    VmValue::Str(s)   => s.trim().parse::<i64>()
                        .map(VmValue::Int)
                        .map_err(|_| RuntimeError::new(format!("int 변환 실패: '{s}'"), span)),
                    other => Err(RuntimeError::new(
                        format!("int(): {} 변환 불가", other.type_name()), span)),
                }
            }
            3 => { // float(x)
                req_args("float", &args, 1, span)?;
                match &args[0] {
                    VmValue::Float(n) => Ok(VmValue::Float(*n)),
                    VmValue::Int(n)   => Ok(VmValue::Float(*n as f64)),
                    VmValue::Str(s)   => s.trim().parse::<f64>()
                        .map(VmValue::Float)
                        .map_err(|_| RuntimeError::new(format!("float 변환 실패: '{s}'"), span)),
                    other => Err(RuntimeError::new(
                        format!("float(): {} 변환 불가", other.type_name()), span)),
                }
            }
            4 => { // bool(x)
                req_args("bool", &args, 1, span)?;
                Ok(VmValue::Bool(args[0].is_truthy()))
            }
            5 => { // len(x)
                req_args("len", &args, 1, span)?;
                match &args[0] {
                    VmValue::List(v) => Ok(VmValue::Int(v.len() as i64)),
                    VmValue::Str(s)  => Ok(VmValue::Int(s.chars().count() as i64)),
                    VmValue::Map(m)  => Ok(VmValue::Int(m.len() as i64)),
                    other => Err(RuntimeError::new(
                        format!("len(): {} 지원 안 됨", other.type_name()), span)),
                }
            }
            6 => { // type(x)
                req_args("type", &args, 1, span)?;
                Ok(VmValue::Str(args[0].type_name().to_string()))
            }
            7 => { // channel([cap])
                let cap = args.first().and_then(|v| match v {
                    VmValue::Int(n) => Some(*n as usize),
                    _ => None,
                }).unwrap_or(0);
                Ok(VmValue::Channel(Arc::new(BangChannel::new(cap))))
            }
            8 => { // send(ch, val)
                req_args("send", &args, 2, span)?;
                match &args[0] {
                    VmValue::Channel(ch) => {
                        ch.send(to_runtime(&args[1]))
                            .map_err(|e| RuntimeError::new(e.message, span))?;
                        Ok(VmValue::Nil)
                    }
                    other => Err(RuntimeError::new(
                        format!("send(): Channel 필요, {} 발견", other.type_name()), span)),
                }
            }
            9 => { // recv(ch)
                req_args("recv", &args, 1, span)?;
                match &args[0] {
                    VmValue::Channel(ch) => Ok(ch.recv().map(from_runtime).unwrap_or(VmValue::Nil)),
                    other => Err(RuntimeError::new(
                        format!("recv(): Channel 필요, {} 발견", other.type_name()), span)),
                }
            }
            10 => { // close(ch)
                req_args("close", &args, 1, span)?;
                match &args[0] {
                    VmValue::Channel(ch) => { ch.close(); Ok(VmValue::Nil) }
                    other => Err(RuntimeError::new(
                        format!("close(): Channel 필요, {} 발견", other.type_name()), span)),
                }
            }
            11 => { // parallel_map(list, fn) — sequential in Part A
                req_args("parallel_map", &args, 2, span)?;
                let list = match &args[0] {
                    VmValue::List(v) => (**v).clone(),
                    other => return Err(RuntimeError::new(
                        format!("parallel_map(): List 필요, {} 발견", other.type_name()), span)),
                };
                let func = args[1].clone();
                let mut results = Vec::with_capacity(list.len());
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(item);
                    self.do_call(1, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        // User function: run until it returns
                        self.exec_until(depth_before)?;
                    }
                    // Return value is on top of stack
                    results.push(self.stack_pop());
                }
                Ok(VmValue::List(Arc::new(results)))
            }
            12 => { // wait(f) — Part A: value is already resolved
                req_args("wait", &args, 1, span)?;
                Ok(args[0].clone())
            }
            13 => { // push(list, val)
                req_args("push", &args, 2, span)?;
                match args[0].clone() {
                    VmValue::List(mut v) => {
                        Arc::make_mut(&mut v).push(args[1].clone());
                        Ok(VmValue::List(v))
                    }
                    other => Err(RuntimeError::new(
                        format!("push(): List 필요, {} 발견", other.type_name()), span)),
                }
            }
            14 => { // pop(list)
                req_args("pop", &args, 1, span)?;
                match args[0].clone() {
                    VmValue::List(v) => Ok(v.last().cloned().unwrap_or(VmValue::Nil)),
                    other => Err(RuntimeError::new(
                        format!("pop(): List 필요, {} 발견", other.type_name()), span)),
                }
            }
            15 => { // keys(map)
                req_args("keys", &args, 1, span)?;
                match &args[0] {
                    VmValue::Map(m) => {
                        let mut keys: Vec<VmValue> =
                            m.keys().map(|k| VmValue::Str(k.clone())).collect();
                        keys.sort_by_key(|a| a.to_string());
                        Ok(VmValue::List(Arc::new(keys)))
                    }
                    other => Err(RuntimeError::new(
                        format!("keys(): Map 필요, {} 발견", other.type_name()), span)),
                }
            }
            16 => { // values(map)
                req_args("values", &args, 1, span)?;
                match &args[0] {
                    VmValue::Map(m) => {
                        let mut pairs: Vec<_> = m.iter().collect();
                        pairs.sort_by_key(|(k, _)| k.as_str());
                        Ok(VmValue::List(Arc::new(pairs.into_iter().map(|(_, v)| v.clone()).collect())))
                    }
                    other => Err(RuntimeError::new(
                        format!("values(): Map 필요, {} 발견", other.type_name()), span)),
                }
            }
            17 => { // range
                match args.len() {
                    1 => {
                        let n = as_int(&args[0], span)?;
                        Ok(VmValue::List(Arc::new((0..n).map(VmValue::Int).collect())))
                    }
                    2 => {
                        let s = as_int(&args[0], span)?;
                        let e = as_int(&args[1], span)?;
                        Ok(VmValue::List(Arc::new((s..e).map(VmValue::Int).collect())))
                    }
                    3 => {
                        let s    = as_int(&args[0], span)?;
                        let e    = as_int(&args[1], span)?;
                        let step = as_int(&args[2], span)?;
                        if step == 0 {
                            return Err(RuntimeError::new("range: step는 0이 될 수 없음", span));
                        }
                        let mut v = Vec::new();
                        let mut i = s;
                        while (step > 0 && i < e) || (step < 0 && i > e) {
                            v.push(VmValue::Int(i));
                            i += step;
                        }
                        Ok(VmValue::List(Arc::new(v)))
                    }
                    _ => Err(RuntimeError::new("range: 인자 1~3개 필요", span)),
                }
            }
            18 => { // assert
                if args.is_empty() {
                    return Err(RuntimeError::new("assert: 인자 필요", span));
                }
                if !args[0].is_truthy() {
                    let msg = args.get(1).map(|v| format!("{v}")).unwrap_or_default();
                    return Err(RuntimeError::new(
                        format!("assertion 실패{}", if msg.is_empty() { String::new() }
                                else { format!(": {msg}") }),
                        span));
                }
                Ok(VmValue::Nil)
            }
            19 => { // exit
                let code = args.first().and_then(|v| match v {
                    VmValue::Int(n) => Some(*n as i32),
                    _ => None,
                }).unwrap_or(0);
                std::process::exit(code);
            }
            // ── 문자열 (20-32) ──────────────────────────────────────────────────
            20 => { // split(s, sep)
                req_args("split", &args, 2, span)?;
                let s = str_arg("split", &args[0], span)?;
                let sep = str_arg("split", &args[1], span)?;
                Ok(VmValue::List(Arc::new(s.split(sep.as_str()).map(|p| VmValue::Str(p.to_string())).collect())))
            }
            21 => { // join(list, sep)
                req_args("join", &args, 2, span)?;
                let list = list_arg("join", &args[0], span)?;
                let sep = str_arg("join", &args[1], span)?;
                let parts: Vec<String> = list.iter().map(|v| format!("{v}")).collect();
                Ok(VmValue::Str(parts.join(&sep)))
            }
            22 => { // trim(s)
                req_args("trim", &args, 1, span)?;
                Ok(VmValue::Str(str_arg("trim", &args[0], span)?.trim().to_string()))
            }
            23 => { // trim_start(s)
                req_args("trim_start", &args, 1, span)?;
                Ok(VmValue::Str(str_arg("trim_start", &args[0], span)?.trim_start().to_string()))
            }
            24 => { // trim_end(s)
                req_args("trim_end", &args, 1, span)?;
                Ok(VmValue::Str(str_arg("trim_end", &args[0], span)?.trim_end().to_string()))
            }
            25 => { // replace(s, from, to)
                req_args("replace", &args, 3, span)?;
                let s    = str_arg("replace", &args[0], span)?;
                let from = str_arg("replace", &args[1], span)?;
                let to   = str_arg("replace", &args[2], span)?;
                Ok(VmValue::Str(s.replace(from.as_str(), &to)))
            }
            26 => { // contains(s, sub)
                req_args("contains", &args, 2, span)?;
                let s   = str_arg("contains", &args[0], span)?;
                let sub = str_arg("contains", &args[1], span)?;
                Ok(VmValue::Bool(s.contains(sub.as_str())))
            }
            27 => { // starts_with(s, prefix)
                req_args("starts_with", &args, 2, span)?;
                let s   = str_arg("starts_with", &args[0], span)?;
                let pre = str_arg("starts_with", &args[1], span)?;
                Ok(VmValue::Bool(s.starts_with(pre.as_str())))
            }
            28 => { // ends_with(s, suffix)
                req_args("ends_with", &args, 2, span)?;
                let s   = str_arg("ends_with", &args[0], span)?;
                let suf = str_arg("ends_with", &args[1], span)?;
                Ok(VmValue::Bool(s.ends_with(suf.as_str())))
            }
            29 => { // upper(s)
                req_args("upper", &args, 1, span)?;
                Ok(VmValue::Str(str_arg("upper", &args[0], span)?.to_uppercase()))
            }
            30 => { // lower(s)
                req_args("lower", &args, 1, span)?;
                Ok(VmValue::Str(str_arg("lower", &args[0], span)?.to_lowercase()))
            }
            31 => { // find(s, sub) → Int (-1 if not found)
                req_args("find", &args, 2, span)?;
                let s   = str_arg("find", &args[0], span)?;
                let sub = str_arg("find", &args[1], span)?;
                let idx = s.find(sub.as_str())
                    .map(|b| s[..b].chars().count() as i64)
                    .unwrap_or(-1);
                Ok(VmValue::Int(idx))
            }
            32 => { // chars(s) → List of single-char strings
                req_args("chars", &args, 1, span)?;
                let s = str_arg("chars", &args[0], span)?;
                Ok(VmValue::List(Arc::new(s.chars().map(|c| VmValue::Str(c.to_string())).collect())))
            }

            // ── 리스트 (33-43) ──────────────────────────────────────────────────
            33 => { // sort(list) → sorted copy (numbers or strings)
                req_args("sort", &args, 1, span)?;
                let mut list = list_arg("sort", &args[0], span)?;
                let mut err: Option<RuntimeError> = None;
                list.sort_by(|a, b| {
                    if err.is_some() { return std::cmp::Ordering::Equal; }
                    match vm_cmp(a, b, span) {
                        Ok(o) => o,
                        Err(e) => { err = Some(e); std::cmp::Ordering::Equal }
                    }
                });
                if let Some(e) = err { return Err(e); }
                Ok(VmValue::List(Arc::new(list)))
            }
            34 => { // reverse(list) → reversed copy
                req_args("reverse", &args, 1, span)?;
                let mut list = list_arg("reverse", &args[0], span)?;
                list.reverse();
                Ok(VmValue::List(Arc::new(list)))
            }
            35 => { // map(list, fn) → List
                req_args("map", &args, 2, span)?;
                let list = list_arg("map", &args[0], span)?;
                let func = args[1].clone();
                let mut results = Vec::with_capacity(list.len());
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(item);
                    self.do_call(1, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        self.exec_until(depth_before)?;
                    }
                    results.push(self.stack_pop());
                }
                Ok(VmValue::List(Arc::new(results)))
            }
            36 => { // filter(list, fn) → List
                req_args("filter", &args, 2, span)?;
                let list = list_arg("filter", &args[0], span)?;
                let func = args[1].clone();
                let mut results = Vec::new();
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(item.clone());
                    self.do_call(1, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        self.exec_until(depth_before)?;
                    }
                    if self.stack_pop().is_truthy() { results.push(item); }
                }
                Ok(VmValue::List(Arc::new(results)))
            }
            37 => { // reduce(list, fn, init) → value
                req_args("reduce", &args, 3, span)?;
                let list = list_arg("reduce", &args[0], span)?;
                let func = args[1].clone();
                let mut acc = args[2].clone();
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(acc);
                    self.stack.push(item);
                    self.do_call(2, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        self.exec_until(depth_before)?;
                    }
                    acc = self.stack_pop();
                }
                Ok(acc)
            }
            38 => { // any(list, fn) → Bool
                req_args("any", &args, 2, span)?;
                let list = list_arg("any", &args[0], span)?;
                let func = args[1].clone();
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(item);
                    self.do_call(1, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        self.exec_until(depth_before)?;
                    }
                    if self.stack_pop().is_truthy() { return Ok(VmValue::Bool(true)); }
                }
                Ok(VmValue::Bool(false))
            }
            39 => { // all(list, fn) → Bool
                req_args("all", &args, 2, span)?;
                let list = list_arg("all", &args[0], span)?;
                let func = args[1].clone();
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(item);
                    self.do_call(1, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        self.exec_until(depth_before)?;
                    }
                    if !self.stack_pop().is_truthy() { return Ok(VmValue::Bool(false)); }
                }
                Ok(VmValue::Bool(true))
            }
            40 => { // flat(list) → flattened one level
                req_args("flat", &args, 1, span)?;
                let list = list_arg("flat", &args[0], span)?;
                let mut out = Vec::new();
                for item in list {
                    match item {
                        VmValue::List(inner) => out.extend(inner.iter().cloned()),
                        other => out.push(other),
                    }
                }
                Ok(VmValue::List(Arc::new(out)))
            }
            41 => { // enumerate(list) → List of [i, val]
                req_args("enumerate", &args, 1, span)?;
                let list = list_arg("enumerate", &args[0], span)?;
                let out = list.into_iter().enumerate()
                    .map(|(i, v)| VmValue::List(Arc::new(vec![VmValue::Int(i as i64), v])))
                    .collect();
                Ok(VmValue::List(Arc::new(out)))
            }
            42 => { // zip(list1, list2) → List of [a, b]
                req_args("zip", &args, 2, span)?;
                let l1 = list_arg("zip", &args[0], span)?;
                let l2 = list_arg("zip", &args[1], span)?;
                let out = l1.into_iter().zip(l2)
                    .map(|(a, b)| VmValue::List(Arc::new(vec![a, b])))
                    .collect();
                Ok(VmValue::List(Arc::new(out)))
            }
            43 => { // sum(list) → number
                req_args("sum", &args, 1, span)?;
                let list = list_arg("sum", &args[0], span)?;
                let mut total_i = 0i64;
                let mut total_f = 0.0f64;
                let mut has_float = false;
                for v in list {
                    match v {
                        VmValue::Int(n)   => total_i += n,
                        VmValue::Float(n) => { total_f += n; has_float = true; }
                        other => return Err(RuntimeError::new(
                            format!("sum(): 숫자 리스트 필요, {} 발견", other.type_name()), span)),
                    }
                }
                if has_float {
                    Ok(VmValue::Float(total_i as f64 + total_f))
                } else {
                    Ok(VmValue::Int(total_i))
                }
            }

            // ── 수학 (44-51) ────────────────────────────────────────────────────
            44 => { // abs(x)
                req_args("abs", &args, 1, span)?;
                match &args[0] {
                    VmValue::Int(n)   => Ok(VmValue::Int(n.abs())),
                    VmValue::Float(n) => Ok(VmValue::Float(n.abs())),
                    other => Err(RuntimeError::new(format!("abs(): 숫자 필요, {} 발견", other.type_name()), span)),
                }
            }
            45 => { // sqrt(x)
                req_args("sqrt", &args, 1, span)?;
                let n = as_float(&args[0], span)?;
                Ok(VmValue::Float(n.sqrt()))
            }
            46 => { // floor(x)
                req_args("floor", &args, 1, span)?;
                let n = as_float(&args[0], span)?;
                Ok(VmValue::Int(n.floor() as i64))
            }
            47 => { // ceil(x)
                req_args("ceil", &args, 1, span)?;
                let n = as_float(&args[0], span)?;
                Ok(VmValue::Int(n.ceil() as i64))
            }
            48 => { // round(x)
                req_args("round", &args, 1, span)?;
                let n = as_float(&args[0], span)?;
                Ok(VmValue::Int(n.round() as i64))
            }
            49 => { // pow(base, exp)
                req_args("pow", &args, 2, span)?;
                let base = as_float(&args[0], span)?;
                let exp  = as_float(&args[1], span)?;
                let result = base.powf(exp);
                if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                    if let (VmValue::Int(_), VmValue::Int(e)) = (&args[0], &args[1]) {
                        if *e >= 0 { return Ok(VmValue::Int(result as i64)); }
                    }
                }
                Ok(VmValue::Float(result))
            }
            50 => { // min(a, b) or min(list)
                match args.len() {
                    1 => {
                        let list = list_arg("min", &args[0], span)?;
                        if list.is_empty() {
                            return Err(RuntimeError::new("min(): 빈 리스트", span));
                        }
                        let mut m = list[0].clone();
                        for v in &list[1..] {
                            if vm_cmp(v, &m, span)? == std::cmp::Ordering::Less { m = v.clone(); }
                        }
                        Ok(m)
                    }
                    2 => {
                        let a = args[0].clone();
                        let b = args[1].clone();
                        if vm_cmp(&a, &b, span)? == std::cmp::Ordering::Less { Ok(a) } else { Ok(b) }
                    }
                    _ => Err(RuntimeError::new("min(): 인자 1개(리스트) 또는 2개 필요", span)),
                }
            }
            51 => { // max(a, b) or max(list)
                match args.len() {
                    1 => {
                        let list = list_arg("max", &args[0], span)?;
                        if list.is_empty() {
                            return Err(RuntimeError::new("max(): 빈 리스트", span));
                        }
                        let mut m = list[0].clone();
                        for v in &list[1..] {
                            if vm_cmp(v, &m, span)? == std::cmp::Ordering::Greater { m = v.clone(); }
                        }
                        Ok(m)
                    }
                    2 => {
                        let a = args[0].clone();
                        let b = args[1].clone();
                        if vm_cmp(&a, &b, span)? == std::cmp::Ordering::Greater { Ok(a) } else { Ok(b) }
                    }
                    _ => Err(RuntimeError::new("max(): 인자 1개(리스트) 또는 2개 필요", span)),
                }
            }

            // ── I/O (52-56) ─────────────────────────────────────────────────────
            52 => { // read_file(path) → Str
                req_args("read_file", &args, 1, span)?;
                let path = str_arg("read_file", &args[0], span)?;
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| RuntimeError::new(format!("read_file(): '{path}': {e}"), span))?;
                Ok(VmValue::Str(content))
            }
            53 => { // write_file(path, content)
                req_args("write_file", &args, 2, span)?;
                let path    = str_arg("write_file", &args[0], span)?;
                let content = str_arg("write_file", &args[1], span)?;
                std::fs::write(&path, content.as_bytes())
                    .map_err(|e| RuntimeError::new(format!("write_file(): '{path}': {e}"), span))?;
                Ok(VmValue::Nil)
            }
            54 => { // input(prompt?) → Str
                if args.len() > 1 {
                    return Err(RuntimeError::new("input(): 인자 0 또는 1개 필요", span));
                }
                if let Some(prompt) = args.first() {
                    print!("{prompt}");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)
                    .map_err(|e| RuntimeError::new(format!("input(): {e}"), span))?;
                Ok(VmValue::Str(line.trim_end_matches('\n').trim_end_matches('\r').to_string()))
            }
            55 => { // print_err(...) → nil
                let mut parts = Vec::with_capacity(args.len());
                for v in args {
                    let resolved = deep_resolve(v, span)?;
                    parts.push(format!("{resolved}"));
                }
                eprintln!("{}", parts.join(" "));
                Ok(VmValue::Nil)
            }
            56 => { // args() → List of CLI args
                let cli_args: Vec<VmValue> = std::env::args()
                    .map(VmValue::Str)
                    .collect();
                Ok(VmValue::List(Arc::new(cli_args)))
            }

            // ── 모듈 (57) ───────────────────────────────────────────────────────
            57 => { // import(path) → Map of module exports
                req_args("import", &args, 1, span)?;
                let path = str_arg("import", &args[0], span)?;
                let source = std::fs::read_to_string(&path)
                    .map_err(|e| RuntimeError::new(format!("import(): '{path}': {e}"), span))?;
                let tokens = crate::lexer::Lexer::new(&source)
                    .tokenize()
                    .map_err(|e| RuntimeError::new(
                        format!("import(): 렉서 오류 in '{path}': {}", e.first().map(|x| x.to_string()).unwrap_or_default()), span))?;
                let prog = crate::parser::Parser::new(tokens)
                    .parse()
                    .map_err(|e| RuntimeError::new(
                        format!("import(): 파서 오류 in '{path}': {}", e.first().map(|x| x.to_string()).unwrap_or_default()), span))?;
                let out = crate::compiler::compile(&prog)
                    .map_err(|e| RuntimeError::new(
                        format!("import(): 컴파일 오류 in '{path}': {}", e.first().map(|x| x.to_string()).unwrap_or_default()), span))?;
                let sub_out = Arc::new(Mutex::new(Vec::<String>::new()));
                let mut sub_vm = Vm::new(out.global_count as usize, sub_out);
                sub_vm.run(out.main_fn)
                    .map_err(|e| RuntimeError::new(format!("import(): 모듈 실행 오류 in '{path}': {e}"), span))?;
                // 모듈의 export(최상위 바인딩)를 Map으로. 함수 값은 sub_vm의
                // 모듈 전역 Arc를 그대로 들고 있어, 메인 VM에서 호출돼도
                // 자기 모듈 전역을 참조한다(sub_vm 드롭 후에도 Arc로 유지).
                let mut map = HashMap::new();
                {
                    let g = sub_vm.globals.lock().unwrap();
                    for (name, slot) in &out.global_names {
                        map.insert(name.clone(), g[*slot as usize].clone());
                    }
                }
                Ok(VmValue::Map(Arc::new(map)))
            }

            _ => Err(RuntimeError::new(format!("알 수 없는 내장 함수 인덱스: {idx}"), span)),
        }
    }
}

// ============================================================================
// Pure helpers (free functions)
// ============================================================================

fn vm_add(l: VmValue, r: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match (l, r) {
        (VmValue::Int(a),   VmValue::Int(b))   => Ok(VmValue::Int(a + b)),
        (VmValue::Float(a), VmValue::Float(b)) => Ok(VmValue::Float(a + b)),
        (VmValue::Int(a),   VmValue::Float(b)) => Ok(VmValue::Float(a as f64 + b)),
        (VmValue::Float(a), VmValue::Int(b))   => Ok(VmValue::Float(a + b as f64)),
        (VmValue::Str(a),   VmValue::Str(b))   => Ok(VmValue::Str(a + &b)),
        (VmValue::List(mut a), VmValue::List(b)) => { Arc::make_mut(&mut a).extend(b.iter().cloned()); Ok(VmValue::List(a)) }
        (l, r) => Err(RuntimeError::new(
            format!("+: {} + {} 연산 불가", l.type_name(), r.type_name()), span)),
    }
}

fn vm_arith(l: VmValue, r: VmValue, span: Span, op: char) -> Result<VmValue, RuntimeError> {
    let to_float = |l: VmValue, r: VmValue| -> (f64, f64) {
        let lf = match &l { VmValue::Int(n) => *n as f64, VmValue::Float(n) => *n, _ => 0.0 };
        let rf = match &r { VmValue::Int(n) => *n as f64, VmValue::Float(n) => *n, _ => 0.0 };
        (lf, rf)
    };
    match (&l, &r) {
        (VmValue::Int(a), VmValue::Int(b)) => match op {
            '-' => Ok(VmValue::Int(a - b)),
            '*' => Ok(VmValue::Int(a * b)),
            '/' => {
                if *b == 0 { return Err(RuntimeError::new("0으로 나눌 수 없음", span)); }
                if a % b == 0 { Ok(VmValue::Int(a / b)) }
                else { Ok(VmValue::Float(*a as f64 / *b as f64)) }
            }
            '%' => {
                if *b == 0 { return Err(RuntimeError::new("나머지: 0으로 나눌 수 없음", span)); }
                Ok(VmValue::Int(a % b))
            }
            _ => unreachable!(),
        },
        (VmValue::Float(_), _) | (_, VmValue::Float(_)) => {
            let (a, b) = to_float(l, r);
            Ok(VmValue::Float(match op {
                '-' => a - b,
                '*' => a * b,
                '/' => a / b,
                '%' => a % b,
                _ => unreachable!(),
            }))
        }
        (l, r) => Err(RuntimeError::new(
            format!("{op}: {} {op} {} 연산 불가", l.type_name(), r.type_name()), span)),
    }
}

fn vm_eq(l: &VmValue, r: &VmValue) -> bool {
    match (l, r) {
        (VmValue::Int(a),   VmValue::Int(b))   => a == b,
        (VmValue::Float(a), VmValue::Float(b)) => a == b,
        (VmValue::Int(a),   VmValue::Float(b)) => (*a as f64) == *b,
        (VmValue::Float(a), VmValue::Int(b))   => *a == (*b as f64),
        (VmValue::Bool(a),  VmValue::Bool(b))  => a == b,
        (VmValue::Str(a),   VmValue::Str(b))   => a == b,
        (VmValue::Nil,      VmValue::Nil)       => true,
        (VmValue::List(a),  VmValue::List(b))  =>
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| vm_eq(x, y)),
        _ => false,
    }
}

fn vm_cmp(l: &VmValue, r: &VmValue, span: Span) -> Result<Ordering, RuntimeError> {
    match (l, r) {
        (VmValue::Int(a),   VmValue::Int(b))   => Ok(a.cmp(b)),
        (VmValue::Float(a), VmValue::Float(b)) => Ok(a.partial_cmp(b).unwrap_or(Ordering::Equal)),
        (VmValue::Int(a),   VmValue::Float(b)) => Ok((*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)),
        (VmValue::Float(a), VmValue::Int(b))   => Ok(a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)),
        (VmValue::Str(a),   VmValue::Str(b))   => Ok(a.cmp(b)),
        (l, r) => Err(RuntimeError::new(
            format!("비교: {} 와 {} 비교 불가", l.type_name(), r.type_name()), span)),
    }
}

fn vm_index_get(target: VmValue, idx: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match (target, &idx) {
        (VmValue::List(items), VmValue::Int(i)) => {
            let len = items.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("인덱스 범위 초과: {i} (길이 {len})"), span));
            }
            Ok(items[i as usize].clone())
        }
        (VmValue::Str(s), VmValue::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("문자열 인덱스 범위 초과: {i} (길이 {len})"), span));
            }
            Ok(VmValue::Str(chars[i as usize].to_string()))
        }
        (VmValue::Map(map), _) => {
            let key = idx.to_string();
            Ok(map.get(&key).cloned().unwrap_or(VmValue::Nil))
        }
        (target, idx) => Err(RuntimeError::new(
            format!("인덱스: {} [{}] 지원 안 됨", target.type_name(), idx.type_name()), span)),
    }
}

fn vm_index_set(container: VmValue, idx: VmValue, val: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match (container, &idx) {
        (VmValue::List(mut items), VmValue::Int(i)) => {
            let len = items.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("인덱스 범위 초과: {i} (길이 {len})"), span));
            }
            Arc::make_mut(&mut items)[i as usize] = val;
            Ok(VmValue::List(items))
        }
        (VmValue::Map(mut map), _) => {
            Arc::make_mut(&mut map).insert(idx.to_string(), val);
            Ok(VmValue::Map(map))
        }
        (container, _) => Err(RuntimeError::new(
            format!("인덱스 대입: {} 지원 안 됨", container.type_name()), span)),
    }
}

fn vm_field_get(target: VmValue, name: &str, span: Span) -> Result<VmValue, RuntimeError> {
    match target {
        VmValue::Map(map) => Ok(map.get(name).cloned().unwrap_or(VmValue::Nil)),
        other => Err(RuntimeError::new(
            format!("필드 접근: {} 에 필드 '{}' 없음", other.type_name(), name), span)),
    }
}

fn str_arg(name: &str, v: &VmValue, span: Span) -> Result<String, RuntimeError> {
    match v {
        VmValue::Str(s) => Ok(s.clone()),
        other => Err(RuntimeError::new(
            format!("{name}(): 문자열 필요, {} 발견", other.type_name()), span)),
    }
}

fn list_arg(name: &str, v: &VmValue, span: Span) -> Result<Vec<VmValue>, RuntimeError> {
    match v {
        VmValue::List(items) => Ok((**items).clone()),
        other => Err(RuntimeError::new(
            format!("{name}(): 리스트 필요, {} 발견", other.type_name()), span)),
    }
}

fn as_float(v: &VmValue, span: Span) -> Result<f64, RuntimeError> {
    match v {
        VmValue::Int(n)   => Ok(*n as f64),
        VmValue::Float(n) => Ok(*n),
        other => Err(RuntimeError::new(
            format!("부동소수점 필요, {} 발견", other.type_name()), span)),
    }
}

fn req_args(name: &str, args: &[VmValue], n: usize, span: Span) -> Result<(), RuntimeError> {
    if args.len() != n {
        Err(RuntimeError::new(
            format!("{name}(): {n}개 인자 필요, {}개 전달", args.len()), span))
    } else {
        Ok(())
    }
}

fn as_int(v: &VmValue, span: Span) -> Result<i64, RuntimeError> {
    match v {
        VmValue::Int(n)   => Ok(*n),
        VmValue::Float(n) => Ok(*n as i64),
        other => Err(RuntimeError::new(
            format!("정수 필요, {} 발견", other.type_name()), span)),
    }
}

/// Future 자동 조인 (얕게): 최상위 Future 하나만 resolve.
fn auto_resolve(v: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match v {
        VmValue::Future(f) => f.resolve().map_err(|mut e| { e.span = span; e }),
        other => Ok(other),
    }
}

/// Future 깊이 해소 (print용): 컨테이너 안의 Future도 재귀 처리.
fn deep_resolve(v: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    let v = auto_resolve(v, span)?;
    match v {
        VmValue::List(items) => {
            let mut resolved = Vec::with_capacity(items.len());
            for item in items.iter() { resolved.push(deep_resolve(item.clone(), span)?); }
            Ok(VmValue::List(Arc::new(resolved)))
        }
        VmValue::Map(map) => {
            let mut resolved = HashMap::new();
            for (k, val) in map.iter() { resolved.insert(k.clone(), deep_resolve(val.clone(), span)?); }
            Ok(VmValue::Map(Arc::new(resolved)))
        }
        other => Ok(other),
    }
}

/// spawn 경계에서 클로저의 upvalue를 독립 복사 (값 의미론).
/// 각 upvalue의 현재 값을 읽어 새로운 독립적 Upvalue 슬롯에 저장.
/// 런타임 값이 타입 힌트와 일치하는지 검사 (Any는 위에서 처리됨).
fn value_matches_type(v: &VmValue, t: TypeAnn) -> bool {
    match t {
        TypeAnn::Int   => matches!(v, VmValue::Int(_)),
        TypeAnn::Float => matches!(v, VmValue::Float(_)),
        TypeAnn::Bool  => matches!(v, VmValue::Bool(_)),
        TypeAnn::Str   => matches!(v, VmValue::Str(_)),
        TypeAnn::Nil   => matches!(v, VmValue::Nil),
        TypeAnn::List  => matches!(v, VmValue::List(_)),
        TypeAnn::Map   => matches!(v, VmValue::Map(_)),
        TypeAnn::Fn    => matches!(v, VmValue::Closure(_) | VmValue::Function(_) | VmValue::Builtin(_)),
        TypeAnn::Any   => true,
    }
}

fn deep_clone_closure(c: &Arc<VmClosure>) -> Arc<VmClosure> {
    let upvalues: Vec<UpvalueRef> = c.upvalues.iter().map(|uv| {
        let val = uv.get(); // VmValue::clone() — 컨테이너 깊은 복사, 참조타입 Arc 클론
        let new_locals = Arc::new(Mutex::new(vec![val]));
        Arc::new(Upvalue { locals: new_locals, slot: 0 })
    }).collect();
    // 모듈 전역도 독립 복사본으로 (값 의미론: 데이터 깊은 복사, 함수/채널은 Arc 공유).
    // Vec<VmValue>::clone() 이 각 값에 대해 올바른 복사 의미를 적용한다.
    let globals = {
        let g = c.globals.lock().unwrap();
        Arc::new(Mutex::new(g.clone()))
    };
    Arc::new(VmClosure { func: c.func.clone(), upvalues, globals })
}

fn to_runtime(v: &VmValue) -> crate::runtime::Value {
    match v {
        VmValue::Int(n)   => crate::runtime::Value::Int(*n),
        VmValue::Float(n) => crate::runtime::Value::Float(*n),
        VmValue::Bool(b)  => crate::runtime::Value::Bool(*b),
        VmValue::Str(s)   => crate::runtime::Value::Str(s.clone()),
        VmValue::Nil      => crate::runtime::Value::Nil,
        _ => crate::runtime::Value::Nil,
    }
}

fn from_runtime(v: crate::runtime::Value) -> VmValue {
    match v {
        crate::runtime::Value::Int(n)   => VmValue::Int(n),
        crate::runtime::Value::Float(n) => VmValue::Float(n),
        crate::runtime::Value::Bool(b)  => VmValue::Bool(b),
        crate::runtime::Value::Str(s)   => VmValue::Str(s),
        crate::runtime::Value::Nil      => VmValue::Nil,
        _ => VmValue::Nil,
    }
}
