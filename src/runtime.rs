// Bang — 런타임 타입: Value, Env, BangChannel, BangFuture

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::ast::Block;
use crate::lexer::token::Span;

// ============================================================================
// RuntimeError
// ============================================================================

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub span: Span,
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>, span: Span) -> Self {
        Self { message: msg.into(), span }
    }
    pub fn no_span(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), span: Span::new(0, 0) }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.span.line == 0 {
            write!(f, "런타임 오류: {}", self.message)
        } else {
            write!(f, "[{}:{}] 런타임 오류: {}", self.span.line, self.span.col, self.message)
        }
    }
}

impl std::error::Error for RuntimeError {}

// ============================================================================
// Value
// ============================================================================

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Function(Arc<BangFunction>),
    Channel(Arc<BangChannel>),
    Future(Arc<BangFuture>),
    // 내장 함수: 이름 태그만 저장, 실제 디스패치는 Interpreter::call_builtin
    Builtin(&'static str),
}

// Safety: 모든 공유 가변 상태는 Arc<Mutex<_>>로 보호.
// List/Map/Str 은 스레드 경계를 넘기 전 복제(Value::clone = 깊은 복사).
// Function/Channel/Future 는 Arc 클론(참조 의미론).
unsafe impl Send for Value {}
unsafe impl Sync for Value {}

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
            Value::Function(_) => "Function",
            Value::Channel(_) => "Channel",
            Value::Future(_) => "Future",
            Value::Builtin(n) => n,
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
            Value::Float(n) => write!(f, "{n}"),
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
                let mut pairs: Vec<_> = map.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Function(func) => {
                if let Some(n) = &func.name { write!(f, "<fn {n}>") }
                else { write!(f, "<fn>") }
            }
            Value::Channel(_) => write!(f, "<channel>"),
            Value::Future(_) => write!(f, "<future>"),
            Value::Builtin(n) => write!(f, "<builtin {n}>"),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

// ============================================================================
// BangFunction — 클로저 + 파라미터 + 본문 보유
// ============================================================================

pub struct BangFunction {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Block,
    pub closure: Arc<Mutex<Env>>,
}

// ============================================================================
// Env — 렉시컬 스코프 체인
// ============================================================================

pub struct Env {
    pub vars: HashMap<String, Value>,
    pub parent: Option<Arc<Mutex<Env>>>,
}

impl Default for Env {
    fn default() -> Self { Self::new() }
}

impl Env {
    pub fn new() -> Self {
        Self { vars: HashMap::new(), parent: None }
    }

    pub fn with_parent(parent: Arc<Mutex<Env>>) -> Self {
        Self { vars: HashMap::new(), parent: Some(parent) }
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.vars.insert(name, value);
    }

    /// 현재·부모 스코프를 순서대로 검색, 있으면 클론 반환
    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        if let Some(p) = &self.parent {
            return p.lock().unwrap().get(name);
        }
        None
    }

    /// 스코프 체인에서 이름을 찾아 재대입. 없으면 false.
    pub fn assign(&mut self, name: &str, value: Value) -> bool {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), value);
            return true;
        }
        if let Some(p) = &self.parent {
            return p.lock().unwrap().assign(name, value);
        }
        false
    }

    /// spawn 용 깊은 스냅샷: 데이터(List/Map/Str)는 깊은 복사, 함수/채널/Future 는 Arc 클론
    pub fn snapshot(&self) -> Arc<Mutex<Env>> {
        let mut new_env = Env::new();
        for (k, v) in &self.vars {
            new_env.vars.insert(k.clone(), v.clone());
        }
        if let Some(p) = &self.parent {
            new_env.parent = Some(p.lock().unwrap().snapshot());
        }
        Arc::new(Mutex::new(new_env))
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

    pub fn close(&self) {
        let mut guard = self.sender.lock().unwrap();
        *guard = None; // Sender 드롭 → 채널 닫힘 → recv() 반환
    }
}

// ============================================================================
// BangFuture — spawn 결과 핸들 (Condvar 기반 캐싱)
// ============================================================================

pub struct BangFuture {
    inner: Mutex<FutureState>,
    ready: Condvar,
}

enum FutureState {
    Pending(Receiver<Result<Value, RuntimeError>>),
    Resolving, // 다른 스레드가 대기 중
    Done(Result<Value, RuntimeError>),
}

impl BangFuture {
    pub fn new(rx: Receiver<Result<Value, RuntimeError>>) -> Self {
        Self {
            inner: Mutex::new(FutureState::Pending(rx)),
            ready: Condvar::new(),
        }
    }

    /// 결과가 올 때까지 블록. 캐시된 경우 즉시 반환(cheap).
    pub fn resolve(&self) -> Result<Value, RuntimeError> {
        let mut guard = self.inner.lock().unwrap();
        loop {
            match &*guard {
                FutureState::Done(r) => return r.clone(),
                FutureState::Resolving => {
                    // 다른 스레드가 대기 중 → Condvar 로 대기
                    guard = self.ready.wait(guard).unwrap();
                }
                FutureState::Pending(_) => {
                    // 우리가 수신 담당
                    let rx = match std::mem::replace(&mut *guard, FutureState::Resolving) {
                        FutureState::Pending(rx) => rx,
                        _ => unreachable!(),
                    };
                    drop(guard); // 락 해제 후 블록
                    let result = rx.recv().unwrap_or_else(|_| {
                        Err(RuntimeError::no_span("작업 스레드 패닉"))
                    });
                    let mut guard = self.inner.lock().unwrap();
                    *guard = FutureState::Done(result.clone());
                    self.ready.notify_all();
                    return result;
                }
            }
        }
    }
}

// ============================================================================
// 헬퍼: Future 해소
// ============================================================================

/// 얕게 해소: 최상위 Future 만 한 번 조인
pub fn resolve_shallow(v: Value) -> Result<Value, RuntimeError> {
    match v {
        Value::Future(f) => f.resolve(),
        other => Ok(other),
    }
}

/// 깊게 해소: 컨테이너 내부 Future 까지 재귀 조인 (print 용)
pub fn deep_resolve(v: Value) -> Result<Value, RuntimeError> {
    let v = resolve_shallow(v)?;
    match v {
        Value::List(items) => {
            let items = items.into_iter().map(deep_resolve).collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(items))
        }
        Value::Map(map) => {
            let map = map.into_iter()
                .map(|(k, v)| deep_resolve(v).map(|v| (k, v)))
                .collect::<Result<HashMap<_, _>, _>>()?;
            Ok(Value::Map(map))
        }
        other => Ok(other),
    }
}
