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
    OP_SET_PATH, OP_UPDATE_VAR, OP_SLICE, OP_MAKE_ITER_PAIRS, OP_IN, OP_UNPACK,
    OP_JUMP_IF_ARG, OP_METHOD_CALL, OP_FMT, OP_BUILD_STR,
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
    /// 작업 경계를 넘어온 Future (Phase 53): 원본의 결과를 처음 꺼낼 때 한 번 격리해 둔다.
    source: Option<Arc<VmFuture>>,
    /// 에러 결과를 누군가 이미 받아 봤는가 (Phase 55) — 스코프 조인이 같은 에러를 두 번 던지지 않게.
    observed: std::sync::atomic::AtomicBool,
}

impl VmFuture {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
            source: None,
            observed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 기다리지 않고 결과 확인 (끝났으면 Some). derived Future는 자기 결과만 본다.
    pub(crate) fn peek(&self) -> Option<Result<VmValue, RuntimeError>> {
        self.result.lock().ok().and_then(|g| g.clone())
    }

    /// 다른 작업으로 넘기는 Future. 결과 안의 클로저 상태가 작업 사이에 공유되지 않게 한다.
    pub(crate) fn derived(src: &Arc<VmFuture>) -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
            source: Some(src.clone()),
            observed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 스코프 종료 시 조인 (Phase 55): 끝날 때까지 기다리고, 아직 아무도 받아 보지 않은 에러만 돌려준다.
    pub(crate) fn join_unobserved(&self) -> Result<(), RuntimeError> {
        match self.wait_result() {
            Err(e) if !self.observed.swap(true, std::sync::atomic::Ordering::SeqCst) => Err(e),
            _ => Ok(()),
        }
    }

    pub fn complete(&self, val: Result<VmValue, RuntimeError>) {
        *self.result.lock().unwrap() = Some(val);
        self.ready.notify_all();
    }

    /// 값으로 꺼내기 (자동 대기·wait). 에러를 받으면 "받아 봤음"으로 표시한다.
    pub fn resolve(&self) -> Result<VmValue, RuntimeError> {
        let r = self.wait_result();
        if r.is_err() {
            self.observed.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        r
    }

    fn wait_result(&self) -> Result<VmValue, RuntimeError> {
        if let Some(src) = &self.source {
            let mut g = self.result.lock()
                .map_err(|_| RuntimeError::no_span("내부 오류: Future 잠금 실패"))?;
            if let Some(r) = &*g {
                return r.clone();
            }
            let r = src.resolve().map(|v| {
                let mut iso = Isolator::new();
                let v = iso.isolate(v);
                iso.finish();
                v
            });
            *g = Some(r.clone());
            return r;
        }
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
    Map(Arc<crate::omap::OrderedMap<VmValue>>),
    // 불변 바이트 열. 변경 연산이 없으므로 Arc 공유가 관찰상 깊은 복사와 동일(값 의미론 유지).
    Bytes(Arc<Vec<u8>>),
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
            VmValue::Bytes(_)    => "Bytes",
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

    /// 참거짓 (Phase 57: Python 규칙) — false·nil·0·0.0·빈 문자열·빈 리스트·빈 맵·빈 bytes는 거짓.
    /// (이전에는 nil·false만 거짓이라 `if not items`가 빈 리스트를 참으로 봤다)
    pub fn is_truthy(&self) -> bool {
        match self {
            VmValue::Bool(b) => *b,
            VmValue::Nil => false,
            VmValue::Int(n) => *n != 0,
            VmValue::Float(x) => *x != 0.0,
            VmValue::Str(s) => !s.is_empty(),
            VmValue::List(l) => !l.is_empty(),
            VmValue::Map(m) => !m.is_empty(),
            VmValue::Bytes(b) => !b.is_empty(),
            _ => true,
        }
    }
}

impl fmt::Display for VmValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmValue::Int(n)    => write!(f, "{n}"),
            VmValue::Float(n)  => write!(f, "{}", fmt_float(*n)),
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
                // 삽입 순서 (Phase 51 — 이전에는 키 정렬)
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {v}", key_display(k))?;
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
            VmValue::Bytes(b)    => crate::runtime::fmt_bytes(f, b),
            VmValue::Builtin(i)  => write!(f, "<builtin {}>", crate::stdlib::builtin_name(*i).unwrap_or("?")),
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
    /// 필수 인자 수 (기본값 매개변수가 있으면 arity보다 작다 — Phase 47)
    pub min_arity: usize,
    /// 이 함수의 소스 파일 (import 모듈이면 그 경로, 메인이면 None) — 에러 위치 표시용 (Phase 56)
    pub file: Option<Arc<str>>,
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
// Builtins
// ============================================================================

// 빌트인 등록 테이블은 crate::stdlib::BUILTINS (이름·인자 수·구현의 단일 출처).
pub use crate::stdlib::builtin_index;

// ============================================================================
// CallFrame
// ============================================================================

pub struct CallFrame {
    pub closure: Arc<VmClosure>,
    pub ip: usize,
    pub locals: Arc<Mutex<Vec<VmValue>>>,
    /// 실제로 전달된 인자 수 (기본값 매개변수 판정용 — OP_JUMP_IF_ARG)
    pub argc: usize,
    /// 이 함수 본문에 뿌리내린 spawn 작업들 (Phase 55) — 함수가 끝날 때 모두 조인한다.
    pub spawned: Vec<Arc<VmFuture>>,
}

/// `parallel {}` 블록의 spawn 스코프. 그 블록이 있는 프레임 깊이를 기억해, 블록 안에서 호출한
/// 다른 함수의 spawn은 그 함수 본문에 뿌리내리게 한다.
pub struct SpawnScope {
    pub frame_depth: usize,
    pub futures: Vec<Arc<VmFuture>>,
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
    /// 핸들러 설정 시점의 parallel 스코프 수 (되감을 때 그 위의 스코프를 조인·정리).
    pub spawn_depth: usize,
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
    /// print의 출력 대상 (Phase 54). spawn한 작업은 부모의 대상을 물려받는다.
    pub output: Output,
    /// 구조적 동시성: spawn 스코프 스택.
    /// 각 항목은 이 스코프 안에서 spawn된 Future 목록.
    /// parallel {} 진입 시 push, 종료 시 pop + join.
    pub spawn_scopes: Vec<SpawnScope>,
    /// 활성 try/catch 핸들러 스택 (innermost = top).
    pub handlers: Vec<TryHandler>,
    /// throw로 던져진 값(있으면). 빌트인/런타임 오류는 None → 메시지 문자열로 변환.
    pub pending_exception: Option<VmValue>,
    /// 호출 프레임 locals 재사용 풀 (호출당 힙 할당 제거 → 할당자 경합 완화).
    /// 클로저에 포착되지 않은(strong_count==1) locals만 회수한다.
    locals_pool: Vec<Arc<Mutex<Vec<VmValue>>>>,
    /// 순환 참조 후보 (Phase 60): 클로저가 붙잡은 채 반환된 프레임의 지역 변수 벡터.
    /// 젊은 세대가 CYCLE_YOUNG_MAX개 모이면 수집하고, 살아남은 것은 늙은 세대로 옮긴다.
    cycle_young: Vec<StateCell>,
    cycle_old: Vec<StateCell>,
    cycle_old_limit: usize,
}

/// 젊은 세대 수집 주기 (후보 수)
const CYCLE_YOUNG_MAX: usize = 2048;
/// 늙은 세대 최소 수집 기준 — 수집 뒤 살아남은 수의 2배로 늘어난다 (분할 상환 O(1))
const CYCLE_OLD_MIN: usize = 8192;

/// print의 출력 대상 (Phase 54).
///
/// 이전에는 print가 화면에 쓰면서 같은 줄을 VM 안의 목록에도 쌓았다. 그 목록은 테스트만 읽으므로
/// 실제 실행에서는 오래 도는 서버의 메모리가 끝없이 늘었고, 모든 스레드의 print가 한 잠금을 두고 경쟁했다.
#[derive(Clone)]
pub enum Output {
    /// 표준 출력에 바로 쓴다 (쌓지 않음) — bang run / REPL / bang test
    Stdout,
    /// 줄을 목록에 모으고 화면에는 쓰지 않는다 — 테스트·내장용
    Capture(Arc<Mutex<Vec<String>>>),
}

impl From<Arc<Mutex<Vec<String>>>> for Output {
    fn from(buf: Arc<Mutex<Vec<String>>>) -> Self {
        Output::Capture(buf)
    }
}

impl Output {
    /// 한 줄 출력. 잠금 실패(다른 스레드 패닉)는 출력 에러로 돌려준다.
    pub fn write_line(&self, line: String) -> Result<(), String> {
        match self {
            Output::Stdout => {
                use std::io::Write;
                let mut out = std::io::stdout().lock();
                match writeln!(out, "{line}") {
                    Ok(()) => Ok(()),
                    // 읽는 쪽이 파이프를 닫음(bang x | head): Unix 관례대로 조용히 끝낸다 (128 + SIGPIPE)
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => std::process::exit(141),
                    Err(e) => Err(format!("표준 출력 쓰기 실패: {e}")),
                }
            }
            Output::Capture(buf) => {
                buf.lock().map_err(|_| "출력 버퍼 잠금 실패".to_string())?.push(line);
                Ok(())
            }
        }
    }
}

impl Vm {
    pub fn new(global_count: usize, output: impl Into<Output>) -> Self {
        let output = output.into();
        Self {
            stack: Vec::with_capacity(256),
            frames: Vec::with_capacity(64),
            globals: Arc::new(Mutex::new(vec![VmValue::Nil; global_count])),
            output,
            spawn_scopes: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            locals_pool: Vec::new(),
            cycle_young: Vec::new(),
            cycle_old: Vec::new(),
            cycle_old_limit: CYCLE_OLD_MIN,
        }
    }

    /// 기존 전역 배열(Arc)을 공유하는 VM 생성 — REPL이 스니펫 간 상태를 유지하는 데 쓴다.
    /// 호출자가 실행 전에 전역 배열 크기를 global_count 이상으로 맞춰야 한다.
    pub fn with_globals(
        globals: Arc<Mutex<Vec<VmValue>>>,
        output: impl Into<Output>,
    ) -> Self {
        let output = output.into();
        Self {
            stack: Vec::with_capacity(256),
            frames: Vec::with_capacity(64),
            globals,
            output,
            spawn_scopes: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            locals_pool: Vec::new(),
            cycle_young: Vec::new(),
            cycle_old: Vec::new(),
            cycle_old_limit: CYCLE_OLD_MIN,
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
        if Arc::strong_count(&locals) == 1 {
            if self.locals_pool.len() < 256 {
                self.locals_pool.push(locals);
            }
            return;
        }
        // 클로저가 이 지역 변수들을 붙잡았다 → 순환 후보 (객체가 자기 변수를 붙잡거나 지역 재귀 함수)
        self.cycle_young.push(locals);
        if self.cycle_young.len() >= CYCLE_YOUNG_MAX {
            self.collect_cycles();
        }
    }

    /// 순환 참조 수집 (Phase 60, src/cycles.rs). 젊은 세대를 보고, 늙은 세대가 기준을 넘으면 함께 본다.
    pub(crate) fn collect_cycles(&mut self) {
        let mut candidates = std::mem::take(&mut self.cycle_young);
        let with_old = self.cycle_old.len() >= self.cycle_old_limit;
        if with_old {
            candidates.append(&mut self.cycle_old);
        }
        let r = crate::cycles::collect(candidates);
        self.cycle_old.extend(r.survivors);
        if with_old {
            self.cycle_old_limit = (self.cycle_old.len() * 2).max(CYCLE_OLD_MIN);
        }
    }

    /// 이 VM의 모든 순환 후보를 꺼낸다 (작업이 끝날 때 VM을 버린 뒤 수집하려고).
    fn take_cycle_candidates(&mut self) -> Vec<StateCell> {
        let mut all = std::mem::take(&mut self.cycle_young);
        all.append(&mut self.cycle_old);
        all
    }

    pub fn run(&mut self, main_fn: Arc<CompiledFn>) -> Result<(), RuntimeError> {
        let closure = Arc::new(VmClosure {
            func: main_fn.clone(),
            upvalues: Vec::new(),
            globals: self.globals.clone(),
        });
        let locals = Arc::new(Mutex::new(vec![VmValue::Nil; main_fn.local_count]));
        self.frames.push(CallFrame { closure, ip: 0, locals, argc: 0, spawned: Vec::new() });
        // 최상위 spawn은 메인 프레임에 뿌리내려 프로그램 끝(메인 반환)에 조인된다.
        // 에러로 빠져나가도 남은 작업을 모두 기다린다 (누수 방지).
        let r = self.exec_until(0);
        if r.is_err() {
            self.drain_pending_tasks();
        }
        r
    }

    /// 에러로 실행을 끝낼 때: 남은 프레임·parallel 스코프의 작업을 모두 기다린다(누수 방지).
    /// 이미 다른 에러가 전파 중이므로 작업의 에러는 경고로만 알린다.
    fn drain_pending_tasks(&mut self) {
        while let Some(f) = self.frames.pop() {
            for t in f.spawned { warn_if_spawn_err(t.join_unobserved()); }
        }
        while let Some(s) = self.spawn_scopes.pop() {
            for t in s.futures { warn_if_spawn_err(t.join_unobserved()); }
        }
    }

    /// 작업 목록을 모두 조인한 뒤, 아직 아무도 받아 보지 않은 첫 에러를 돌려준다.
    fn join_tasks(tasks: Vec<Arc<VmFuture>>) -> Result<(), RuntimeError> {
        let mut first = None;
        for t in tasks {
            if let Err(e) = t.join_unobserved() {
                if first.is_none() { first = Some(e); }
            }
        }
        match first { Some(e) => Err(e), None => Ok(()) }
    }

    /// `bang test`용: 전역 슬롯의 값이 클로저면 인자 없이 **격리 실행**한다 (spawn과 같은 격리 —
    /// 테스트끼리 전역 변경이 보이지 않는다). 클로저가 아니면 None.
    pub fn run_global_fn_isolated(&self, slot: usize) -> Option<Result<VmValue, RuntimeError>> {
        let value = self.globals.lock().ok()?.get(slot)?.clone();
        match value {
            VmValue::Closure(c) => {
                let (iso, cells) = isolate_closure(&c);
                let r = Vm::run_spawned(self.output.clone(), iso, Vec::new(), Span::new(0, 0));
                crate::cycles::collect(cells);
                Some(r)
            }
            _ => None,
        }
    }

    /// spawned 클로저를 서브-VM에서 실행하고 결과 반환.
    /// std::thread::spawn 클로저 안에서 호출된다.
    /// `call_span`: 작업을 시작한 지점 (인자 수 에러 등 작업 시작 전 에러의 위치).
    pub fn run_spawned(
        output: Output,
        closure: Arc<VmClosure>,
        args: Vec<VmValue>,
        call_span: Span,
    ) -> Result<VmValue, RuntimeError> {
        // spawned 클로저는 이미 자기 모듈 전역의 독립 복사본을 들고 있다
        // (deep_clone_closure에서 깊은 복사). 그 Arc를 루트 전역으로 쓴다.
        let mut vm = Vm {
            stack: Vec::with_capacity(64),
            frames: Vec::with_capacity(16),
            globals: closure.globals.clone(),
            output,
            spawn_scopes: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            locals_pool: Vec::new(),
            cycle_young: Vec::new(),
            cycle_old: Vec::new(),
            cycle_old_limit: CYCLE_OLD_MIN,
        };
        let argc = args.len();
        if argc < closure.func.min_arity || argc > closure.func.arity {
            let mut e = arity_error(&closure.func, argc, call_span);
            e.file = closure.func.file.clone();
            return Err(e);
        }
        let local_count = closure.func.local_count;
        let mut locals_vec = vec![VmValue::Nil; local_count];
        for (i, arg) in args.into_iter().enumerate() {
            if i < locals_vec.len() { locals_vec[i] = arg; }
        }
        let locals = Arc::new(Mutex::new(locals_vec));
        vm.frames.push(CallFrame { closure, ip: 0, locals, argc, spawned: Vec::new() });
        // 작업의 자식들은 작업 함수 본문에 뿌리내려 반환 시 조인된다 (에러는 이 작업의 에러가 됨)
        let result = match vm.exec_until(0) {
            Ok(()) => Ok(vm.stack.pop().unwrap_or(VmValue::Nil)),
            Err(e) => {
                vm.drain_pending_tasks();
                Err(e)
            }
        };
        // 작업이 끝났다: VM을 버린 뒤 작업 안에서 생긴 순환을 수집 (결과 값은 외부 참조로 살아 있다)
        let candidates = vm.take_cycle_candidates();
        drop(vm);
        crate::cycles::collect(candidates);
        result
    }

    /// 모듈 파일을 읽어 sub-VM에서 실행하고 export 맵을 만들어 캐시에 넣는다.
    /// (호출자는 module_loading 표시/해제로 순환 import를 감지한다.)
    pub(crate) fn import_module(&mut self, path: &str, key: &str, span: Span)
        -> Result<VmValue, RuntimeError>
    {
        let source = std::fs::read_to_string(path)
            .map_err(|e| RuntimeError::new(format!("import(): '{path}': {e}"), span).with_kind(crate::runtime::kind::IMPORT))?;
        let tokens = crate::lexer::Lexer::new(&source)
            .tokenize()
            .map_err(|e| RuntimeError::new(
                format!("import(): 렉서 오류 in '{path}': {}", e.first().map(|x| x.to_string()).unwrap_or_default()), span).with_kind(crate::runtime::kind::IMPORT))?;
        let prog = crate::parser::Parser::new(tokens)
            .parse()
            .map_err(|e| RuntimeError::new(
                format!("import(): 파서 오류 in '{path}': {}", e.first().map(|x| x.to_string()).unwrap_or_default()), span).with_kind(crate::runtime::kind::IMPORT))?;
        let out = crate::compiler::compile_with_file(&prog, Some(path))
            .map_err(|e| RuntimeError::new(
                format!("import(): 컴파일 오류 in '{path}': {}", e.first().map(|x| x.to_string()).unwrap_or_default()), span).with_kind(crate::runtime::kind::IMPORT))?;
        // 모듈의 top-level print는 메인 프로그램 출력으로 보낸다.
        let mut sub_vm = Vm::new(out.global_count as usize, self.output.clone());
        sub_vm.run(out.main_fn)
            .map_err(|e| RuntimeError::new(format!("import(): 모듈 실행 오류 in '{path}': {e}"), span).with_kind(crate::runtime::kind::IMPORT))?;
        // 모듈의 export(최상위 바인딩)를 Map으로. 함수 값은 sub_vm의
        // 모듈 전역 Arc를 그대로 들고 있어, 메인 VM에서 호출돼도
        // 자기 모듈 전역을 참조한다(sub_vm 드롭 후에도 Arc로 유지).
        let mut map = crate::omap::OrderedMap::new();
        {
            let g = sub_vm.globals.lock().unwrap();
            // 모듈 export 맵은 선언 순서(전역 슬롯 순)
            let mut names: Vec<(&String, &u16)> = out.global_names.iter().collect();
            names.sort_by_key(|(_, s)| **s);
            for (name, slot) in names {
                map.insert(name.clone(), g[*slot as usize].clone());
            }
        }
        let result = VmValue::Map(Arc::new(map));
        module_cache().lock().unwrap().insert(key.to_string(), result.clone());
        Ok(result)
    }

    /// 논블로킹 이벤트 루프 서버. 단일 스레드가 리액터로 다수 연결을 멀티플렉싱한다.
    /// handler: fn(req_str) -> resp_str (전체 HTTP 응답 문자열을 반환).
    /// 유휴 keep-alive 연결은 리액터에 파킹되어 스레드를 점유하지 않는다(C10K 1차).
    /// 주의: 단일 스레드라 CPU 무거운 핸들러는 루프를 막는다(향후 풀 오프로드 과제).
    pub(crate) fn run_event_server(&self, addr: &str, handler: Arc<VmClosure>, span: Span)
        -> Result<VmValue, RuntimeError>
    {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};
        let err = |m: String| RuntimeError::new(format!("serve_event: {m}"), span);

        let listener = TcpListener::bind(addr).map_err(|e| err(format!("bind '{addr}': {e}")))?;
        listener.set_nonblocking(true).map_err(|e| err(format!("set_nonblocking: {e}")))?;
        let mut reactor = crate::reactor::Reactor::new().map_err(|e| err(format!("reactor: {e}")))?;
        const LKEY: usize = 0;
        reactor.add_readable(&listener, LKEY).map_err(|e| err(format!("register listener: {e}")))?;

        let mut conns: HashMap<usize, TcpStream> = HashMap::new();
        let mut bufs: HashMap<usize, Vec<u8>> = HashMap::new();
        let mut next_key: usize = 1;

        loop {
            let ready = reactor.wait(None).map_err(|e| err(format!("wait: {e}")))?;
            for key in ready {
                if key == LKEY {
                    // 대기 중인 새 연결을 모두 accept (논블로킹)
                    loop {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let _ = stream.set_nonblocking(true);
                                let k = next_key; next_key += 1;
                                if reactor.add_readable(&stream, k).is_ok() {
                                    conns.insert(k, stream);
                                    bufs.insert(k, Vec::new());
                                }
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    }
                    let _ = reactor.modify_readable(&listener, LKEY); // oneshot 재무장
                    continue;
                }

                // 연결에서 읽기 → 요청 완성 시 핸들러 호출 → 응답 쓰기
                let mut close = false;
                let mut response: Option<Vec<u8>> = None;
                if let Some(stream) = conns.get_mut(&key) {
                    let mut tmp = [0u8; 4096];
                    match stream.read(&mut tmp) {
                        Ok(0) => close = true, // EOF
                        Ok(n) => {
                            let buf = bufs.entry(key).or_default();
                            buf.extend_from_slice(&tmp[..n]);
                            if find_subseq(buf, b"\r\n\r\n").is_some() {
                                let req = String::from_utf8_lossy(buf).into_owned();
                                buf.clear();
                                let (h, cells) = isolate_closure(&handler);
                                let handled = Vm::run_spawned(self.output.clone(), h, vec![VmValue::Str(req)], span);
                                crate::cycles::collect(cells);
                                match handled {
                                    Ok(VmValue::Str(s)) => response = Some(s.into_bytes()),
                                    Ok(other) => response = Some(format!("{other}").into_bytes()),
                                    Err(e) => {
                                        eprintln!("경고: serve_event 핸들러 에러: {e}");
                                        close = true;
                                    }
                                }
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => close = true,
                    }
                }
                if let Some(bytes) = response {
                    if let Some(stream) = conns.get_mut(&key) {
                        if stream.write_all(&bytes).is_err() { close = true; }
                    }
                }
                if close {
                    if let Some(s) = conns.remove(&key) { let _ = reactor.deregister(&s); }
                    bufs.remove(&key);
                } else if let Some(s) = conns.get(&key) {
                    let _ = reactor.modify_readable(s, key); // keep-alive: 재무장
                }
            }
        }
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

    pub(crate) fn stack_pop(&mut self) -> VmValue {
        self.stack.pop().expect("VM: empty stack on pop")
    }

    fn stack_peek(&self) -> &VmValue {
        self.stack.last().expect("VM: empty stack on peek")
    }

    /// 변수 위치(kind, slot) → (그 값을 담은 벡터, 벡터 내 인덱스). OP_SET_PATH/OP_UPDATE_VAR용.
    fn var_cell(&self, kind: u8, slot: usize) -> (Arc<Mutex<Vec<VmValue>>>, usize) {
        let fi = self.frames.len() - 1;
        match kind {
            0 => (self.frames[fi].locals.clone(), slot),
            1 => {
                let uv = &self.frames[fi].closure.upvalues[slot];
                (uv.locals.clone(), uv.slot)
            }
            _ => (self.frames[fi].closure.globals.clone(), slot),
        }
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
    pub(crate) fn exec_until(&mut self, stop_depth: usize) -> Result<(), RuntimeError> {
        loop {
            match self.exec_dispatch(stop_depth) {
                Ok(()) => return Ok(()),
                Err(mut e) => {
                    // 위치가 없는 에러에는 실행 중이던 지점을, 파일은 그 함수의 소스 파일을 붙인다 (헌법 7조, Phase 56)
                    if e.span.line == 0 && !self.frames.is_empty() {
                        e.span = self.current_span();
                    }
                    if e.file.is_none() {
                        e.file = self.frames.last().and_then(|f| f.closure.func.file.clone());
                    }
                    if let Some(h) = self.take_handler_above(stop_depth) {
                        // 사용자 throw는 던진 값 그대로, 내장 에러는 {kind, message, line, col} 맵
                        let exc = self.pending_exception.take()
                            .unwrap_or_else(|| error_to_value(&e));
                        self.unwind_to(&h, exc);
                        // 루프 계속 → catch 블록에서 재개
                    } else if let Some(v) = self.pending_exception.take() {
                        // 다시 던진 내장 에러 맵이면 원래 에러로 복원 (메시지·위치 보존)
                        if let Some(orig) = rethrown_error(&v) {
                            return Err(orig);
                        }
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
        // 되감기는 프레임·parallel 스코프의 작업도 모두 기다린다 (작업 누수 불가 — 헌법 3조).
        // 예외가 이미 전파 중이므로 그 작업들의 에러는 경고로만 알린다.
        while self.frames.len() > h.frame_depth {
            if let Some(f) = self.frames.pop() {
                for t in f.spawned { warn_if_spawn_err(t.join_unobserved()); }
                self.release_locals(f.locals);
            }
        }
        while self.spawn_scopes.len() > h.spawn_depth {
            if let Some(s) = self.spawn_scopes.pop() {
                for t in s.futures { warn_if_spawn_err(t.join_unobserved()); }
            }
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
                    // Implicit nil return — 함수 본문에 뿌리내린 작업을 조인 (Phase 55)
                    if let Some(f) = self.frames.pop() {
                        self.release_locals(f.locals);
                        if !f.spawned.is_empty() {
                            Self::join_tasks(f.spawned)?;
                        }
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
                        VmValue::Int(n)   => VmValue::Int(n.checked_neg()
                            .ok_or_else(|| overflow_err("단항 -", span))?),
                        VmValue::Float(n) => VmValue::Float(-n),
                        other => return Err(RuntimeError::new(
                            format!("단항 -: 숫자 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
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
                        // 함수 본문에 뿌리내린 작업을 모두 조인 — 받아 보지 않은 에러는 호출자에게 전파 (Phase 55)
                        if !f.spawned.is_empty() {
                            Self::join_tasks(f.spawned)?;
                        }
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
                    let span = self.current_span();
                    let mut map = crate::omap::OrderedMap::new();
                    for pair in flat.chunks(2) {
                        map.insert(map_key(&pair[0], span)?, pair[1].clone());
                    }
                    self.stack.push(VmValue::Map(Arc::new(map)));
                }

                OP_INDEX_GET => {
                    let span = self.current_span();
                    // 인덱스/필드 대상은 값이 필요한 지점 — Future 자동 대기 (SPEC 6장)
                    let idx = auto_resolve(self.stack_pop(), span)?;
                    let target = auto_resolve(self.stack_pop(), span)?;
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
                OP_SET_PATH => {
                    // stack: [root 복사본, k1..kN, val]
                    let span = self.current_span();
                    let slot = self.read_u16() as usize;
                    let kind = self.read_byte();
                    let depth = self.read_byte() as usize;
                    let binop = self.read_byte();
                    let val = self.stack_pop();
                    let mut keys = self.stack.split_off(self.stack.len() - depth);
                    for k in keys.iter_mut() {
                        *k = auto_resolve(std::mem::replace(k, VmValue::Nil), span)?;
                    }
                    let root_copy = self.stack_pop();
                    let (cell, idx) = self.var_cell(kind, slot);
                    let mut guard = cell.lock().map_err(|_| lock_err(span))?;
                    let slot_ref = guard.get_mut(idx).ok_or_else(|| bad_slot(span))?;
                    // 슬롯이 아직 같은 값이면 복사본을 버려 참조 수를 1로 → 제자리 변경.
                    // (키·값 평가 중 변수가 재대입됐다면 원래 의미대로 복사본을 고쳐 저장)
                    let prev = std::mem::replace(slot_ref, root_copy);
                    let restore = if same_container(&prev, slot_ref) { drop(prev); None } else { Some(prev) };
                    let result = match set_path(slot_ref, &keys, val, binop, span) {
                        Ok(v) => v,
                        Err(e) => {
                            if let Some(p) = restore { *slot_ref = p; }
                            return Err(e);
                        }
                    };
                    drop(guard);
                    self.stack.push(result);
                }
                OP_UPDATE_VAR => {
                    // stack: [x 복사본, e]
                    let span = self.current_span();
                    let slot = self.read_u16() as usize;
                    let kind = self.read_byte();
                    let op = self.read_byte();
                    let rhs = self.stack_pop();
                    let cur_copy = auto_resolve(self.stack_pop(), span)?;
                    let (cell, idx) = self.var_cell(kind, slot);
                    let mut guard = cell.lock().map_err(|_| lock_err(span))?;
                    let slot_ref = guard.get_mut(idx).ok_or_else(|| bad_slot(span))?;
                    let in_place = matches!((&cur_copy, slot_ref as &VmValue),
                        (VmValue::List(a), VmValue::List(b)) if Arc::ptr_eq(a, b));
                    let new_val = if in_place {
                        drop(cur_copy); // 슬롯만 남김 → make_mut이 복사하지 않는다
                        match (op, &mut *slot_ref) {
                            (0, VmValue::List(items)) => Arc::make_mut(items).push(rhs),
                            (_, VmValue::List(items)) => match auto_resolve(rhs, span)? {
                                VmValue::List(r) => Arc::make_mut(items).extend(r.iter().cloned()),
                                other => {
                                    let l = VmValue::List(items.clone());
                                    *slot_ref = vm_add(l, other, span)?;
                                }
                            },
                            _ => {}
                        }
                        slot_ref.clone()
                    } else {
                        let v = if op == 0 {
                            match cur_copy {
                                VmValue::List(mut items) => { Arc::make_mut(&mut items).push(rhs); VmValue::List(items) }
                                other => return Err(RuntimeError::new(
                                    format!("push(): List 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
                            }
                        } else {
                            vm_add(cur_copy, auto_resolve(rhs, span)?, span)?
                        };
                        *slot_ref = v.clone();
                        v
                    };
                    drop(guard);
                    self.stack.push(new_val);
                }
                OP_FMT => {
                    let span = self.current_span();
                    let idx = self.read_u16();
                    let spec_src = match self.get_constant(idx) {
                        VmValue::Str(s) => s,
                        _ => return Err(RuntimeError::no_span("OP_FMT: non-string spec")),
                    };
                    let v = deep_resolve(self.stack_pop(), span)?;
                    let spec = crate::stdlib::parse_format_spec(&spec_src).map_err(|m| {
                        RuntimeError::new(format!("형식 '{spec_src}': {m}"), span).with_kind(crate::runtime::kind::VALUE)
                    })?;
                    let s = crate::stdlib::apply_format(&v, &spec).map_err(|m| {
                        RuntimeError::new(format!("형식 '{spec_src}': {m}"), span).with_kind(crate::runtime::kind::VALUE)
                    })?;
                    self.stack.push(VmValue::Str(s));
                }
                OP_BUILD_STR => {
                    let span = self.current_span();
                    let n = self.read_byte() as usize;
                    let parts = self.stack.split_off(self.stack.len() - n);
                    let mut out = String::new();
                    for p in parts {
                        // 값이 필요한 지점 → Future는 (print처럼) 깊게 해소
                        let v = deep_resolve(p, span)?;
                        out.push_str(&v.to_string());
                    }
                    self.stack.push(VmValue::Str(out));
                }
                OP_JUMP_IF_ARG => {
                    let idx = self.read_byte() as usize;
                    let offset = self.read_i16();
                    let fi = self.frames.len() - 1;
                    if self.frames[fi].argc > idx {
                        self.jump_by(offset);
                    }
                }
                OP_METHOD_CALL => {
                    // stack: [x, args..]
                    let span = self.current_span();
                    let name_idx = self.read_u16();
                    let argc = self.read_byte() as usize;
                    let discarded = self.read_byte() != 0;
                    let name = match self.get_constant(name_idx) {
                        VmValue::Str(s) => s,
                        _ => return Err(RuntimeError::no_span("OP_METHOD_CALL: non-string name")),
                    };
                    let base = self.stack.len() - argc - 1;
                    let target = auto_resolve(std::mem::replace(&mut self.stack[base], VmValue::Nil), span)?;
                    // 1) 맵 "객체"의 키가 우선 (맵 + 클로저 관용구)
                    let member = match &target {
                        VmValue::Map(m) => m.get(&encode_str_key(&name)).cloned(),
                        _ => None,
                    };
                    if let Some(member) = member {
                        self.stack[base] = member;
                        self.do_call(argc, span, stop_depth)?;
                    } else {
                    // 2) 빌트인 name(x, args..) — UFCS
                    let Some(idx) = builtin_index(&name) else {
                        return Err(RuntimeError::new(
                            format!("메서드 없음: {} 에 '{name}' 없음", target.type_name()), span).with_kind(crate::runtime::kind::TYPE));
                    };
                    if discarded && crate::resolver::PURE_BUILTINS.contains(&name.as_str()) {
                        return Err(RuntimeError::new(format!(
                            "x.{name}(...)의 결과가 버려짐 — {name}은(는) 새 값을 돌려줄 뿐 x를 바꾸지 않습니다. \
                             x = x.{name}(...)처럼 대입하세요 (리스트 추가는 xs += [v])"), span).with_kind(crate::runtime::kind::VALUE));
                    }
                    self.stack[base] = target;
                    self.stack.insert(base, VmValue::Builtin(idx));
                    self.do_call(argc + 1, span, stop_depth)?;
                    }
                }
                OP_SLICE => {
                    let span = self.current_span();
                    let end = auto_resolve(self.stack_pop(), span)?;
                    let start = auto_resolve(self.stack_pop(), span)?;
                    let target = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(vm_slice(target, &start, &end, span)?);
                }
                OP_IN => {
                    let span = self.current_span();
                    let container = auto_resolve(self.stack_pop(), span)?;
                    let needle = auto_resolve(self.stack_pop(), span)?;
                    self.stack.push(VmValue::Bool(vm_contains(&container, &needle, span)?));
                }
                OP_UNPACK => {
                    let span = self.current_span();
                    let n = self.read_byte() as usize;
                    match auto_resolve(self.stack_pop(), span)? {
                        VmValue::List(items) if items.len() == n => {
                            self.stack.extend(items.iter().cloned());
                        }
                        VmValue::List(items) => return Err(RuntimeError::new(
                            format!("분해: 값 {n}개 기대, {}개 발견", items.len()), span).with_kind(crate::runtime::kind::VALUE)),
                        other => return Err(RuntimeError::new(
                            format!("분해: List 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::VALUE)),
                    }
                }
                OP_FIELD_GET => {
                    let span = self.current_span();
                    let name_idx = self.read_u16();
                    let name = match self.get_constant(name_idx) {
                        VmValue::Str(s) => s,
                        _ => return Err(RuntimeError::no_span("OP_FIELD_GET: non-string name")),
                    };
                    let target = auto_resolve(self.stack_pop(), span)?;
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
                            // 호출 대상과 인자를 한 격리기로 — 인자 안의 객체가 대상과 상태를 공유하면 복사본에서도 공유
                            let mut iso = Isolator::new();
                            let closure_copy = iso.closure(&closure);
                            let args_copy: Vec<VmValue> = args.into_iter().map(|a| iso.isolate(a)).collect();
                            iso.finish();
                            let cells = iso.new_cells();
                            drop(iso);
                            let output_copy  = self.output.clone();
                            let future = VmFuture::new();
                            let future2 = future.clone();
                            // M:N 스케줄러에 태스크 제출 (Phase 9 Part A)
                            crate::scheduler::global().spawn_task(move || {
                                let result = Vm::run_spawned(
                                    output_copy, closure_copy, args_copy, span);
                                // 작업용으로 복사한 상태(모듈 전역 ↔ 함수 순환)를 정리 (Phase 60 — 이전엔 spawn마다 누수)
                                crate::cycles::collect(cells);
                                future2.complete(result);
                            });
                            // 가장 가까운 스코프에 등록: 같은 프레임의 parallel 블록이면 그 블록, 아니면 함수 본문
                            let depth = self.frames.len();
                            match self.spawn_scopes.last_mut() {
                                Some(scope) if scope.frame_depth == depth => scope.futures.push(future.clone()),
                                _ => {
                                    if let Some(frame) = self.frames.last_mut() {
                                        frame.spawned.push(future.clone());
                                    }
                                }
                            }
                            self.stack.push(VmValue::Future(future));
                        }
                        other => {
                            return Err(RuntimeError::new(
                                format!("spawn: 클로저 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::TYPE));
                        }
                    }
                }
                OP_PARALLEL_ENTER => {
                    self.spawn_scopes.push(SpawnScope { frame_depth: self.frames.len(), futures: Vec::new() });
                }
                OP_PARALLEL_EXIT => {
                    // 모든 spawn을 끝까지 조인한 뒤 첫 에러 전파 (이전: 첫 에러에서 멈춰 나머지를 기다리지 않음)
                    if let Some(scope) = self.spawn_scopes.pop() {
                        Self::join_tasks(scope.futures)?;
                    }
                }

                // --- try / catch / throw ---
                OP_SETUP_TRY => {
                    let catch_ip = self.read_u16() as usize; // 절대 위치
                    self.handlers.push(TryHandler {
                        frame_depth: self.frames.len(),
                        stack_len: self.stack.len(),
                        catch_ip,
                        spawn_depth: self.spawn_scopes.len(),
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
                            format!("타입 불일치: {exp} 기대, {} 받음", v.type_name()), span).with_kind(crate::runtime::kind::TYPE));
                    }
                    self.stack.push(v);
                }

                // --- For loop ---
                OP_MAKE_ITER | OP_MAKE_ITER_PAIRS => {
                    let span = self.current_span();
                    let pairs = opcode == OP_MAKE_ITER_PAIRS;
                    let val = auto_resolve(self.stack_pop(), span)?;
                    let iter = match val {
                        VmValue::List(items) => VmIter::List { items, idx: 0 },
                        VmValue::Channel(ch) => VmIter::Channel(ch),
                        // 맵: 키 정렬 순서(keys()와 동일). 변수 2개면 [키, 값] 쌍.
                        VmValue::Map(m) => {
                            // 삽입 순서 (Phase 51 — 이전에는 키 정렬)
                            let items = m.keys().map(|k| {
                                let key = decode_key(k);
                                if pairs {
                                    let v = m.get(k).cloned().unwrap_or(VmValue::Nil);
                                    VmValue::List(Arc::new(vec![key, v]))
                                } else {
                                    key
                                }
                            }).collect();
                            VmIter::List { items: Arc::new(items), idx: 0 }
                        }
                        VmValue::Str(s) => VmIter::List {
                            items: Arc::new(s.chars().map(|c| VmValue::Str(c.to_string())).collect()),
                            idx: 0,
                        },
                        VmValue::Bytes(b) => VmIter::List {
                            items: Arc::new(b.iter().map(|&x| VmValue::Int(x as i64)).collect()),
                            idx: 0,
                        },
                        other => return Err(RuntimeError::new(
                            format!("for-in: List/Map/Str/Bytes/Channel 필요, {} 발견", other.type_name()),
                            span).with_kind(crate::runtime::kind::TYPE)),
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

    pub(crate) fn do_call(&mut self, arg_count: usize, span: Span, stop_depth: usize) -> Result<(), RuntimeError> {
        let callee_idx = self.stack.len() - arg_count - 1;
        let callee = self.stack[callee_idx].clone();

        match callee {
            VmValue::Closure(closure) => {
                if arg_count < closure.func.min_arity || arg_count > closure.func.arity {
                    return Err(arity_error(&closure.func, arg_count, span));
                }
                let callee_idx = self.stack.len() - arg_count - 1;
                let base = callee_idx + 1; // 첫 인자 위치

                // 풀에서 locals를 가져와 인자를 슬롯 0..arg_count에 채운다 (중간 Vec 없음)
                let local_count = closure.func.local_count;
                let locals = self.acquire_locals(local_count);
                if let Ok(mut g) = locals.lock() {
                    for i in 0..arg_count {
                        g[i] = self.stack[base + i].clone();
                    }
                }
                self.stack.truncate(callee_idx); // 인자 + callee 제거
                self.frames.push(CallFrame { closure, ip: 0, locals, argc: arg_count, spawned: Vec::new() });
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
                    format!("호출할 수 없는 값: {}", other.type_name()), span).with_kind(crate::runtime::kind::TYPE));
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
        match crate::stdlib::BUILTINS.get(idx) {
            Some(b) => (b.func)(self, args, span, stop_depth)
                .map_err(|e| crate::stdlib::categorize_error(b.name, e)),
            None => Err(RuntimeError::new(format!("알 수 없는 내장 함수 인덱스: {idx}"), span)),
        }
    }
}

// ============================================================================
// Pure helpers (free functions)
// ============================================================================

/// 정수 오버플로 에러 (try/catch로 잡힘). debug/release 의미 동일.
pub(crate) fn overflow_err(op: &str, span: Span) -> RuntimeError {
    RuntimeError::new(format!("정수 오버플로: {op} 결과가 64비트 범위를 벗어남"), span).with_kind(crate::runtime::kind::OVERFLOW)
}

fn vm_add(l: VmValue, r: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match (l, r) {
        (VmValue::Int(a),   VmValue::Int(b))   => a.checked_add(b)
            .map(VmValue::Int).ok_or_else(|| overflow_err("+", span)),
        (VmValue::Float(a), VmValue::Float(b)) => Ok(VmValue::Float(a + b)),
        (VmValue::Int(a),   VmValue::Float(b)) => Ok(VmValue::Float(a as f64 + b)),
        (VmValue::Float(a), VmValue::Int(b))   => Ok(VmValue::Float(a + b as f64)),
        (VmValue::Str(a),   VmValue::Str(b))   => Ok(VmValue::Str(a + &b)),
        (VmValue::List(mut a), VmValue::List(b)) => { Arc::make_mut(&mut a).extend(b.iter().cloned()); Ok(VmValue::List(a)) }
        (VmValue::Bytes(mut a), VmValue::Bytes(b)) => { Arc::make_mut(&mut a).extend_from_slice(&b); Ok(VmValue::Bytes(a)) }
        (l, r) => Err(RuntimeError::new(
            format!("+: {} + {} 연산 불가", l.type_name(), r.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
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
            '-' => a.checked_sub(*b).map(VmValue::Int).ok_or_else(|| overflow_err("-", span)),
            '*' => a.checked_mul(*b).map(VmValue::Int).ok_or_else(|| overflow_err("*", span)),
            '/' => {
                if *b == 0 { return Err(RuntimeError::new("0으로 나눌 수 없음", span).with_kind(crate::runtime::kind::ZERO_DIVISION)); }
                // checked_rem: i64::MIN % -1 도 오버플로(debug panic) → checked로 방어
                // `/`는 항상 float (Phase 57). 이전: 나누어떨어지면 int, 아니면 float — 값에 따라 타입이 바뀌었다.
                // 정수 나눗셈은 idiv(a, b).
                Ok(VmValue::Float(*a as f64 / *b as f64))
            }
            '%' => {
                if *b == 0 { return Err(RuntimeError::new("나머지: 0으로 나눌 수 없음", span).with_kind(crate::runtime::kind::ZERO_DIVISION)); }
                a.checked_rem(*b).map(VmValue::Int).ok_or_else(|| overflow_err("%", span))
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
            format!("{op}: {} {op} {} 연산 불가", l.type_name(), r.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

pub(crate) fn vm_eq(l: &VmValue, r: &VmValue) -> bool {
    match (l, r) {
        (VmValue::Int(a),   VmValue::Int(b))   => a == b,
        (VmValue::Float(a), VmValue::Float(b)) => a == b,
        (VmValue::Int(a),   VmValue::Float(b)) => (*a as f64) == *b,
        (VmValue::Float(a), VmValue::Int(b))   => *a == (*b as f64),
        (VmValue::Bool(a),  VmValue::Bool(b))  => a == b,
        (VmValue::Str(a),   VmValue::Str(b))   => a == b,
        (VmValue::Bytes(a), VmValue::Bytes(b)) => a == b,
        (VmValue::Nil,      VmValue::Nil)       => true,
        (VmValue::List(a),  VmValue::List(b))  =>
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| vm_eq(x, y)),
        // 맵도 값이다: 같은 키 집합 + 키마다 같은 값 (Phase 49 이전엔 항상 false였던 결함)
        (VmValue::Map(a),   VmValue::Map(b))   =>
            a.len() == b.len() && a.iter().all(|(k, x)| b.get(k).is_some_and(|y| vm_eq(x, y))),
        // 참조 타입은 동일성(같은 Arc)으로 비교
        (VmValue::Closure(a), VmValue::Closure(b)) => Arc::ptr_eq(a, b),
        (VmValue::Builtin(a), VmValue::Builtin(b)) => a == b,
        (VmValue::Channel(a), VmValue::Channel(b)) => Arc::ptr_eq(a, b),
        (VmValue::Future(a),  VmValue::Future(b))  => Arc::ptr_eq(a, b),
        (VmValue::TcpListener(a), VmValue::TcpListener(b)) => Arc::ptr_eq(a, b),
        (VmValue::TcpConn(a), VmValue::TcpConn(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

pub(crate) fn vm_cmp(l: &VmValue, r: &VmValue, span: Span) -> Result<Ordering, RuntimeError> {
    match (l, r) {
        (VmValue::Int(a),   VmValue::Int(b))   => Ok(a.cmp(b)),
        (VmValue::Float(a), VmValue::Float(b)) => Ok(a.partial_cmp(b).unwrap_or(Ordering::Equal)),
        (VmValue::Int(a),   VmValue::Float(b)) => Ok((*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)),
        (VmValue::Float(a), VmValue::Int(b))   => Ok(a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)),
        (VmValue::Str(a),   VmValue::Str(b))   => Ok(a.cmp(b)),
        (l, r) => Err(RuntimeError::new(
            format!("비교: {} 와 {} 비교 불가", l.type_name(), r.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

fn vm_index_get(target: VmValue, idx: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match (target, &idx) {
        (VmValue::List(items), VmValue::Int(i)) => {
            let len = items.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("인덱스 범위 초과: {i} (길이 {len})"), span).with_kind(crate::runtime::kind::INDEX));
            }
            Ok(items[i as usize].clone())
        }
        (VmValue::Str(s), VmValue::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("문자열 인덱스 범위 초과: {i} (길이 {len})"), span).with_kind(crate::runtime::kind::INDEX));
            }
            Ok(VmValue::Str(chars[i as usize].to_string()))
        }
        (VmValue::Bytes(b), VmValue::Int(i)) => {
            let len = b.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("바이트 인덱스 범위 초과: {i} (길이 {len})"), span).with_kind(crate::runtime::kind::INDEX));
            }
            Ok(VmValue::Int(b[i as usize] as i64))
        }
        (VmValue::Map(map), _) => {
            let key = map_key(&idx, span)?;
            map.get(&key).cloned().ok_or_else(|| missing_key(&key, span))
        }
        (target, idx) => Err(RuntimeError::new(
            format!("인덱스: {} [{}] 지원 안 됨", target.type_name(), idx.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

fn vm_index_set(container: VmValue, idx: VmValue, val: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match (container, &idx) {
        (VmValue::List(mut items), VmValue::Int(i)) => {
            let len = items.len() as i64;
            let i = if *i < 0 { len + i } else { *i };
            if i < 0 || i >= len {
                return Err(RuntimeError::new(
                    format!("인덱스 범위 초과: {i} (길이 {len})"), span).with_kind(crate::runtime::kind::INDEX));
            }
            Arc::make_mut(&mut items)[i as usize] = val;
            Ok(VmValue::List(items))
        }
        (VmValue::Map(mut map), _) => {
            Arc::make_mut(&mut map).insert(map_key(&idx, span)?, val);
            Ok(VmValue::Map(map))
        }
        (VmValue::Bytes(_), _) => Err(RuntimeError::new(
            "인덱스 대입: Bytes는 불변 — slice와 +로 새 Bytes를 만드세요", span).with_kind(crate::runtime::kind::TYPE)),
        (container, _) => Err(RuntimeError::new(
            format!("인덱스 대입: {} 지원 안 됨", container.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

/// 슬라이스 경계 해석: nil=기본값, 음수=끝에서부터, 범위 밖은 잘라냄 (Python과 같음).
fn slice_bounds(len: usize, start: &VmValue, end: &VmValue, span: Span) -> Result<(usize, usize), RuntimeError> {
    let len = len as i64;
    let norm = |v: &VmValue, default: i64| -> Result<i64, RuntimeError> {
        match v {
            VmValue::Nil => Ok(default),
            VmValue::Int(i) => {
                let i = if *i < 0 { len.saturating_add(*i) } else { *i };
                Ok(i.clamp(0, len))
            }
            other => Err(RuntimeError::new(
                format!("슬라이스 경계는 Int 또는 생략이어야 함, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
        }
    };
    let s = norm(start, 0)?;
    let e = norm(end, len)?;
    Ok((s as usize, e.max(s) as usize))
}

fn vm_slice(target: VmValue, start: &VmValue, end: &VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match target {
        VmValue::List(items) => {
            let (s, e) = slice_bounds(items.len(), start, end, span)?;
            Ok(VmValue::List(Arc::new(items[s..e].to_vec())))
        }
        VmValue::Str(st) => {
            let chars: Vec<char> = st.chars().collect();
            let (s, e) = slice_bounds(chars.len(), start, end, span)?;
            Ok(VmValue::Str(chars[s..e].iter().collect()))
        }
        VmValue::Bytes(b) => {
            let (s, e) = slice_bounds(b.len(), start, end, span)?;
            Ok(VmValue::Bytes(Arc::new(b[s..e].to_vec())))
        }
        other => Err(RuntimeError::new(
            format!("슬라이스: List/Str/Bytes 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

/// `needle in container`: 리스트 원소(==)·맵 키·부분 문자열·바이트 값/부분열.
fn vm_contains(container: &VmValue, needle: &VmValue, span: Span) -> Result<bool, RuntimeError> {
    match (container, needle) {
        (VmValue::List(items), _) => Ok(items.iter().any(|x| vm_eq(x, needle))),
        (VmValue::Map(m), VmValue::Str(_) | VmValue::Int(_)) => Ok(m.contains_key(&map_key(needle, span)?)),
        (VmValue::Str(s), VmValue::Str(sub)) => Ok(s.contains(sub.as_str())),
        (VmValue::Bytes(b), VmValue::Int(x)) => Ok(*x >= 0 && *x <= 255 && b.contains(&(*x as u8))),
        (VmValue::Bytes(b), VmValue::Bytes(sub)) => Ok(sub.is_empty() || find_subseq(b, sub).is_some()),
        (c, n) => Err(RuntimeError::new(
            format!("in: {} 안에서 {} 찾기 지원 안 됨", c.type_name(), n.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

fn arity_error(func: &CompiledFn, got: usize, span: Span) -> RuntimeError {
    let need = if func.min_arity == func.arity {
        format!("{}개", func.arity)
    } else {
        format!("{}~{}개", func.min_arity, func.arity)
    };
    RuntimeError::new(format!("인자 개수 불일치: {need} 기대, {got}개 전달"), span).with_kind(crate::runtime::kind::ARGUMENT)
}

// ── 맵 키 (Phase 57) ─────────────────────────────────────────────────────────
// 맵 키는 str 또는 int다 (1과 "1"은 다른 키). 저장소(OrderedMap<String, _>)는 문자열 키라서 int 키는
// 표식 문자 뒤에 숫자를 붙여 저장하고, 키를 꺼낼 때(keys·for·출력·JSON) 원래 타입으로 되돌린다.
// 표식으로 시작하는 문자열 키는 따로 이스케이프해 int 키와 절대 섞이지 않는다.
const KEY_TAG: char = '\u{1}';

/// 값 → 저장용 키. str/int만 허용 (이전: 무엇이든 조용히 문자열로 바꿨다 — 암묵 변환 금지 위반).
pub(crate) fn map_key(v: &VmValue, span: Span) -> Result<String, RuntimeError> {
    match v {
        VmValue::Str(s) => Ok(encode_str_key(s)),
        VmValue::Int(n) => Ok(format!("{KEY_TAG}i{n}")),
        other => Err(RuntimeError::new(
            format!("맵 키는 str 또는 int여야 함, {} 발견", other.type_name()), span)
            .with_kind(crate::runtime::kind::TYPE)),
    }
}

/// 문자열 키 → 저장용 키 (대부분 그대로).
pub(crate) fn encode_str_key(s: &str) -> String {
    if s.starts_with(KEY_TAG) { format!("{KEY_TAG}s{s}") } else { s.to_string() }
}

/// 저장용 키 → 원래 값 (Int 또는 Str).
pub(crate) fn decode_key(k: &str) -> VmValue {
    if let Some(rest) = k.strip_prefix(KEY_TAG) {
        if let Some(n) = rest.strip_prefix('i').and_then(|d| d.parse::<i64>().ok()) {
            return VmValue::Int(n);
        }
        if let Some(s) = rest.strip_prefix('s') {
            return VmValue::Str(s.to_string());
        }
    }
    VmValue::Str(k.to_string())
}

/// 저장용 키 → 표시 문자열 (출력·JSON 키·에러 메시지).
pub(crate) fn key_display(k: &str) -> String {
    decode_key(k).to_string()
}

/// float 표시 (Phase 57): 정수 값이어도 소수점을 붙여 int와 구별한다 (4.0 → "4.0", Python과 같음).
pub(crate) fn fmt_float(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e16 {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

/// 없는 맵 키 읽기 (Phase 48: 이전에는 조용히 nil).
fn missing_key(key: &str, span: Span) -> RuntimeError {
    let key = key_display(key);
    RuntimeError::new(
        format!("맵에 키 '{key}' 없음 (없을 수 있으면 get(m, k, 기본값) 또는 has(m, k))"),
        span,
    )
    .with_kind(crate::runtime::kind::KEY)
}

/// 내장 런타임 에러 → `catch e`에 바인딩되는 맵 `{kind, message, line, col}` (Phase 48).
pub(crate) fn error_to_value(e: &RuntimeError) -> VmValue {
    let mut m = crate::omap::OrderedMap::new();
    m.insert("kind".to_string(), VmValue::Str(e.kind.to_string()));
    m.insert("message".to_string(), VmValue::Str(e.message.clone()));
    m.insert("line".to_string(), VmValue::Int(e.span.line as i64));
    m.insert("col".to_string(), VmValue::Int(e.span.col as i64));
    VmValue::Map(Arc::new(m))
}

/// `throw e`로 다시 던진 에러 맵이 끝내 잡히지 않으면, 원래 에러(종류·메시지·위치)로 복원한다.
fn rethrown_error(v: &VmValue) -> Option<RuntimeError> {
    let VmValue::Map(m) = v else { return None };
    let (Some(VmValue::Str(k)), Some(VmValue::Str(msg)), Some(VmValue::Int(line)), Some(VmValue::Int(col))) =
        (m.get("kind"), m.get("message"), m.get("line"), m.get("col"))
    else {
        return None;
    };
    if m.len() != 4 {
        return None;
    }
    let kind = crate::runtime::kind::ALL.iter().copied().find(|x| *x == k.as_str())?;
    Some(RuntimeError::new(msg.clone(), Span::new(*line as usize, *col as usize)).with_kind(kind))
}

fn lock_err(span: Span) -> RuntimeError {
    RuntimeError::new("내부 오류: 변수 잠금 실패", span)
}

fn bad_slot(span: Span) -> RuntimeError {
    RuntimeError::new("내부 오류: 잘못된 변수 슬롯", span)
}

/// 두 값이 같은 컨테이너(Arc)를 가리키는가 — 제자리 변경 가능 판정용.
fn same_container(a: &VmValue, b: &VmValue) -> bool {
    match (a, b) {
        (VmValue::List(x), VmValue::List(y)) => Arc::ptr_eq(x, y),
        (VmValue::Map(x), VmValue::Map(y)) => Arc::ptr_eq(x, y),
        (VmValue::Future(x), VmValue::Future(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}

/// target[k1][k2]...[kN] = val 을 제자리에서 수행한다 (Arc::make_mut — 공유 중일 때만 복사).
/// 실패는 마지막 대입 전에 검출되므로 관찰 가능한 부분 변경이 없다.
fn set_path(target: &mut VmValue, keys: &[VmValue], val: VmValue, binop: u8, span: Span) -> Result<VmValue, RuntimeError> {
    let Some((last, init)) = keys.split_last() else {
        return Err(RuntimeError::new("내부 오류: 빈 대입 경로", span));
    };
    let mut cur = target;
    for key in init {
        if let VmValue::Future(_) = cur {
            *cur = auto_resolve(cur.clone(), span)?;
        }
        cur = child_mut(cur, key, span)?;
    }
    if let VmValue::Future(_) = cur {
        *cur = auto_resolve(cur.clone(), span)?;
    }
    // 복합 대입(v[k] op= val): 기존 값과 연산한 결과를 저장한다. 기존 값이 없으면 에러.
    let combine = |old: &VmValue, val: VmValue| -> Result<VmValue, RuntimeError> {
        let old = auto_resolve(old.clone(), span)?;
        let val = auto_resolve(val, span)?;
        match binop {
            OP_ADD => vm_add(old, val, span),
            OP_SUB => vm_arith(old, val, span, '-'),
            OP_MUL => vm_arith(old, val, span, '*'),
            OP_DIV => vm_arith(old, val, span, '/'),
            _ => vm_arith(old, val, span, '%'),
        }
    };
    match (cur, last) {
        (VmValue::List(items), VmValue::Int(i)) => {
            let i = list_pos(items.len(), *i, span)?;
            let new = if binop == 0 { val } else { combine(&items[i], val)? };
            Arc::make_mut(items)[i] = new.clone();
            Ok(new)
        }
        (VmValue::Map(map), key) => {
            let k = map_key(key, span)?;
            let new = if binop == 0 {
                val
            } else {
                match map.get(&k) {
                    Some(old) => combine(old, val)?,
                    None => return Err(RuntimeError::new(
                        format!("복합 대입: 맵에 키 '{}' 없음 (get(m, k, 기본값) 사용)", key_display(&k)), span).with_kind(crate::runtime::kind::KEY)),
                }
            };
            Arc::make_mut(map).insert(k, new.clone());
            Ok(new)
        }
        (VmValue::Bytes(_), _) => Err(RuntimeError::new(
            "인덱스 대입: Bytes는 불변 — slice와 +로 새 Bytes를 만드세요", span).with_kind(crate::runtime::kind::TYPE)),
        (VmValue::List(_), key) => Err(RuntimeError::new(
            format!("인덱스 대입: List 인덱스는 Int여야 함, {} 발견", key.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
        (container, _) => Err(RuntimeError::new(
            format!("인덱스 대입: {} 지원 안 됨", container.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

fn list_pos(len: usize, i: i64, span: Span) -> Result<usize, RuntimeError> {
    let len = len as i64;
    let j = if i < 0 { len + i } else { i };
    if j < 0 || j >= len {
        return Err(RuntimeError::new(format!("인덱스 범위 초과: {j} (길이 {len})"), span).with_kind(crate::runtime::kind::INDEX));
    }
    Ok(j as usize)
}

/// 경로 중간 단계: 컨테이너의 자식에 대한 가변 참조 (없는 맵 키는 에러).
fn child_mut<'a>(cur: &'a mut VmValue, key: &VmValue, span: Span) -> Result<&'a mut VmValue, RuntimeError> {
    match cur {
        VmValue::List(items) => match key {
            VmValue::Int(i) => {
                let i = list_pos(items.len(), *i, span)?;
                Ok(&mut Arc::make_mut(items)[i])
            }
            other => Err(RuntimeError::new(
                format!("인덱스 대입: List 인덱스는 Int여야 함, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
        },
        VmValue::Map(map) => {
            let k = map_key(key, span)?;
            if !map.contains_key(&k) {
                return Err(RuntimeError::new(format!("인덱스 대입: 맵에 키 '{}' 없음", key_display(&k)), span).with_kind(crate::runtime::kind::KEY));
            }
            Arc::make_mut(map).get_mut(&k).ok_or_else(|| bad_slot(span))
        }
        other => Err(RuntimeError::new(
            format!("인덱스 대입: {} 안쪽은 대입할 수 없음", other.type_name()), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

fn vm_field_get(target: VmValue, name: &str, span: Span) -> Result<VmValue, RuntimeError> {
    match target {
        VmValue::Map(map) => {
            let key = encode_str_key(name);
            map.get(&key).cloned().ok_or_else(|| missing_key(&key, span))
        }
        other => Err(RuntimeError::new(
            format!("필드 접근: {} 에 필드 '{}' 없음", other.type_name(), name), span).with_kind(crate::runtime::kind::TYPE)),
    }
}

pub(crate) fn num_of(name: &str, v: &VmValue, span: Span) -> Result<f64, RuntimeError> {
    match v {
        VmValue::Int(n) => Ok(*n as f64),
        VmValue::Float(n) => Ok(*n),
        other => Err(RuntimeError::new(
            format!("{name}(): 숫자 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

pub(crate) fn int_of(name: &str, v: &VmValue, span: Span) -> Result<i64, RuntimeError> {
    match v {
        VmValue::Int(n) => Ok(*n),
        other => Err(RuntimeError::new(
            format!("{name}(): 정수 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

pub(crate) fn str_arg(name: &str, v: &VmValue, span: Span) -> Result<String, RuntimeError> {
    match v {
        VmValue::Str(s) => Ok(s.clone()),
        other => Err(RuntimeError::new(
            format!("{name}(): 문자열 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

pub(crate) fn bytes_arg<'a>(name: &str, v: &'a VmValue, span: Span) -> Result<&'a [u8], RuntimeError> {
    match v {
        VmValue::Bytes(b) => Ok(b.as_slice()),
        other => Err(RuntimeError::new(
            format!("{name}(): Bytes 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

/// 쓰기 계열 빌트인용: Str(UTF-8 그대로) 또는 Bytes(무손실)를 받는다.
pub(crate) fn data_arg<'a>(name: &str, v: &'a VmValue, span: Span) -> Result<&'a [u8], RuntimeError> {
    match v {
        VmValue::Str(s) => Ok(s.as_bytes()),
        VmValue::Bytes(b) => Ok(b.as_slice()),
        other => Err(RuntimeError::new(
            format!("{name}(): 문자열 또는 Bytes 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

pub(crate) fn list_arg(name: &str, v: &VmValue, span: Span) -> Result<Vec<VmValue>, RuntimeError> {
    match v {
        VmValue::List(items) => Ok((**items).clone()),
        other => Err(RuntimeError::new(
            format!("{name}(): 리스트 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

pub(crate) fn as_float(v: &VmValue, span: Span) -> Result<f64, RuntimeError> {
    match v {
        VmValue::Int(n)   => Ok(*n as f64),
        VmValue::Float(n) => Ok(*n),
        other => Err(RuntimeError::new(
            format!("부동소수점 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

pub(crate) fn req_args(name: &str, args: &[VmValue], n: usize, span: Span) -> Result<(), RuntimeError> {
    if args.len() != n {
        Err(RuntimeError::new(
            format!("{name}(): {n}개 인자 필요, {}개 전달", args.len()), span).with_kind(crate::runtime::kind::ARGUMENT))
    } else {
        Ok(())
    }
}

pub(crate) fn as_int(v: &VmValue, span: Span) -> Result<i64, RuntimeError> {
    match v {
        VmValue::Int(n)   => Ok(*n),
        VmValue::Float(n) => Ok(*n as i64),
        other => Err(RuntimeError::new(
            format!("정수 필요, {} 발견", other.type_name()), span).with_kind(crate::runtime::kind::ARGUMENT)),
    }
}

/// Future 자동 조인 (얕게): 최상위 Future 하나만 resolve.
fn auto_resolve(v: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    match v {
        // 작업 안에서 난 에러는 원래 위치(작업 쪽 줄·파일)를 유지한다 — 실제로 잘못된 곳을 가리키도록 (Phase 56).
        // 위치가 없는 에러만 값을 쓴 지점으로 채운다.
        VmValue::Future(f) => f.resolve().map_err(|mut e| {
            if e.span.line == 0 {
                e.span = span;
            }
            e
        }),
        other => Ok(other),
    }
}

/// Future 깊이 해소 (print용): 컨테이너 안의 Future도 재귀 처리.
pub(crate) fn deep_resolve(v: VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    let v = auto_resolve(v, span)?;
    match v {
        VmValue::List(items) => {
            let mut resolved = Vec::with_capacity(items.len());
            for item in items.iter() { resolved.push(deep_resolve(item.clone(), span)?); }
            Ok(VmValue::List(Arc::new(resolved)))
        }
        VmValue::Map(map) => {
            let mut resolved = crate::omap::OrderedMap::new();
            for (k, val) in map.iter() { resolved.insert(k.clone(), deep_resolve(val.clone(), span)?); }
            Ok(VmValue::Map(Arc::new(resolved)))
        }
        other => Ok(other),
    }
}

/// import 모듈 캐시 — 같은 파일은 한 번만 실행되고 결과(export 맵)를 공유한다.
/// (모듈을 싱글톤으로 만드는 패키지 시스템 v1의 핵심.)
pub(crate) fn module_cache() -> &'static Mutex<HashMap<String, VmValue>> {
    static CACHE: OnceLock<Mutex<HashMap<String, VmValue>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 현재 로딩 중인 모듈 집합 — 순환 import(a→b→a)를 무한 재귀(스택 오버플로) 대신
/// 잡을 수 있는 런타임 에러로 감지한다.
pub(crate) fn module_loading() -> &'static Mutex<std::collections::HashSet<String>> {
    static LOADING: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    LOADING.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// import 모듈 해석. 바레 이름은 검색 경로에서 찾고, 경로/확장자가 있으면 직접 경로.
pub(crate) fn resolve_module(spec: &str) -> Option<std::path::PathBuf> {
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
        TypeAnn::Bytes => matches!(v, VmValue::Bytes(_)),
    }
}

/// 작업 경계 격리기 (Phase 53).
///
/// spawn·parallel_map·`bang test`가 새 작업으로 값을 넘길 때, 그 값에서 닿는 **모든 클로저**와
/// 클로저가 붙잡은 상태(지역 변수 벡터·모듈 전역)를 새로 복사한다. 이전(deep_clone_closure)에는
/// 호출 대상 클로저만 복사해서, 인자나 맵 안에 든 "객체"(맵 + 클로저)의 상태가 스레드 사이에
/// 공유되어 경쟁 상태가 생겼다.
///
/// - 같은 격리 안에서 상태를 공유하던 클로저들은 복사본에서도 서로 공유한다 (객체의 메서드끼리).
/// - 클로저가 들어 있지 않은 리스트·맵은 복사하지 않고 같은 Arc를 쓴다 (불변 공유 — COW로 안전).
/// - 채널·Future·TCP 핸들은 원래 공유 대상이라 그대로 둔다.
pub(crate) struct Isolator {
    closures: HashMap<usize, Arc<VmClosure>>,
    futures: HashMap<usize, Arc<VmFuture>>,
    cells: HashMap<usize, StateCell>,
    /// (원본, 복사본) — 아직 내용을 채우지 않은 상태 벡터
    pending: Vec<(StateCell, StateCell)>,
}

/// 클로저가 붙잡는 상태 벡터 (프레임 지역 변수들 또는 모듈 전역).
pub(crate) type StateCell = Arc<Mutex<Vec<VmValue>>>;

impl Isolator {
    pub(crate) fn new() -> Self {
        Self { closures: HashMap::new(), futures: HashMap::new(), cells: HashMap::new(), pending: Vec::new() }
    }

    /// 상태 벡터(지역 변수들 또는 모듈 전역)의 복사본 자리를 만든다. 내용은 finish()에서 채운다.
    fn cell(&mut self, old: &Arc<Mutex<Vec<VmValue>>>) -> Arc<Mutex<Vec<VmValue>>> {
        let key = Arc::as_ptr(old) as usize;
        if let Some(n) = self.cells.get(&key) {
            return n.clone();
        }
        let n = Arc::new(Mutex::new(Vec::new()));
        self.cells.insert(key, n.clone());
        self.pending.push((old.clone(), n.clone()));
        n
    }

    pub(crate) fn closure(&mut self, c: &Arc<VmClosure>) -> Arc<VmClosure> {
        let key = Arc::as_ptr(c) as usize;
        if let Some(n) = self.closures.get(&key) {
            return n.clone();
        }
        let upvalues = c.upvalues.iter()
            .map(|uv| Arc::new(Upvalue { locals: self.cell(&uv.locals), slot: uv.slot }))
            .collect();
        let globals = self.cell(&c.globals);
        let n = Arc::new(VmClosure { func: c.func.clone(), upvalues, globals });
        self.closures.insert(key, n.clone());
        n
    }

    /// 값 안의 클로저를 격리한 새 값. 바뀔 것이 없으면 None (원래 값을 그대로 쓰면 됨).
    fn value(&mut self, v: &VmValue) -> Option<VmValue> {
        match v {
            VmValue::Closure(c) => Some(VmValue::Closure(self.closure(c))),
            VmValue::Future(f) => {
                // 이미 끝났고 결과에 클로저가 없으면 결코 바뀌지 않으므로 그대로 공유 (재포장·재검사 없음).
                // 검사는 복사 없는 순수 확인이어야 한다 — 이전에는 검사하면서 결과 객체를 통째로 복사해
                // (어차피 derived Future가 꺼낼 때 다시 격리하므로 버려지는 일) 전역에 객체 Future를 모으며
                // spawn하면 O(n²) 복사가 됐다 (300회 1.1초 → Phase 60에서 수정).
                if let Some(Ok(v)) = f.peek() {
                    if !contains_closure(&v) {
                        return None;
                    }
                }
                let key = Arc::as_ptr(f) as usize;
                let d = self.futures.entry(key).or_insert_with(|| VmFuture::derived(f)).clone();
                Some(VmValue::Future(d))
            }
            VmValue::List(items) => {
                if free_cache_hit(&free_cache().lists, items) {
                    return None;
                }
                let mut changed: Option<Vec<VmValue>> = None;
                for (i, x) in items.iter().enumerate() {
                    if let Some(nx) = self.value(x) {
                        changed.get_or_insert_with(|| items.to_vec())[i] = nx;
                    }
                }
                match changed {
                    Some(v) => Some(VmValue::List(Arc::new(v))),
                    None => {
                        free_cache_remember(&free_cache().lists, items);
                        None
                    }
                }
            }
            VmValue::Map(m) => {
                if free_cache_hit(&free_cache().maps, m) {
                    return None;
                }
                let mut changed: Option<crate::omap::OrderedMap<VmValue>> = None;
                for (k, x) in m.iter() {
                    if let Some(nx) = self.value(x) {
                        let nm = changed.get_or_insert_with(|| (**m).clone());
                        if let Some(slot) = nm.get_mut(k) {
                            *slot = nx;
                        }
                    }
                }
                match changed {
                    Some(m) => Some(VmValue::Map(Arc::new(m))),
                    None => {
                        free_cache_remember(&free_cache().maps, m);
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// 이 격리기가 새로 만든 상태 벡터들 (작업이 끝난 뒤 순환 수집 후보 — Phase 60).
    pub(crate) fn new_cells(&self) -> Vec<StateCell> {
        self.cells.values().cloned().collect()
    }

    pub(crate) fn isolate(&mut self, v: VmValue) -> VmValue {
        self.value(&v).unwrap_or(v)
    }

    /// 자리만 만들어 둔 상태 벡터들을 채운다 (채우는 중 새로 발견된 것도 반복 처리 — 순환 안전).
    /// 이 격리기로 isolate/closure를 모두 부른 뒤 마지막에 한 번 호출한다.
    pub(crate) fn finish(&mut self) {
        while let Some((old, new)) = self.pending.pop() {
            let snapshot = old.lock().map(|g| g.clone()).unwrap_or_default();
            let filled: Vec<VmValue> = snapshot.iter()
                .map(|x| self.value(x).unwrap_or_else(|| x.clone()))
                .collect();
            if let Ok(mut g) = new.lock() {
                *g = filled;
            }
        }
    }
}

/// "클로저가 없다"고 확인된 큰 컨테이너 캐시 — spawn마다 같은 큰 리스트를 다시 훑지 않게 한다.
///
/// 약한 참조(Weak)만 보관한다. 약한 참조가 있으면 `Arc::make_mut`은 제자리 변경 대신 새 할당으로
/// 옮기므로(그리고 `Arc::get_mut`은 실패하므로), 내용이 바뀐 컨테이너는 다른 포인터가 되어
/// 캐시에 절대 맞지 않는다 — 낡은 판정이 쓰일 수 없다. 약한 참조가 할당을 붙잡고 있어 주소 재사용도 없다.
struct FreeCache {
    lists: Mutex<HashMap<usize, std::sync::Weak<Vec<VmValue>>>>,
    maps: Mutex<HashMap<usize, std::sync::Weak<crate::omap::OrderedMap<VmValue>>>>,
}

fn free_cache() -> &'static FreeCache {
    static CACHE: OnceLock<FreeCache> = OnceLock::new();
    CACHE.get_or_init(|| FreeCache { lists: Mutex::new(HashMap::new()), maps: Mutex::new(HashMap::new()) })
}

/// 이 크기 미만은 훑는 비용이 캐시 비용보다 싸서 캐시하지 않는다.
const FREE_CACHE_MIN_LEN: usize = 64;
const FREE_CACHE_CAP: usize = 4096;

trait HasLen {
    fn size(&self) -> usize;
}
impl HasLen for Vec<VmValue> {
    fn size(&self) -> usize { self.len() }
}
impl HasLen for crate::omap::OrderedMap<VmValue> {
    fn size(&self) -> usize { self.len() }
}

/// 값 안에 클로저가 있는가 — 복사 없는 순수 검사 (큰 컨테이너는 "클로저 없음" 캐시 사용).
pub(crate) fn contains_closure(v: &VmValue) -> bool {
    match v {
        VmValue::Closure(_) => true,
        VmValue::List(l) => !known_closure_free_list(l) && l.iter().any(contains_closure),
        VmValue::Map(m) => !known_closure_free_map(m) && m.values().any(contains_closure),
        _ => false,
    }
}

/// 큰 리스트/맵이 "클로저 없음"으로 이미 확인됐는가 (순환 수집기가 훑기를 건너뛰는 데 쓴다).
pub(crate) fn known_closure_free_list(a: &Arc<Vec<VmValue>>) -> bool {
    free_cache_hit(&free_cache().lists, a)
}
pub(crate) fn known_closure_free_map(a: &Arc<crate::omap::OrderedMap<VmValue>>) -> bool {
    free_cache_hit(&free_cache().maps, a)
}

fn free_cache_hit<T: HasLen>(cache: &Mutex<HashMap<usize, std::sync::Weak<T>>>, a: &Arc<T>) -> bool {
    if a.size() < FREE_CACHE_MIN_LEN {
        return false;
    }
    let Ok(g) = cache.lock() else { return false };
    g.get(&(Arc::as_ptr(a) as usize))
        .and_then(|w| w.upgrade())
        .is_some_and(|b| Arc::ptr_eq(&b, a))
}

fn free_cache_remember<T: HasLen>(cache: &Mutex<HashMap<usize, std::sync::Weak<T>>>, a: &Arc<T>) {
    if a.size() < FREE_CACHE_MIN_LEN {
        return;
    }
    let Ok(mut g) = cache.lock() else { return };
    if g.len() >= FREE_CACHE_CAP {
        g.retain(|_, w| w.strong_count() > 0);
        if g.len() >= FREE_CACHE_CAP {
            g.clear();
        }
    }
    g.insert(Arc::as_ptr(a) as usize, Arc::downgrade(a));
}

/// 클로저 하나를 새 작업용으로 격리 (Isolator 한 번 쓰기). 새로 만든 상태 벡터도 돌려준다 —
/// 작업이 끝나면 crate::cycles::collect에 넘겨 순환을 정리한다.
pub(crate) fn isolate_closure(c: &Arc<VmClosure>) -> (Arc<VmClosure>, Vec<StateCell>) {
    let mut iso = Isolator::new();
    let n = iso.closure(c);
    iso.finish();
    (n, iso.new_cells())
}

/// 바이트 슬라이스에서 부분열의 시작 위치를 찾는다 (HTTP 헤더 끝 감지용).
pub(crate) fn find_subseq(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() { return None; }
    hay.windows(needle.len()).position(|w| w == needle)
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
fn warn_if_spawn_err(result: Result<(), RuntimeError>) {
    if let Err(e) = result {
        eprintln!("경고: spawn된 작업에서 처리되지 않은 에러: {e}");
    }
}

/// 채널 송신용 변환. 데이터 값(스칼라·List·Map)만 허용한다.
/// 함수/채널/Future/Tcp 등 참조 타입은 컨테이너 안에 있어도 에러 (조용히 Nil로 떨구지 않음).
pub(crate) fn to_runtime(v: &VmValue, span: Span) -> Result<crate::runtime::Value, RuntimeError> {
    use crate::runtime::Value as RV;
    Ok(match v {
        VmValue::Int(n)   => RV::Int(*n),
        VmValue::Float(n) => RV::Float(*n),
        VmValue::Bool(b)  => RV::Bool(*b),
        VmValue::Str(s)   => RV::Str(s.clone()),
        VmValue::Nil      => RV::Nil,
        VmValue::Bytes(b) => RV::Bytes(b.clone()),
        VmValue::List(items) => RV::List(
            items.iter().map(|x| to_runtime(x, span)).collect::<Result<_, _>>()?),
        VmValue::Map(m) => RV::Map(
            m.iter().map(|(k, x)| Ok((k.clone(), to_runtime(x, span)?)))
                .collect::<Result<_, RuntimeError>>()?),
        other => return Err(RuntimeError::new(
            format!("send(): {}은(는) 채널로 보낼 수 없음 \
                     (데이터 값만 전송 가능: int/float/bool/str/bytes/nil/list/map)", other.type_name()),
            span).with_kind(crate::runtime::kind::VALUE)),
    })
}

pub(crate) fn from_runtime(v: crate::runtime::Value) -> VmValue {
    use crate::runtime::Value as RV;
    match v {
        RV::Int(n)   => VmValue::Int(n),
        RV::Float(n) => VmValue::Float(n),
        RV::Bool(b)  => VmValue::Bool(b),
        RV::Str(s)   => VmValue::Str(s),
        RV::Bytes(b) => VmValue::Bytes(b),
        RV::Nil      => VmValue::Nil,
        RV::List(items) => VmValue::List(Arc::new(items.into_iter().map(from_runtime).collect())),
        RV::Map(m) => VmValue::Map(Arc::new(m.into_iter().map(|(k, v)| (k, from_runtime(v))).collect())),
    }
}
