// Bang — Phase 5: Bytecode VM (stack VM + upvalues, Part B multi-thread spawn)

#![allow(clippy::ptr_arg)]

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

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
    // 네트워크 핸들 — 참조 의미론 (Arc 공유, 채널과 동일)
    TcpListener(Arc<std::net::TcpListener>),
    TcpConn(Arc<Mutex<std::net::TcpStream>>),
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
            VmValue::TcpListener(_) => "TcpListener",
            VmValue::TcpConn(_)  => "TcpConn",
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
            VmValue::TcpListener(_) => write!(f, "<tcp listener>"),
            VmValue::TcpConn(_)  => write!(f, "<tcp conn>"),
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

    /// 상수를 풀에 추가하고 인덱스(u16)를 반환한다.
    /// 단순 상수(Int/Float/Bool/Str/Nil)는 중복 제거해 풀 크기를 절약한다.
    /// 함수 등 참조 타입은 매번 새로 추가한다.
    pub fn add_constant(&mut self, val: VmValue) -> u16 {
        if let Some(i) = self.constants.iter().position(|c| const_eq(c, &val)) {
            return i as u16;
        }
        let idx = self.constants.len();
        self.constants.push(val);
        idx as u16
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
    // stdlib 확장 (58-63)
    "slice",        // 58
    "has",          // 59
    "get",          // 60
    "merge",        // 61
    "repeat",       // 62
    "index_of",     // 63
    // stdlib: JSON / 시간 / 난수 (64-68)
    "json_parse",     // 64
    "json_stringify", // 65
    "now_ms",         // 66
    "random",         // 67
    "random_int",     // 68
    // stdlib: 파일시스템 / list 유틸 / 시간포맷 / 문자 (69-76)
    "list_dir",     // 69
    "file_exists",  // 70
    "is_dir",       // 71
    "sort_by",      // 72
    "unique",       // 73
    "format_time",  // 74
    "ord",          // 75
    "chr",          // 76
    // stdlib: 정규식 (77-80)
    "regex_match",    // 77
    "regex_find",     // 78
    "regex_find_all", // 79
    "regex_replace",  // 80
    "regex_groups",   // 81
    // stdlib: math (82-92)
    "gcd",        // 82
    "clamp",      // 83
    "sign",       // 84
    "sin",        // 85
    "cos",        // 86
    "tan",        // 87
    "log",        // 88
    "log10",      // 89
    "exp",        // 90
    "pi",         // 91
    "e",          // 92
    // stdlib: 집합 연산 (93-95)
    "union",      // 93
    "intersect",  // 94
    "difference", // 95
    // stdlib: 네트워킹 TCP (96-100)
    "tcp_listen", // 96
    "tcp_accept", // 97
    "tcp_read",   // 98
    "tcp_write",  // 99
    "tcp_close",  // 100
    "tcp_read_until",  // 101
    "tcp_set_timeout", // 102
    "select",          // 103
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
    /// 호출 프레임 locals 재사용 풀 (호출당 힙 할당 제거 → 할당자 경합 완화).
    /// 클로저에 포착되지 않은(strong_count==1) locals만 회수한다.
    locals_pool: Vec<Arc<Mutex<Vec<VmValue>>>>,
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
            locals_pool: Vec::new(),
        }
    }

    /// 풀에서 locals 버퍼를 가져오거나 새로 할당한다. local_count 크기로 Nil 초기화.
    fn acquire_locals(&mut self, local_count: usize) -> Arc<Mutex<Vec<VmValue>>> {
        if let Some(arc) = self.locals_pool.pop() {
            if let Ok(mut g) = arc.lock() {
                g.clear();
                g.resize(local_count, VmValue::Nil);
            }
            arc
        } else {
            Arc::new(Mutex::new(vec![VmValue::Nil; local_count]))
        }
    }

    /// 프레임 종료 시 locals를 회수한다. 클로저가 포착(strong_count>1)했으면 버린다.
    fn release_locals(&mut self, locals: Arc<Mutex<Vec<VmValue>>>) {
        if Arc::strong_count(&locals) == 1 && self.locals_pool.len() < 256 {
            self.locals_pool.push(locals);
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
        for f in scope { warn_if_spawn_err(f.resolve()); }

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
            locals_pool: Vec::new(),
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
        for f in scope { warn_if_spawn_err(f.resolve()); }

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

    fn get_constant(&self, idx: u16) -> VmValue {
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
                    if let Some(f) = self.frames.pop() {
                        self.release_locals(f.locals);
                    }
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
                    let idx = self.read_u16();
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
                    // Arc 복제 없이 lock (guard는 문장 끝에 drop → stack.push와 충돌 없음)
                    let v = self.frames[fi].closure.globals.lock().unwrap()[slot].clone();
                    self.stack.push(v);
                }
                OP_STORE_GLOBAL => {
                    let slot = self.read_u16() as usize;
                    let val = self.stack_pop();
                    let fi = self.frames.len() - 1;
                    self.frames[fi].closure.globals.lock().unwrap()[slot] = val;
                }

                // --- Builtins ---
                OP_LOAD_BUILTIN => {
                    let idx = self.read_byte() as usize;
                    self.stack.push(VmValue::Builtin(idx));
                }

                // --- Closure ---
                OP_CLOSURE => {
                    let fn_const_idx = self.read_u16();
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
                    if let Some(f) = self.frames.pop() {
                        self.release_locals(f.locals);
                    }
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
                    let name_idx = self.read_u16();
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
                let callee_idx = self.stack.len() - arg_count - 1;
                let base = callee_idx + 1; // 첫 인자 위치

                // Phase 9 Part B: JIT 호출 시도 (Int-only 함수에만 적용)
                #[cfg(feature = "jit")]
                {
                    let args: Vec<VmValue> = self.stack[base..].to_vec();
                    if let Some(result) = crate::codegen::jit::try_jit_call(&closure.func, &args, span) {
                        self.stack.truncate(callee_idx); // 인자 + callee 제거
                        self.stack.push(result?);
                        return Ok(());
                    }
                }

                // 풀에서 locals를 가져와 인자를 슬롯 0..arg_count에 채운다 (중간 Vec 없음)
                let local_count = closure.func.local_count;
                let locals = self.acquire_locals(local_count);
                if let Ok(mut g) = locals.lock() {
                    for i in 0..arg_count {
                        g[i] = self.stack[base + i].clone();
                    }
                }
                self.stack.truncate(callee_idx); // 인자 + callee 제거
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
            57 => { // import(spec) → Map of module exports (캐시된 싱글톤)
                req_args("import", &args, 1, span)?;
                let spec = str_arg("import", &args[0], span)?;
                // 이름 해석: 바레 이름은 bang_modules/ 및 BANG_PATH에서 검색,
                // .bang/경로 구분자 포함이면 직접 경로.
                let path = resolve_module(&spec).ok_or_else(|| RuntimeError::new(
                    format!("import(): 모듈을 찾을 수 없음: '{spec}' (bang_modules/ 또는 BANG_PATH 확인)"), span))?;
                let path = path.to_string_lossy().into_owned();
                // 캐시 키: 정규화된 절대경로 (실패 시 해석된 경로)
                let key = std::fs::canonicalize(&path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| path.clone());
                if let Some(cached) = module_cache().lock().unwrap().get(&key).cloned() {
                    return Ok(cached); // 이미 로드됨 → 같은 모듈 인스턴스 반환
                }
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
                // 모듈의 top-level print는 메인 프로그램 출력으로 보낸다.
                let mut sub_vm = Vm::new(out.global_count as usize, self.output.clone());
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
                let result = VmValue::Map(Arc::new(map));
                module_cache().lock().unwrap().insert(key, result.clone());
                Ok(result)
            }

            // ── stdlib 확장 (58-63) ──────────────────────────────────────────
            58 => { // slice(seq, start, end) — list/str 부분 추출
                req_args("slice", &args, 3, span)?;
                let start = int_of("slice", &args[1], span)?;
                let end = int_of("slice", &args[2], span)?;
                match &args[0] {
                    VmValue::List(items) => {
                        let len = items.len() as i64;
                        let s = start.clamp(0, len) as usize;
                        let e = end.clamp(0, len) as usize;
                        let sub = if s < e { items[s..e].to_vec() } else { Vec::new() };
                        Ok(VmValue::List(Arc::new(sub)))
                    }
                    VmValue::Str(s0) => {
                        let chars: Vec<char> = s0.chars().collect();
                        let len = chars.len() as i64;
                        let s = start.clamp(0, len) as usize;
                        let e = end.clamp(0, len) as usize;
                        let sub: String = if s < e { chars[s..e].iter().collect() } else { String::new() };
                        Ok(VmValue::Str(sub))
                    }
                    other => Err(RuntimeError::new(
                        format!("slice: List/Str 필요, {} 발견", other.type_name()), span)),
                }
            }
            59 => { // has(map, key) → Bool
                req_args("has", &args, 2, span)?;
                match &args[0] {
                    VmValue::Map(m) => Ok(VmValue::Bool(m.contains_key(&args[1].to_string()))),
                    other => Err(RuntimeError::new(
                        format!("has: Map 필요, {} 발견", other.type_name()), span)),
                }
            }
            60 => { // get(map, key, default)
                req_args("get", &args, 3, span)?;
                match &args[0] {
                    VmValue::Map(m) => Ok(m.get(&args[1].to_string())
                        .cloned().unwrap_or_else(|| args[2].clone())),
                    other => Err(RuntimeError::new(
                        format!("get: Map 필요, {} 발견", other.type_name()), span)),
                }
            }
            61 => { // merge(map1, map2) → 새 map (map2 우선)
                req_args("merge", &args, 2, span)?;
                match (&args[0], &args[1]) {
                    (VmValue::Map(a), VmValue::Map(b)) => {
                        let mut m = (**a).clone();
                        for (k, v) in b.iter() { m.insert(k.clone(), v.clone()); }
                        Ok(VmValue::Map(Arc::new(m)))
                    }
                    _ => Err(RuntimeError::new("merge: 두 인자 모두 Map이어야 함", span)),
                }
            }
            62 => { // repeat(str, n)
                req_args("repeat", &args, 2, span)?;
                let s = str_arg("repeat", &args[0], span)?;
                let n = int_of("repeat", &args[1], span)?;
                if n < 0 {
                    return Err(RuntimeError::new("repeat: n은 음수가 될 수 없음", span));
                }
                Ok(VmValue::Str(s.repeat(n as usize)))
            }
            63 => { // index_of(list, x) → 첫 인덱스 또는 -1
                req_args("index_of", &args, 2, span)?;
                let list = list_arg("index_of", &args[0], span)?;
                let idx = list.iter().position(|e| vm_eq(e, &args[1]))
                    .map(|i| i as i64).unwrap_or(-1);
                Ok(VmValue::Int(idx))
            }

            // ── JSON / 시간 / 난수 (64-68) ────────────────────────────────────
            64 => { // json_parse(str) → value
                req_args("json_parse", &args, 1, span)?;
                let s = str_arg("json_parse", &args[0], span)?;
                json_parse(&s, span)
            }
            65 => { // json_stringify(value) → str
                req_args("json_stringify", &args, 1, span)?;
                let mut out = String::new();
                json_stringify(&args[0], &mut out, span)?;
                Ok(VmValue::Str(out))
            }
            66 => { // now_ms() → epoch millis
                req_args("now_ms", &args, 0, span)?;
                let ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(VmValue::Int(ms))
            }
            67 => { // random() → [0,1) float
                req_args("random", &args, 0, span)?;
                Ok(VmValue::Float(next_random()))
            }
            68 => { // random_int(lo, hi) → [lo, hi] 정수 (포함)
                req_args("random_int", &args, 2, span)?;
                let lo = int_of("random_int", &args[0], span)?;
                let hi = int_of("random_int", &args[1], span)?;
                if hi < lo {
                    return Err(RuntimeError::new("random_int: hi >= lo 이어야 함", span));
                }
                let range = (hi - lo + 1) as f64;
                let n = lo + (next_random() * range) as i64;
                Ok(VmValue::Int(n.min(hi)))
            }

            // ── 파일시스템 / list 유틸 / 시간포맷 / 문자 (69-76) ──────────────
            69 => { // list_dir(path) → List of entry names
                req_args("list_dir", &args, 1, span)?;
                let path = str_arg("list_dir", &args[0], span)?;
                let mut names = Vec::new();
                let rd = std::fs::read_dir(&path)
                    .map_err(|e| RuntimeError::new(format!("list_dir('{path}'): {e}"), span))?;
                for entry in rd.flatten() {
                    names.push(VmValue::Str(entry.file_name().to_string_lossy().into_owned()));
                }
                Ok(VmValue::List(Arc::new(names)))
            }
            70 => { // file_exists(path) → Bool
                req_args("file_exists", &args, 1, span)?;
                let path = str_arg("file_exists", &args[0], span)?;
                Ok(VmValue::Bool(std::path::Path::new(&path).exists()))
            }
            71 => { // is_dir(path) → Bool
                req_args("is_dir", &args, 1, span)?;
                let path = str_arg("is_dir", &args[0], span)?;
                Ok(VmValue::Bool(std::path::Path::new(&path).is_dir()))
            }
            72 => { // sort_by(list, keyfn) → 키 기준 정렬 사본
                req_args("sort_by", &args, 2, span)?;
                let list = list_arg("sort_by", &args[0], span)?;
                let func = args[1].clone();
                // 각 원소의 키를 계산 (고차 호출)
                let mut keyed: Vec<(VmValue, VmValue)> = Vec::with_capacity(list.len());
                for item in list {
                    let depth_before = self.frames.len();
                    self.stack.push(func.clone());
                    self.stack.push(item.clone());
                    self.do_call(1, span, stop_depth)?;
                    if self.frames.len() > depth_before {
                        self.exec_until(depth_before)?;
                    }
                    let key = self.stack_pop();
                    keyed.push((key, item));
                }
                let mut err: Option<RuntimeError> = None;
                keyed.sort_by(|a, b| {
                    if err.is_some() { return std::cmp::Ordering::Equal; }
                    match vm_cmp(&a.0, &b.0, span) {
                        Ok(o) => o,
                        Err(e) => { err = Some(e); std::cmp::Ordering::Equal }
                    }
                });
                if let Some(e) = err { return Err(e); }
                Ok(VmValue::List(Arc::new(keyed.into_iter().map(|(_, v)| v).collect())))
            }
            73 => { // unique(list) → 순서 유지 중복 제거
                req_args("unique", &args, 1, span)?;
                let list = list_arg("unique", &args[0], span)?;
                let mut out: Vec<VmValue> = Vec::new();
                for item in list {
                    if !out.iter().any(|e| vm_eq(e, &item)) {
                        out.push(item);
                    }
                }
                Ok(VmValue::List(Arc::new(out)))
            }
            74 => { // format_time(ms) → "YYYY-MM-DD HH:MM:SS" (UTC)
                req_args("format_time", &args, 1, span)?;
                let ms = int_of("format_time", &args[0], span)?;
                Ok(VmValue::Str(format_time_utc(ms)))
            }
            75 => { // ord(str) → 첫 문자의 코드포인트
                req_args("ord", &args, 1, span)?;
                let s = str_arg("ord", &args[0], span)?;
                match s.chars().next() {
                    Some(c) => Ok(VmValue::Int(c as i64)),
                    None => Err(RuntimeError::new("ord: 빈 문자열", span)),
                }
            }
            76 => { // chr(int) → 한 문자 문자열
                req_args("chr", &args, 1, span)?;
                let n = int_of("chr", &args[0], span)?;
                match u32::try_from(n).ok().and_then(char::from_u32) {
                    Some(c) => Ok(VmValue::Str(c.to_string())),
                    None => Err(RuntimeError::new(format!("chr: 잘못된 코드포인트 {n}"), span)),
                }
            }

            // ── 정규식 (77-80) ────────────────────────────────────────────────
            77 => { // regex_match(s, pat) → Bool
                req_args("regex_match", &args, 2, span)?;
                let s = str_arg("regex_match", &args[0], span)?;
                let pat = str_arg("regex_match", &args[1], span)?;
                let re = compile_regex(&pat, span)?;
                Ok(VmValue::Bool(re.is_match(&s.chars().collect::<Vec<_>>())))
            }
            78 => { // regex_find(s, pat) → 첫 매치 문자열 또는 nil
                req_args("regex_find", &args, 2, span)?;
                let s = str_arg("regex_find", &args[0], span)?;
                let pat = str_arg("regex_find", &args[1], span)?;
                let re = compile_regex(&pat, span)?;
                let chars: Vec<char> = s.chars().collect();
                match re.search(&chars) {
                    Some((a, b)) => Ok(VmValue::Str(chars[a..b].iter().collect())),
                    None => Ok(VmValue::Nil),
                }
            }
            79 => { // regex_find_all(s, pat) → List of 매치 문자열
                req_args("regex_find_all", &args, 2, span)?;
                let s = str_arg("regex_find_all", &args[0], span)?;
                let pat = str_arg("regex_find_all", &args[1], span)?;
                let re = compile_regex(&pat, span)?;
                let chars: Vec<char> = s.chars().collect();
                let matches: Vec<VmValue> = re.find_all(&chars).into_iter()
                    .map(|(a, b)| VmValue::Str(chars[a..b].iter().collect()))
                    .collect();
                Ok(VmValue::List(Arc::new(matches)))
            }
            80 => { // regex_replace(s, pat, repl) → 치환된 문자열
                req_args("regex_replace", &args, 3, span)?;
                let s = str_arg("regex_replace", &args[0], span)?;
                let pat = str_arg("regex_replace", &args[1], span)?;
                let repl = str_arg("regex_replace", &args[2], span)?;
                let re = compile_regex(&pat, span)?;
                let chars: Vec<char> = s.chars().collect();
                Ok(VmValue::Str(re.replace_all(&chars, &repl)))
            }
            81 => { // regex_groups(s, pat) → [전체매치, g1, g2, ...] 또는 nil
                req_args("regex_groups", &args, 2, span)?;
                let s = str_arg("regex_groups", &args[0], span)?;
                let pat = str_arg("regex_groups", &args[1], span)?;
                let re = compile_regex(&pat, span)?;
                let chars: Vec<char> = s.chars().collect();
                match re.captures(&chars) {
                    Some(groups) => {
                        let items: Vec<VmValue> = groups.into_iter()
                            .map(|g| match g {
                                Some((a, b)) => VmValue::Str(chars[a..b].iter().collect()),
                                None => VmValue::Nil,
                            })
                            .collect();
                        Ok(VmValue::List(Arc::new(items)))
                    }
                    None => Ok(VmValue::Nil),
                }
            }

            // ── math (82-92) ──────────────────────────────────────────────────
            82 => { // gcd(a, b)
                req_args("gcd", &args, 2, span)?;
                let mut a = int_of("gcd", &args[0], span)?.abs();
                let mut b = int_of("gcd", &args[1], span)?.abs();
                while b != 0 { let t = b; b = a % b; a = t; }
                Ok(VmValue::Int(a))
            }
            83 => { // clamp(x, lo, hi) → 원본 타입 유지
                req_args("clamp", &args, 3, span)?;
                if vm_cmp(&args[0], &args[1], span)? == Ordering::Less {
                    Ok(args[1].clone())
                } else if vm_cmp(&args[0], &args[2], span)? == Ordering::Greater {
                    Ok(args[2].clone())
                } else {
                    Ok(args[0].clone())
                }
            }
            84 => { // sign(x) → -1 / 0 / 1
                req_args("sign", &args, 1, span)?;
                let x = num_of("sign", &args[0], span)?;
                Ok(VmValue::Int(if x > 0.0 { 1 } else if x < 0.0 { -1 } else { 0 }))
            }
            85 => { req_args("sin", &args, 1, span)?; Ok(VmValue::Float(num_of("sin", &args[0], span)?.sin())) }
            86 => { req_args("cos", &args, 1, span)?; Ok(VmValue::Float(num_of("cos", &args[0], span)?.cos())) }
            87 => { req_args("tan", &args, 1, span)?; Ok(VmValue::Float(num_of("tan", &args[0], span)?.tan())) }
            88 => { req_args("log", &args, 1, span)?; Ok(VmValue::Float(num_of("log", &args[0], span)?.ln())) }
            89 => { req_args("log10", &args, 1, span)?; Ok(VmValue::Float(num_of("log10", &args[0], span)?.log10())) }
            90 => { req_args("exp", &args, 1, span)?; Ok(VmValue::Float(num_of("exp", &args[0], span)?.exp())) }
            91 => { req_args("pi", &args, 0, span)?; Ok(VmValue::Float(std::f64::consts::PI)) }
            92 => { req_args("e", &args, 0, span)?; Ok(VmValue::Float(std::f64::consts::E)) }

            // ── 집합 연산 (리스트 기반) (93-95) ───────────────────────────────
            93 => { // union(a, b) → 중복 제거 합집합 (a 순서 후 b의 새 원소)
                req_args("union", &args, 2, span)?;
                let a = list_arg("union", &args[0], span)?;
                let b = list_arg("union", &args[1], span)?;
                let mut out: Vec<VmValue> = Vec::new();
                for item in a.into_iter().chain(b) {
                    if !out.iter().any(|e| vm_eq(e, &item)) { out.push(item); }
                }
                Ok(VmValue::List(Arc::new(out)))
            }
            94 => { // intersect(a, b) → a 중 b에도 있는 원소 (중복 제거)
                req_args("intersect", &args, 2, span)?;
                let a = list_arg("intersect", &args[0], span)?;
                let b = list_arg("intersect", &args[1], span)?;
                let mut out: Vec<VmValue> = Vec::new();
                for item in a {
                    if b.iter().any(|e| vm_eq(e, &item)) && !out.iter().any(|e| vm_eq(e, &item)) {
                        out.push(item);
                    }
                }
                Ok(VmValue::List(Arc::new(out)))
            }
            95 => { // difference(a, b) → a 중 b에 없는 원소 (중복 제거)
                req_args("difference", &args, 2, span)?;
                let a = list_arg("difference", &args[0], span)?;
                let b = list_arg("difference", &args[1], span)?;
                let mut out: Vec<VmValue> = Vec::new();
                for item in a {
                    if !b.iter().any(|e| vm_eq(e, &item)) && !out.iter().any(|e| vm_eq(e, &item)) {
                        out.push(item);
                    }
                }
                Ok(VmValue::List(Arc::new(out)))
            }

            // ── 네트워킹 TCP (96-100) ─────────────────────────────────────────
            96 => { // tcp_listen(addr) → TcpListener  (예: "127.0.0.1:8080")
                req_args("tcp_listen", &args, 1, span)?;
                let addr = str_arg("tcp_listen", &args[0], span)?;
                let listener = std::net::TcpListener::bind(&addr)
                    .map_err(|e| RuntimeError::new(format!("tcp_listen('{addr}'): {e}"), span))?;
                Ok(VmValue::TcpListener(Arc::new(listener)))
            }
            97 => { // tcp_accept(server) → TcpConn  (블로킹)
                req_args("tcp_accept", &args, 1, span)?;
                match &args[0] {
                    VmValue::TcpListener(l) => {
                        let (stream, _peer) = l.accept()
                            .map_err(|e| RuntimeError::new(format!("tcp_accept(): {e}"), span))?;
                        Ok(VmValue::TcpConn(Arc::new(Mutex::new(stream))))
                    }
                    other => Err(RuntimeError::new(
                        format!("tcp_accept: TcpListener 필요, {} 발견", other.type_name()), span)),
                }
            }
            98 => { // tcp_read(conn) → str  (최대 4096바이트 1회 읽기, EOF면 "")
                req_args("tcp_read", &args, 1, span)?;
                match &args[0] {
                    VmValue::TcpConn(c) => {
                        use std::io::Read;
                        let mut buf = [0u8; 4096];
                        let n = {
                            let mut s = c.lock().unwrap();
                            s.read(&mut buf)
                                .map_err(|e| RuntimeError::new(format!("tcp_read(): {e}"), span))?
                        };
                        Ok(VmValue::Str(String::from_utf8_lossy(&buf[..n]).into_owned()))
                    }
                    other => Err(RuntimeError::new(
                        format!("tcp_read: TcpConn 필요, {} 발견", other.type_name()), span)),
                }
            }
            99 => { // tcp_write(conn, str) → nil
                req_args("tcp_write", &args, 2, span)?;
                let data = str_arg("tcp_write", &args[1], span)?;
                match &args[0] {
                    VmValue::TcpConn(c) => {
                        use std::io::Write;
                        let mut s = c.lock().unwrap();
                        s.write_all(data.as_bytes())
                            .map_err(|e| RuntimeError::new(format!("tcp_write(): {e}"), span))?;
                        Ok(VmValue::Nil)
                    }
                    other => Err(RuntimeError::new(
                        format!("tcp_write: TcpConn 필요, {} 발견", other.type_name()), span)),
                }
            }
            100 => { // tcp_close(conn) → nil
                req_args("tcp_close", &args, 1, span)?;
                match &args[0] {
                    VmValue::TcpConn(c) => {
                        let s = c.lock().unwrap();
                        let _ = s.shutdown(std::net::Shutdown::Both);
                        Ok(VmValue::Nil)
                    }
                    other => Err(RuntimeError::new(
                        format!("tcp_close: TcpConn 필요, {} 발견", other.type_name()), span)),
                }
            }

            101 => { // tcp_read_until(conn, marker) → marker 나올 때까지 누적 읽기
                req_args("tcp_read_until", &args, 2, span)?;
                let marker = str_arg("tcp_read_until", &args[1], span)?;
                match &args[0] {
                    VmValue::TcpConn(c) => {
                        use std::io::Read;
                        let mut acc = String::new();
                        let mut buf = [0u8; 1024];
                        loop {
                            if !marker.is_empty() && acc.contains(&marker) { break; }
                            let n = {
                                let mut s = c.lock().unwrap();
                                s.read(&mut buf)
                                    .map_err(|e| RuntimeError::new(format!("tcp_read_until(): {e}"), span))?
                            };
                            if n == 0 { break; } // EOF
                            acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                        }
                        Ok(VmValue::Str(acc))
                    }
                    other => Err(RuntimeError::new(
                        format!("tcp_read_until: TcpConn 필요, {} 발견", other.type_name()), span)),
                }
            }
            102 => { // tcp_set_timeout(conn, ms) → 읽기 타임아웃 설정 (0 = 무제한)
                req_args("tcp_set_timeout", &args, 2, span)?;
                let ms = int_of("tcp_set_timeout", &args[1], span)?;
                match &args[0] {
                    VmValue::TcpConn(c) => {
                        let dur = if ms <= 0 { None }
                            else { Some(std::time::Duration::from_millis(ms as u64)) };
                        let s = c.lock().unwrap();
                        s.set_read_timeout(dur)
                            .map_err(|e| RuntimeError::new(format!("tcp_set_timeout(): {e}"), span))?;
                        Ok(VmValue::Nil)
                    }
                    other => Err(RuntimeError::new(
                        format!("tcp_set_timeout: TcpConn 필요, {} 발견", other.type_name()), span)),
                }
            }

            103 => { // select(channels) → [index, value] (먼저 준비된 채널), 모두 닫히면 nil
                req_args("select", &args, 1, span)?;
                let list = list_arg("select", &args[0], span)?;
                // 모든 인자가 채널인지 확인
                let mut chans = Vec::with_capacity(list.len());
                for v in &list {
                    match v {
                        VmValue::Channel(c) => chans.push(c.clone()),
                        other => return Err(RuntimeError::new(
                            format!("select: 채널 리스트 필요, {} 발견", other.type_name()), span)),
                    }
                }
                if chans.is_empty() {
                    return Err(RuntimeError::new("select: 빈 채널 리스트", span));
                }
                use crate::runtime::TryRecv;
                loop {
                    let mut all_closed = true;
                    for (i, ch) in chans.iter().enumerate() {
                        match ch.try_recv() {
                            TryRecv::Value(v) => {
                                return Ok(VmValue::List(Arc::new(vec![
                                    VmValue::Int(i as i64),
                                    from_runtime(v),
                                ])));
                            }
                            TryRecv::Empty => all_closed = false,
                            TryRecv::Closed => {}
                        }
                    }
                    if all_closed {
                        return Ok(VmValue::Nil); // 모든 채널 닫힘
                    }
                    // 준비된 채널 없음 → 잠깐 양보 후 재시도 (폴링 select)
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
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

fn num_of(name: &str, v: &VmValue, span: Span) -> Result<f64, RuntimeError> {
    match v {
        VmValue::Int(n) => Ok(*n as f64),
        VmValue::Float(n) => Ok(*n),
        other => Err(RuntimeError::new(
            format!("{name}(): 숫자 필요, {} 발견", other.type_name()), span)),
    }
}

fn int_of(name: &str, v: &VmValue, span: Span) -> Result<i64, RuntimeError> {
    match v {
        VmValue::Int(n) => Ok(*n),
        other => Err(RuntimeError::new(
            format!("{name}(): 정수 필요, {} 발견", other.type_name()), span)),
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
/// import 모듈 캐시 — 같은 파일은 한 번만 실행되고 결과(export 맵)를 공유한다.
/// (모듈을 싱글톤으로 만드는 패키지 시스템 v1의 핵심.)
fn module_cache() -> &'static Mutex<HashMap<String, VmValue>> {
    static CACHE: OnceLock<Mutex<HashMap<String, VmValue>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// import 모듈 해석. 바레 이름은 검색 경로에서 찾고, 경로/확장자가 있으면 직접 경로.
fn resolve_module(spec: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    // 직접 경로: .bang 확장자나 경로 구분자 포함
    if spec.ends_with(".bang") || spec.contains('/') || spec.contains('\\') {
        let p = PathBuf::from(spec);
        return if p.is_file() { Some(p) } else { None };
    }
    // 바레 이름: 검색 후보
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from(format!("{spec}.bang")),
        PathBuf::from(format!("bang_modules/{spec}/{spec}.bang")),
        PathBuf::from(format!("bang_modules/{spec}/main.bang")),
        PathBuf::from(format!("bang_modules/{spec}/lib.bang")),
    ];
    if let Ok(bp) = std::env::var("BANG_PATH") {
        for dir in bp.split(':').filter(|d| !d.is_empty()) {
            candidates.push(PathBuf::from(format!("{dir}/{spec}.bang")));
            candidates.push(PathBuf::from(format!("{dir}/{spec}/{spec}.bang")));
            candidates.push(PathBuf::from(format!("{dir}/{spec}/main.bang")));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// 정규식 패턴 컴파일 (에러는 try/catch로 잡히는 런타임 에러).
fn compile_regex(pat: &str, span: Span) -> Result<crate::regex::Regex, RuntimeError> {
    crate::regex::compile(pat)
        .map_err(|e| RuntimeError::new(format!("정규식 오류: {e}"), span))
}

// ── PRNG (xorshift64, 시간 시드) ────────────────────────────────────────────
fn rng_state() -> &'static Mutex<u64> {
    static S: OnceLock<Mutex<u64>> = OnceLock::new();
    S.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15)
            | 1;
        Mutex::new(seed)
    })
}

/// [0, 1) 범위의 난수.
fn next_random() -> f64 {
    let mut guard = match rng_state().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let mut x = *guard;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *guard = x;
    ((x >> 11) as f64) / ((1u64 << 53) as f64)
}

/// epoch 밀리초 → "YYYY-MM-DD HH:MM:SS" (UTC). 외부 의존성 없이 civil-date 변환.
fn format_time_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's days_from_civil 역변환 (civil_from_days)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

// ── JSON ────────────────────────────────────────────────────────────────────

/// JSON 텍스트 → VmValue. object→Map, array→List, number→Int/Float, null→Nil.
fn json_parse(s: &str, span: Span) -> Result<VmValue, RuntimeError> {
    let chars: Vec<char> = s.chars().collect();
    let mut p = JsonParser { chars: &chars, pos: 0, span };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(RuntimeError::new("json_parse: 끝에 잉여 문자", span));
    }
    Ok(v)
}

struct JsonParser<'a> {
    chars: &'a [char],
    pos: usize,
    span: Span,
}

impl JsonParser<'_> {
    fn err(&self, msg: &str) -> RuntimeError {
        RuntimeError::new(format!("json_parse: {msg}"), self.span)
    }
    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' { self.pos += 1; } else { break; }
        }
    }
    fn parse_value(&mut self) -> Result<VmValue, RuntimeError> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => Ok(VmValue::Str(self.parse_string()?)),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            _ => Err(self.err("예상하지 못한 토큰")),
        }
    }
    fn expect(&mut self, lit: &str) -> Result<(), RuntimeError> {
        for ec in lit.chars() {
            if self.peek() == Some(ec) { self.pos += 1; } else { return Err(self.err("리터럴 불일치")); }
        }
        Ok(())
    }
    fn parse_bool(&mut self) -> Result<VmValue, RuntimeError> {
        if self.peek() == Some('t') { self.expect("true")?; Ok(VmValue::Bool(true)) }
        else { self.expect("false")?; Ok(VmValue::Bool(false)) }
    }
    fn parse_null(&mut self) -> Result<VmValue, RuntimeError> {
        self.expect("null")?; Ok(VmValue::Nil)
    }
    fn parse_string(&mut self) -> Result<String, RuntimeError> {
        self.pos += 1; // opening "
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("종료되지 않은 문자열")),
                Some('"') => { self.pos += 1; break; }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some('"') => out.push('"'),
                        Some('\\') => out.push('\\'),
                        Some('/') => out.push('/'),
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('r') => out.push('\r'),
                        Some('b') => out.push('\u{0008}'),
                        Some('f') => out.push('\u{000C}'),
                        Some('u') => {
                            let mut code = 0u32;
                            for _ in 0..4 {
                                self.pos += 1;
                                let d = self.peek().and_then(|c| c.to_digit(16))
                                    .ok_or_else(|| self.err("잘못된 \\u 이스케이프"))?;
                                code = code * 16 + d;
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.err("잘못된 이스케이프")),
                    }
                    self.pos += 1;
                }
                Some(c) => { out.push(c); self.pos += 1; }
            }
        }
        Ok(out)
    }
    fn parse_number(&mut self) -> Result<VmValue, RuntimeError> {
        let start = self.pos;
        let mut is_float = false;
        if self.peek() == Some('-') { self.pos += 1; }
        while let Some(c) = self.peek() {
            match c {
                '0'..='9' => self.pos += 1,
                '.' | 'e' | 'E' | '+' | '-' => { is_float = true; self.pos += 1; }
                _ => break,
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        if is_float {
            text.parse::<f64>().map(VmValue::Float).map_err(|_| self.err("잘못된 숫자"))
        } else {
            match text.parse::<i64>() {
                Ok(n) => Ok(VmValue::Int(n)),
                Err(_) => text.parse::<f64>().map(VmValue::Float).map_err(|_| self.err("잘못된 숫자")),
            }
        }
    }
    fn parse_array(&mut self) -> Result<VmValue, RuntimeError> {
        self.pos += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') { self.pos += 1; return Ok(VmValue::List(Arc::new(items))); }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => { self.pos += 1; }
                Some(']') => { self.pos += 1; break; }
                _ => return Err(self.err("배열에 ',' 또는 ']' 기대")),
            }
        }
        Ok(VmValue::List(Arc::new(items)))
    }
    fn parse_object(&mut self) -> Result<VmValue, RuntimeError> {
        self.pos += 1; // {
        let mut map = HashMap::new();
        self.skip_ws();
        if self.peek() == Some('}') { self.pos += 1; return Ok(VmValue::Map(Arc::new(map))); }
        loop {
            self.skip_ws();
            if self.peek() != Some('"') { return Err(self.err("객체 키는 문자열")); }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(':') { return Err(self.err("키 뒤에 ':' 기대")); }
            self.pos += 1;
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(',') => { self.pos += 1; }
                Some('}') => { self.pos += 1; break; }
                _ => return Err(self.err("객체에 ',' 또는 '}' 기대")),
            }
        }
        Ok(VmValue::Map(Arc::new(map)))
    }
}

/// VmValue → JSON 텍스트. 함수/채널/Future는 직렬화 불가(에러).
fn json_stringify(v: &VmValue, out: &mut String, span: Span) -> Result<(), RuntimeError> {
    match v {
        VmValue::Int(n) => { out.push_str(&n.to_string()); }
        VmValue::Float(n) => { out.push_str(&n.to_string()); }
        VmValue::Bool(b) => { out.push_str(if *b { "true" } else { "false" }); }
        VmValue::Nil => { out.push_str("null"); }
        VmValue::Str(s) => json_escape(s, out),
        VmValue::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 { out.push(','); }
                json_stringify(item, out, span)?;
            }
            out.push(']');
        }
        VmValue::Map(m) => {
            out.push('{');
            // 안정적 출력을 위해 키 정렬
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 { out.push(','); }
                json_escape(k, out);
                out.push(':');
                json_stringify(&m[*k], out, span)?;
            }
            out.push('}');
        }
        other => return Err(RuntimeError::new(
            format!("json_stringify: {} 타입은 직렬화할 수 없음", other.type_name()), span)),
    }
    Ok(())
}

fn json_escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

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
    // 모듈 전역을 spawn 경계에서 **완전 독립 복사**한다.
    // 단순히 Vec를 복사하면 그 안의 함수들이 원본 globals Arc를 그대로 공유해
    // (함수=참조 의미론), 여러 스레드가 같은 globals Mutex를 잠가 심한 경합이 난다.
    // 따라서 같은 모듈에 속한 형제 함수들의 globals를 새 복사본으로 재지정한다.
    let snapshot = { c.globals.lock().unwrap().clone() };
    let new_globals = Arc::new(Mutex::new(snapshot));
    {
        let mut g = new_globals.lock().unwrap();
        for v in g.iter_mut() {
            if let VmValue::Closure(inner) = v {
                if Arc::ptr_eq(&inner.globals, &c.globals) {
                    *v = VmValue::Closure(Arc::new(VmClosure {
                        func: inner.func.clone(),
                        upvalues: inner.upvalues.clone(),
                        globals: new_globals.clone(), // Arc bump (재잠금 아님)
                    }));
                }
            }
        }
    }
    Arc::new(VmClosure { func: c.func.clone(), upvalues, globals: new_globals })
}

/// 상수 풀 중복 제거용 동등성 비교. 단순 값만 같다고 판정하고,
/// 함수 등 참조 타입은 항상 다르게 취급(매번 새 슬롯).
fn const_eq(a: &VmValue, b: &VmValue) -> bool {
    match (a, b) {
        (VmValue::Int(x),   VmValue::Int(y))   => x == y,
        (VmValue::Float(x), VmValue::Float(y)) => x.to_bits() == y.to_bits(),
        (VmValue::Bool(x),  VmValue::Bool(y))  => x == y,
        (VmValue::Str(x),   VmValue::Str(y))   => x == y,
        (VmValue::Nil,      VmValue::Nil)       => true,
        _ => false,
    }
}

/// spawn된 작업이 에러로 끝나면 stderr에 경고를 출력한다(제어 흐름은 유지).
/// 구조적 동시성상 부모를 중단시키진 않지만, 에러가 조용히 사라지지 않게 한다.
fn warn_if_spawn_err(result: Result<VmValue, RuntimeError>) {
    if let Err(e) = result {
        eprintln!("경고: spawn된 작업에서 처리되지 않은 에러: {e}");
    }
}

fn to_runtime(v: &VmValue) -> crate::runtime::Value {
    use crate::runtime::Value as RV;
    match v {
        VmValue::Int(n)   => RV::Int(*n),
        VmValue::Float(n) => RV::Float(*n),
        VmValue::Bool(b)  => RV::Bool(*b),
        VmValue::Str(s)   => RV::Str(s.clone()),
        VmValue::Nil      => RV::Nil,
        // 컨테이너는 재귀 변환 (채널로 List/Map 전송 지원)
        VmValue::List(items) => RV::List(items.iter().map(to_runtime).collect()),
        VmValue::Map(m) => RV::Map(m.iter().map(|(k, v)| (k.clone(), to_runtime(v))).collect()),
        // 함수/채널/Future/Tcp 등 참조 타입은 채널로 전송 불가 → Nil
        _ => RV::Nil,
    }
}

fn from_runtime(v: crate::runtime::Value) -> VmValue {
    use crate::runtime::Value as RV;
    match v {
        RV::Int(n)   => VmValue::Int(n),
        RV::Float(n) => VmValue::Float(n),
        RV::Bool(b)  => VmValue::Bool(b),
        RV::Str(s)   => VmValue::Str(s),
        RV::Nil      => VmValue::Nil,
        RV::List(items) => VmValue::List(Arc::new(items.into_iter().map(from_runtime).collect())),
        RV::Map(m) => VmValue::Map(Arc::new(m.into_iter().map(|(k, v)| (k, from_runtime(v))).collect())),
        _ => VmValue::Nil,
    }
}
