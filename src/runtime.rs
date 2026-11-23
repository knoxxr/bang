// Bang — 런타임 공용 타입: RuntimeError, 채널 전송용 Value(데이터 전용), BangChannel

use std::fmt;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::lexer::token::Span;

// ============================================================================
// RuntimeError
// ============================================================================

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub span: Span,
    /// 에러 종류 (Phase 48). `catch e`에서 `e.kind`로 보인다. 목록은 [`kind`] 모듈 (SPEC 8.2).
    pub kind: &'static str,
    /// 에러가 난 소스 파일 (Phase 56) — import한 모듈 안의 에러면 그 모듈 경로. None이면 실행 중인 메인 파일.
    pub file: Option<std::sync::Arc<str>>,
    /// 잡히지 않은 사용자 `throw`의 원래 값 (Phase 62). 작업(spawn·parallel_map)이나 내장 함수의 콜백 경계를
    /// 건너 바깥 `catch`에 도달하면 에러 맵 대신 이 값이 그대로 바인딩된다 ("throw한 값은 그대로", SPEC 8.1).
    pub thrown: Option<Box<crate::vm::VmValue>>,
}

/// `catch e`의 `e.kind` 값 (SPEC 8.2). 대외 계약이므로 이름을 바꾸지 않는다.
pub mod kind {
    /// 0으로 나누기 / 나머지
    pub const ZERO_DIVISION: &str = "zero_division";
    /// 정수 오버플로
    pub const OVERFLOW: &str = "overflow";
    /// 리스트/문자열/바이트 인덱스 범위 초과
    pub const INDEX: &str = "index";
    /// 맵에 없는 키 읽기/갱신
    pub const KEY: &str = "key";
    /// 연산·호출·접근 대상의 타입이 맞지 않음 (타입 힌트 위반 포함)
    pub const TYPE: &str = "type";
    /// 빌트인/함수 인자 개수·타입 잘못
    pub const ARGUMENT: &str = "argument";
    /// 형식은 맞지만 값이 잘못됨 (JSON/정규식 파싱, 분해 길이, 변환 실패 등)
    pub const VALUE: &str = "value";
    /// 파일·네트워크·프로세스
    pub const IO: &str = "io";
    /// assert / assert_eq 실패
    pub const ASSERTION: &str = "assertion";
    /// import 실패
    pub const IMPORT: &str = "import";
    /// 그 밖의 런타임 에러
    pub const ERROR: &str = "error";

    /// 전체 목록 (다시 던진 에러 맵 복원용).
    pub const ALL: &[&str] = &[
        ZERO_DIVISION, OVERFLOW, INDEX, KEY, TYPE, ARGUMENT, VALUE, IO, ASSERTION, IMPORT, ERROR,
    ];
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>, span: Span) -> Self {
        Self { message: msg.into(), span, kind: kind::ERROR, file: None, thrown: None }
    }
    pub fn no_span(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), span: Span::new(0, 0), kind: kind::ERROR, file: None, thrown: None }
    }
    /// 종류 지정 (빌더).
    pub fn with_kind(mut self, k: &'static str) -> Self {
        self.kind = k;
        self
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.span.line) {
            (None, 0) => write!(f, "런타임 오류: {}", self.message),
            (None, _) => write!(f, "[{}:{}] 런타임 오류: {}", self.span.line, self.span.col, self.message),
            (Some(file), 0) => write!(f, "[{file}] 런타임 오류: {}", self.message),
            (Some(file), _) => write!(f, "[{file} {}:{}] 런타임 오류: {}", self.span.line, self.span.col, self.message),
        }
    }
}

impl std::error::Error for RuntimeError {}

// ============================================================================
// Value
// ============================================================================

/// 채널로 전달되는 값 — **데이터 전용**. 함수·채널·Future·TCP 핸들은 표현할 수 없으므로
/// "채널로는 데이터만 보낸다"(SPEC 6.4)가 타입으로 보장된다. 모든 변형이 소유 데이터라
/// 자동으로 Send + Sync다 (unsafe impl 불필요).
#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    List(Vec<Value>),
    Map(crate::omap::OrderedMap<Value>),
    /// 불변 바이트 열 (값 의미론 — 불변이라 Arc 공유가 관찰상 깊은 복사와 동일)
    Bytes(Arc<Vec<u8>>),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Str(_) => "Str",
            Value::Nil => "Nil",
            Value::List(_) => "List",
            Value::Map(_) => "Map",
            Value::Bytes(_) => "Bytes",
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Bool(false) | Value::Nil)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{}", crate::vm::fmt_float(*n)),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Nil => write!(f, "nil"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {v}", crate::vm::key_display(k))?;
                }
                write!(f, "}}")
            }
            Value::Bytes(b) => fmt_bytes(f, b),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

// ============================================================================
// BangChannel — 스레드 간 메시지 전달 (mpsc 기반)
// ============================================================================

pub struct BangChannel {
    sender: Mutex<Option<Sender<Value>>>,
    receiver: Mutex<Receiver<Value>>,
}

impl std::fmt::Debug for BangChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<channel>")
    }
}

/// 논블로킹 수신 결과.
pub enum TryRecv {
    Value(Value),
    Empty,  // 아직 값 없음 (열려 있음)
    Closed, // 닫혔고 버퍼 비어 있음
}

impl BangChannel {
    pub fn new(_capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            sender: Mutex::new(Some(tx)),
            receiver: Mutex::new(rx),
        }
    }

    pub fn send(&self, value: Value) -> Result<(), RuntimeError> {
        let guard = self.sender.lock().unwrap();
        match &*guard {
            Some(tx) => tx.send(value).map_err(|_| RuntimeError::no_span("채널 송신 실패: 수신단 없음")),
            None => Err(RuntimeError::no_span("채널이 닫혔습니다")),
        }
    }

    /// 블록되며 값을 수신. 채널이 닫히면 None.
    pub fn recv(&self) -> Option<Value> {
        // Mutex 를 취득한 채로 recv() 블록 → 단일 소비자면 문제 없음
        self.receiver.lock().unwrap().recv().ok()
    }

    /// 논블로킹 수신 (select 용).
    pub fn try_recv(&self) -> TryRecv {
        match self.receiver.lock().unwrap().try_recv() {
            Ok(v) => TryRecv::Value(v),
            Err(mpsc::TryRecvError::Empty) => TryRecv::Empty,
            Err(mpsc::TryRecvError::Disconnected) => TryRecv::Closed,
        }
    }

    pub fn close(&self) {
        let mut guard = self.sender.lock().unwrap();
        *guard = None; // Sender 드롭 → 채널 닫힘 → recv() 반환
    }
}

/// 바이트 열 표시 형식: `<bytes N: 16진수>` — 앞 32바이트까지만 보이고 나머지는 `…`.
/// (리터럴 문법이 없으므로 표시는 디버깅용이며, 다시 파싱되는 형식이 아니다.)
pub fn fmt_bytes(f: &mut fmt::Formatter<'_>, b: &[u8]) -> fmt::Result {
    const SHOW: usize = 32;
    write!(f, "<bytes {}: ", b.len())?;
    for x in b.iter().take(SHOW) {
        write!(f, "{x:02x}")?;
    }
    if b.len() > SHOW {
        write!(f, "…")?;
    }
    write!(f, ">")
}
