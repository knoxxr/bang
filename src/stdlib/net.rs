//! 빌트인: 네트워킹 (TCP·이벤트 서버·HTTP)

use super::prelude::*;

/// 96: tcp_listen(addr) → TcpListener  (예: "127.0.0.1:8080")
pub(crate) fn b_tcp_listen(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_listen", &args, 1, span)?;
    let addr = str_arg("tcp_listen", &args[0], span)?;
    let listener = std::net::TcpListener::bind(&addr)
        .map_err(|e| RuntimeError::new(format!("tcp_listen('{addr}'): {e}"), span))?;
    Ok(VmValue::TcpListener(Arc::new(listener)))
}

/// 97: tcp_accept(server) → TcpConn  (블로킹)
pub(crate) fn b_tcp_accept(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_accept", &args, 1, span)?;
    match &args[0] {
        VmValue::TcpListener(l) => {
            let (stream, _peer) = l
                .accept()
                .map_err(|e| RuntimeError::new(format!("tcp_accept(): {e}"), span))?;
            Ok(VmValue::TcpConn(Arc::new(Mutex::new(stream))))
        }
        other => Err(RuntimeError::new(
            format!("tcp_accept: TcpListener 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 98: tcp_read(conn) → str  (최대 4096바이트 1회 읽기, EOF면 "")
pub(crate) fn b_tcp_read(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
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
            Ok(VmValue::Str(
                String::from_utf8_lossy(&buf[..n]).into_owned(),
            ))
        }
        other => Err(RuntimeError::new(
            format!("tcp_read: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 99: tcp_write(conn, data: Str|Bytes) → nil
pub(crate) fn b_tcp_write(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_write", &args, 2, span)?;
    let data = data_arg("tcp_write", &args[1], span)?;
    match &args[0] {
        VmValue::TcpConn(c) => {
            use std::io::Write;
            let mut s = c.lock().unwrap();
            s.write_all(data)
                .map_err(|e| RuntimeError::new(format!("tcp_write(): {e}"), span))?;
            Ok(VmValue::Nil)
        }
        other => Err(RuntimeError::new(
            format!("tcp_write: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 100: tcp_close(conn) → nil
pub(crate) fn b_tcp_close(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_close", &args, 1, span)?;
    match &args[0] {
        VmValue::TcpConn(c) => {
            let s = c.lock().unwrap();
            let _ = s.shutdown(std::net::Shutdown::Both);
            Ok(VmValue::Nil)
        }
        other => Err(RuntimeError::new(
            format!("tcp_close: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 101: tcp_read_until(conn, marker) → marker 나올 때까지 누적 읽기
pub(crate) fn b_tcp_read_until(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_read_until", &args, 2, span)?;
    let marker = str_arg("tcp_read_until", &args[1], span)?;
    match &args[0] {
        VmValue::TcpConn(c) => {
            use std::io::Read;
            let mut acc = String::new();
            let mut buf = [0u8; 1024];
            loop {
                if !marker.is_empty() && acc.contains(&marker) {
                    break;
                }
                let n = {
                    let mut s = c.lock().unwrap();
                    s.read(&mut buf)
                        .map_err(|e| RuntimeError::new(format!("tcp_read_until(): {e}"), span))?
                };
                if n == 0 {
                    break;
                } // EOF
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            Ok(VmValue::Str(acc))
        }
        other => Err(RuntimeError::new(
            format!("tcp_read_until: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 102: tcp_set_timeout(conn, ms) → 읽기 타임아웃 설정 (0 = 무제한)
pub(crate) fn b_tcp_set_timeout(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_set_timeout", &args, 2, span)?;
    let ms = int_of("tcp_set_timeout", &args[1], span)?;
    match &args[0] {
        VmValue::TcpConn(c) => {
            let dur = if ms <= 0 {
                None
            } else {
                Some(std::time::Duration::from_millis(ms as u64))
            };
            let s = c.lock().unwrap();
            s.set_read_timeout(dur)
                .map_err(|e| RuntimeError::new(format!("tcp_set_timeout(): {e}"), span))?;
            Ok(VmValue::Nil)
        }
        other => Err(RuntimeError::new(
            format!("tcp_set_timeout: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 105: tcp_send_file(conn, path) → Int (보낸 바이트 수). UTF-8 변환 없이 무손실 전송.
pub(crate) fn b_tcp_send_file(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_send_file", &args, 2, span)?;
    let path = str_arg("tcp_send_file", &args[1], span)?;
    let bytes = std::fs::read(&path)
        .map_err(|e| RuntimeError::new(format!("tcp_send_file('{path}'): {e}"), span))?;
    match &args[0] {
        VmValue::TcpConn(c) => {
            use std::io::Write;
            let mut s = c.lock().unwrap();
            s.write_all(&bytes)
                .map_err(|e| RuntimeError::new(format!("tcp_send_file(): {e}"), span))?;
            Ok(VmValue::Int(bytes.len() as i64))
        }
        other => Err(RuntimeError::new(
            format!("tcp_send_file: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 106: serve_event(addr, handler) — 논블로킹 이벤트 루프 서버
pub(crate) fn b_serve_event(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    // handler: fn(req_str) -> resp_str. 단일 스레드가 리액터로 많은 연결을
    // 멀티플렉싱 → 유휴 keep-alive 연결이 스레드를 점유하지 않음(C10K 1차).
    req_args("serve_event", &args, 2, span)?;
    let addr = str_arg("serve_event", &args[0], span)?;
    let handler = match &args[1] {
        VmValue::Closure(c) => c.clone(),
        other => {
            return Err(RuntimeError::new(
                format!("serve_event: 핸들러(함수) 필요, {} 발견", other.type_name()),
                span,
            ))
        }
    };
    vm.run_event_server(&addr, handler, span)
}

/// 107: tcp_connect(addr) → TcpConn  (예: "example.com:80")
pub(crate) fn b_tcp_connect(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_connect", &args, 1, span)?;
    let addr = str_arg("tcp_connect", &args[0], span)?;
    let stream = std::net::TcpStream::connect(&addr)
        .map_err(|e| RuntimeError::new(format!("tcp_connect('{addr}'): {e}"), span))?;
    Ok(VmValue::TcpConn(Arc::new(Mutex::new(stream))))
}

/// 108: http_get(url) → {status, headers, body}
pub(crate) fn b_http_get(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("http_get", &args, 1, span)?;
    let url = str_arg("http_get", &args[0], span)?;
    http_get(&url, span)
}

/// 요청 헤더 목록 (이름, 값) — 순서 유지.
type HeaderList = Vec<(String, String)>;

/// http_post/http_request의 body·headers 인자 해석.
/// body: str/bytes는 그대로, map/list는 JSON으로 직렬화하고 Content-Type: application/json을 기본으로 붙인다.
fn http_body_and_headers(
    fname: &str,
    body: Option<&VmValue>,
    headers: Option<&VmValue>,
    span: Span,
) -> Result<(Option<Vec<u8>>, HeaderList), RuntimeError> {
    let mut hs: Vec<(String, String)> = Vec::new();
    match headers {
        None | Some(VmValue::Nil) => {}
        Some(VmValue::Map(m)) => {
            for (k, v) in m.iter() {
                let k = &key_display(k);
                let v = match v {
                    VmValue::Str(s) => s.clone(),
                    other => other.to_string(),
                };
                hs.push((k.clone(), v));
            }
        }
        Some(other) => {
            return Err(RuntimeError::new(
                format!("{fname}(): headers는 Map이어야 함, {} 발견", other.type_name()),
                span,
            )
            .with_kind(crate::runtime::kind::ARGUMENT))
        }
    }
    let has_ct = hs.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    let bytes = match body {
        None | Some(VmValue::Nil) => None,
        Some(VmValue::Str(s)) => {
            if !has_ct {
                hs.push(("Content-Type".into(), "text/plain; charset=utf-8".into()));
            }
            Some(s.as_bytes().to_vec())
        }
        Some(VmValue::Bytes(b)) => {
            if !has_ct {
                hs.push(("Content-Type".into(), "application/octet-stream".into()));
            }
            Some(b.to_vec())
        }
        Some(v @ (VmValue::Map(_) | VmValue::List(_))) => {
            let mut out = String::new();
            super::json::json_stringify(v, &mut out, span)?;
            if !has_ct {
                hs.push(("Content-Type".into(), "application/json".into()));
            }
            Some(out.into_bytes())
        }
        Some(other) => {
            return Err(RuntimeError::new(
                format!("{fname}(): body는 str/bytes/map/list/nil, {} 발견", other.type_name()),
                span,
            )
            .with_kind(crate::runtime::kind::ARGUMENT))
        }
    };
    Ok((bytes, hs))
}

/// 132: http_post(url, body[, headers[, options]]) → {status, headers, body}
pub(crate) fn b_http_post(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if !(2..=4).contains(&args.len()) {
        return Err(RuntimeError::new(
            format!("http_post(): 2~4개 인자 필요 (url, body[, headers[, options]]), {}개 전달", args.len()),
            span,
        )
        .with_kind(crate::runtime::kind::ARGUMENT));
    }
    let url = str_arg("http_post", &args[0], span)?;
    let (body, hs) = http_body_and_headers("http_post", args.get(1), args.get(2), span)?;
    let opts = http_opts("http_post", args.get(3), span)?;
    run_http(vm, stop_depth, "http_post", "POST", &url, Some(body.as_deref().unwrap_or(&[])), &hs, opts, span)
}

/// 133: http_request(method, url[, body[, headers[, options]]]) → {status, headers, body}
pub(crate) fn b_http_request(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if !(2..=5).contains(&args.len()) {
        return Err(RuntimeError::new(
            format!("http_request(): 2~5개 인자 필요 (method, url[, body[, headers[, options]]]), {}개 전달", args.len()),
            span,
        )
        .with_kind(crate::runtime::kind::ARGUMENT));
    }
    let method = str_arg("http_request", &args[0], span)?;
    let url = str_arg("http_request", &args[1], span)?;
    let (body, hs) = http_body_and_headers("http_request", args.get(2), args.get(3), span)?;
    let opts = http_opts("http_request", args.get(4), span)?;
    run_http(vm, stop_depth, "http_request", &method, &url, body.as_deref(), &hs, opts, span)
}

/// 옵션의 on_line 콜백을 bang 함수 호출로 연결해 요청을 실행한다.
#[allow(clippy::too_many_arguments)]
fn run_http(
    vm: &mut Vm,
    stop_depth: usize,
    fname: &str,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    hs: &[(String, String)],
    opts: HttpOpts,
    span: Span,
) -> Result<VmValue, RuntimeError> {
    let Some(f) = opts.on_line.clone() else {
        return http_request(fname, method, url, body, hs, &opts, None, span);
    };
    let mut call = |line: String| -> Result<bool, RuntimeError> {
        let depth_before = vm.frames.len();
        vm.stack.push(f.clone());
        vm.stack.push(VmValue::Str(line));
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        let r = crate::vm::auto_resolve(vm.stack_pop(), span)?;
        Ok(!matches!(r, VmValue::Bool(false)))
    };
    http_request(fname, method, url, body, hs, &opts, Some(&mut call), span)
}

/// 122: tcp_read_bytes(conn) → Bytes (최대 4096바이트 1회 읽기, EOF면 빈 Bytes)
pub(crate) fn b_tcp_read_bytes(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tcp_read_bytes", &args, 1, span)?;
    match &args[0] {
        VmValue::TcpConn(c) => {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            let n = {
                let mut s = c
                    .lock()
                    .map_err(|_| RuntimeError::new("tcp_read_bytes(): 연결 잠금 실패", span))?;
                s.read(&mut buf)
                    .map_err(|e| RuntimeError::new(format!("tcp_read_bytes(): {e}"), span))?
            };
            Ok(VmValue::Bytes(Arc::new(buf[..n].to_vec())))
        }
        other => Err(RuntimeError::new(
            format!("tcp_read_bytes: TcpConn 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

// ── 구현 헬퍼 (vm.rs에서 이동) ──────────────────────────────────────────────

// ── http_get: 최소 HTTP/1.1 클라이언트 ────────────────────────────────────────
//
// http:// 는 자급 구현. https:// 는 `https` feature(기본 켜짐)일 때 rustls로 TLS를 입힌다
// (HTTP 처리 코드는 동일 — 스트림만 교체). feature를 끄면 https는 명시적 에러.
// Connection: close 로 요청하고 EOF까지 읽는다. Transfer-Encoding: chunked 디코딩.
// 리다이렉트는 따라가지 않는다(상태 코드와 location 헤더를 그대로 반환).

/// http_get이 다루는 스트림 (평문 TcpStream 또는 TLS 스트림).
trait HttpStream: std::io::Read + std::io::Write {}
impl<T: std::io::Read + std::io::Write> HttpStream for T {}

/// 파싱된 URL.
struct HttpUrl {
    tls: bool,
    host: String,
    port: u16,
    path: String,
}

/// URL 파싱: http(s)://host[:port]/path?query
fn parse_http_url(url: &str, span: Span) -> Result<HttpUrl, RuntimeError> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(RuntimeError::new(
            format!("http_get: URL은 http:// 또는 https:// 로 시작해야 함: '{url}'"),
            span,
        ));
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => {
            let port: u16 = p
                .parse()
                .map_err(|_| RuntimeError::new(format!("http_get: 잘못된 포트: '{p}'"), span))?;
            (h.to_string(), port)
        }
        None => (hostport.to_string(), if tls { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(RuntimeError::new(
            format!("http_get: 호스트가 비어 있음: '{url}'"),
            span,
        ));
    }
    Ok(HttpUrl {
        tls,
        host,
        port,
        path,
    })
}

/// TCP 스트림에 TLS를 입힌다. 설정(루트 인증서 포함)은 프로세스당 한 번만 만든다.
#[cfg(feature = "https")]
fn tls_wrap(mut tcp: std::net::TcpStream, host: &str) -> Result<Box<dyn HttpStream>, String> {
    use rustls::pki_types::ServerName;
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, String>> = OnceLock::new();
    let config = CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map(|b| Arc::new(b.with_root_certificates(roots).with_no_client_auth()))
            .map_err(|e| format!("TLS 설정 실패: {e}"))
        })
        .clone()?;
    let name = ServerName::try_from(host.to_string())
        .map_err(|e| format!("TLS 서버 이름이 잘못됨 '{host}': {e}"))?;
    let mut conn = rustls::ClientConnection::new(config, name)
        .map_err(|e| format!("TLS 연결 생성 실패: {e}"))?;
    // 핸드셰이크를 여기서 끝내 인증서 오류가 "TLS 핸드셰이크 실패"로 보고되게 한다
    // (그렇지 않으면 첫 쓰기 때 일어나 "요청 전송 실패"로 보인다).
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp)
            .map_err(|e| format!("TLS 핸드셰이크 실패: {e}"))?;
    }
    Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
}

#[cfg(not(feature = "https"))]
fn tls_wrap(_tcp: std::net::TcpStream, _host: &str) -> Result<Box<dyn HttpStream>, String> {
    Err(
        "이 bang 빌드는 https를 지원하지 않음 (`https` feature 없이 빌드됨) — http:// 를 쓰세요"
            .into(),
    )
}

/// TLS 피어가 close_notify 없이 연결을 끊는 경우(흔함)는 rustls가 UnexpectedEof로 알린다.
/// 본문 길이를 Content-Length/chunked로 이미 확인하므로 EOF와 같게 취급한다.
fn is_eof_like(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::UnexpectedEof
}

/// http_get(url) → {status: Int, headers: Map(소문자 키), body: Str}
pub(crate) fn http_get(url: &str, span: Span) -> Result<VmValue, RuntimeError> {
    http_request("http_get", "GET", url, None, &[], &HttpOpts::default(), None, span)
}

/// 기본 응답 대기 시간 (옵션 `timeout_ms`로 바꿀 수 있다 — Phase 63).
const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// http_post/http_request의 옵션 맵 (Phase 63).
pub(crate) struct HttpOpts {
    /// 연결·쓰기·읽기 각각의 대기 상한. None = 무제한 (`timeout_ms: 0`).
    pub timeout: Option<std::time::Duration>,
    /// 줄 단위 수신 콜백 (`on_line`) — 2xx 응답 본문을 받는 대로 한 줄씩 넘긴다.
    pub on_line: Option<VmValue>,
}

impl Default for HttpOpts {
    fn default() -> Self {
        Self { timeout: Some(std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS)), on_line: None }
    }
}

/// 옵션 맵 해석. 모르는 키나 잘못된 타입은 조용히 무시하지 않고 argument 에러 (헌법 7조).
pub(crate) fn http_opts(fname: &str, v: Option<&VmValue>, span: Span) -> Result<HttpOpts, RuntimeError> {
    let bad = |m: String| RuntimeError::new(format!("{fname}(): {m}"), span).with_kind(crate::runtime::kind::ARGUMENT);
    let mut o = HttpOpts::default();
    let m = match v {
        None | Some(VmValue::Nil) => return Ok(o),
        Some(VmValue::Map(m)) => m,
        Some(other) => return Err(bad(format!("options는 Map이어야 함, {} 발견", other.type_name()))),
    };
    for (k, val) in m.iter() {
        match (key_display(k).as_str(), val) {
            ("timeout_ms", VmValue::Int(ms)) if *ms >= 0 => {
                o.timeout = if *ms == 0 { None } else { Some(std::time::Duration::from_millis(*ms as u64)) };
            }
            ("timeout_ms", other) => {
                return Err(bad(format!("timeout_ms는 0 이상의 int여야 함 (0 = 무제한), {other} 발견")))
            }
            ("on_line", f @ (VmValue::Closure(_) | VmValue::Builtin(_))) => o.on_line = Some(f.clone()),
            ("on_line", other) => {
                return Err(bad(format!("on_line은 함수여야 함, {} 발견", other.type_name())))
            }
            (key, _) => return Err(bad(format!("알 수 없는 옵션 '{key}' (가능: timeout_ms, on_line)"))),
        }
    }
    Ok(o)
}

/// 응답 본문의 경계 (RFC 9112 6.3).
enum Framing {
    Chunked,
    Length(usize),
    /// 연결이 닫힐 때까지
    Eof,
    /// 본문 없음 (HEAD, 1xx, 204, 304)
    Empty,
}

enum ChunkState {
    Size,
    Data(usize),
    DataEnd,
    Trailer,
}

/// 받은 바이트를 조금씩 넣으면 본문 바이트를 조금씩 꺼내 주는 디코더 (Phase 63 — 스트리밍용).
/// 이전 구현은 전체를 모은 뒤 "0\r\n\r\n"을 찾아 끝을 판단해, 데이터 안에 같은 바이트가 있으면 오판했다.
struct BodyDecoder {
    raw: Vec<u8>,
    framing: Framing,
    chunk: ChunkState,
    done: bool,
}

impl BodyDecoder {
    fn new(framing: Framing, initial: Vec<u8>) -> Self {
        let done = matches!(framing, Framing::Empty | Framing::Length(0));
        Self { raw: initial, framing, chunk: ChunkState::Size, done }
    }

    /// 지금 가진 바이트로 꺼낼 수 있는 본문을 `out`에 붙인다.
    fn decode(&mut self, out: &mut Vec<u8>) -> Result<(), String> {
        if self.done {
            self.raw.clear();
            return Ok(());
        }
        match &mut self.framing {
            Framing::Empty => self.done = true,
            Framing::Eof => out.append(&mut self.raw),
            Framing::Length(left) => {
                let n = (*left).min(self.raw.len());
                out.extend(self.raw.drain(..n));
                *left -= n;
                if *left == 0 {
                    self.done = true;
                }
            }
            Framing::Chunked => loop {
                match self.chunk {
                    ChunkState::Size => {
                        let Some(end) = find_subseq(&self.raw, b"\r\n") else { break };
                        let line = String::from_utf8_lossy(&self.raw[..end]).into_owned();
                        let hex = line.split(';').next().unwrap_or("").trim();
                        let size = usize::from_str_radix(hex, 16)
                            .map_err(|_| format!("chunked 크기 줄 해석 실패: '{line}'"))?;
                        self.raw.drain(..end + 2);
                        self.chunk = if size == 0 { ChunkState::Trailer } else { ChunkState::Data(size) };
                    }
                    ChunkState::Data(left) => {
                        if self.raw.is_empty() {
                            break;
                        }
                        let n = left.min(self.raw.len());
                        out.extend(self.raw.drain(..n));
                        self.chunk = if left == n { ChunkState::DataEnd } else { ChunkState::Data(left - n) };
                    }
                    ChunkState::DataEnd => {
                        if self.raw.len() < 2 {
                            break;
                        }
                        if &self.raw[..2] != b"\r\n" {
                            return Err("chunked 데이터 뒤에 CRLF가 없음".into());
                        }
                        self.raw.drain(..2);
                        self.chunk = ChunkState::Size;
                    }
                    ChunkState::Trailer => {
                        let Some(end) = find_subseq(&self.raw, b"\r\n") else { break };
                        self.raw.drain(..end + 2);
                        if end == 0 {
                            self.done = true; // 빈 줄 = 트레일러 끝
                            break;
                        }
                    }
                }
            },
        }
        Ok(())
    }

    /// 연결이 닫혔을 때: 경계가 "닫힐 때까지"면 정상 끝, 아니면 잘린 응답.
    fn finish_on_eof(&mut self) -> Result<(), String> {
        match self.framing {
            Framing::Eof => {
                self.done = true;
                Ok(())
            }
            _ if self.done => Ok(()),
            _ => Err("응답 본문이 끝나기 전에 연결이 끊김".into()),
        }
    }
}

/// 본문 바이트를 줄로 나눠 콜백에 넘긴다 (줄 끝 \r 제거). 콜백이 false를 돌려주면 그만 받는다.
struct LineSplitter {
    pending: Vec<u8>,
}

impl LineSplitter {
    fn feed(
        &mut self,
        bytes: &[u8],
        sink: &mut dyn FnMut(String) -> Result<bool, RuntimeError>,
    ) -> Result<bool, RuntimeError> {
        self.pending.extend_from_slice(bytes);
        while let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=i).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !sink(String::from_utf8_lossy(&line).into_owned())? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn flush(&mut self, sink: &mut dyn FnMut(String) -> Result<bool, RuntimeError>) -> Result<(), RuntimeError> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            sink(String::from_utf8_lossy(&line).into_owned())?;
        }
        Ok(())
    }
}

/// 줄 콜백: `on_line` 옵션의 bang 함수를 부른다. 돌려준 값이 정확히 false면 수신을 멈춘다.
pub(crate) type LineSink<'a> = &'a mut dyn FnMut(String) -> Result<bool, RuntimeError>;

/// HTTP/1.1 요청 하나 (Phase 52: http_get을 일반화 — http_post/http_request가 공유).
/// `headers`는 기본 헤더(Host·Connection·User-Agent·Accept·Content-Length) 뒤에 붙고,
/// 같은 이름(대소문자 무시)이면 기본값 대신 쓰인다 (Host·Connection·Content-Length는 고정).
/// Phase 63: 대기 시간은 `opts.timeout`, 2xx 본문은 `line_sink`가 있으면 받는 대로 줄 단위로 넘기고 body는 nil.
#[allow(clippy::too_many_arguments)]
pub(crate) fn http_request(
    fname: &str,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    headers: &[(String, String)],
    opts: &HttpOpts,
    line_sink: Option<LineSink>,
    span: Span,
) -> Result<VmValue, RuntimeError> {
    use std::io::{Read, Write};
    let HttpUrl {
        tls,
        host,
        port,
        path,
    } = parse_http_url(url, span)?;
    let err = |m: String| RuntimeError::new(format!("{fname}('{url}'): {m}"), span);
    let timeout_msg = || match opts.timeout {
        Some(d) => format!("응답 대기 시간 초과 ({}ms — 옵션 timeout_ms로 늘릴 수 있음)", d.as_millis()),
        None => "응답 대기 시간 초과".to_string(),
    };
    let is_timeout = |e: &std::io::Error| {
        matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
    };
    if method.is_empty() || !method.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(RuntimeError::new(
            format!("{fname}(): 메서드는 대문자 영문이어야 함 (GET, POST, PUT, DELETE ...), '{method}' 발견"),
            span,
        )
        .with_kind(crate::runtime::kind::VALUE));
    }
    for (k, v) in headers {
        let bad_name = k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if bad_name || v.contains('\r') || v.contains('\n') {
            return Err(RuntimeError::new(
                format!("{fname}(): 잘못된 헤더 '{k}' (이름은 영숫자·-·_, 값에 줄바꿈 불가)"),
                span,
            )
            .with_kind(crate::runtime::kind::VALUE));
        }
    }

    let tcp = connect_tcp(&host, port, opts.timeout).map_err(|e| {
        if is_timeout(&e) { err(format!("연결 시간 초과: {e}")) } else { err(format!("연결 실패: {e}")) }
    })?;
    let _ = tcp.set_read_timeout(opts.timeout);
    let _ = tcp.set_write_timeout(opts.timeout);
    let mut stream: Box<dyn HttpStream> = if tls {
        tls_wrap(tcp, &host).map_err(err)?
    } else {
        Box::new(tcp)
    };

    let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if !has("user-agent") {
        req.push_str(&format!("User-Agent: bang/{}\r\n", env!("CARGO_PKG_VERSION")));
    }
    if !has("accept") {
        req.push_str("Accept: */*\r\n");
    }
    for (k, v) in headers {
        if ["host", "connection", "content-length"].iter().any(|f| k.eq_ignore_ascii_case(f)) {
            continue;
        }
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    let mut wire = req.into_bytes();
    if let Some(b) = body {
        wire.extend_from_slice(b);
    }
    stream.write_all(&wire).map_err(|e| {
        if is_timeout(&e) { err(timeout_msg()) } else { err(format!("요청 전송 실패: {e}")) }
    })?;

    // 헤더 끝(\r\n\r\n)까지 먼저 읽는다. read_to_end는 서버가 keep-alive면
    // 타임아웃까지 대기하므로 쓰지 않는다.
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let sep = loop {
        if let Some(p) = find_subseq(&raw, b"\r\n\r\n") {
            break p;
        }
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(e) if is_eof_like(&e) => 0,
            Err(e) if is_timeout(&e) => return Err(err(timeout_msg())),
            Err(e) => return Err(err(format!("응답 수신 실패: {e}"))),
        };
        if n == 0 {
            return Err(err("헤더 완성 전 연결 종료".into()));
        }
        raw.extend_from_slice(&buf[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..sep]).into_owned();

    // 상태줄: HTTP/1.1 200 OK
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: i64 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| err(format!("상태줄 파싱 실패: '{status_line}'")))?;

    // 헤더 (소문자 키 — bang-web 관례와 일치)
    let mut headers = crate::omap::OrderedMap::new();
    let mut chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            if key == "transfer-encoding" && val.to_lowercase().contains("chunked") {
                chunked = true;
            }
            if key == "content-length" {
                content_length = val.parse().ok();
            }
            headers.insert(key, VmValue::Str(val));
        }
    }

    let framing = if method == "HEAD" || (100..200).contains(&status) || status == 204 || status == 304 {
        Framing::Empty
    } else if chunked {
        Framing::Chunked
    } else if let Some(len) = content_length {
        Framing::Length(len)
    } else {
        Framing::Eof
    };
    let mut dec = BodyDecoder::new(framing, raw[sep + 4..].to_vec());

    // 2xx이고 콜백이 있으면 줄 단위로 넘긴다. 그 밖(에러 응답 등)은 body에 모아 돌려준다.
    let mut sink = if (200..300).contains(&status) { line_sink } else { None };
    let streaming = sink.is_some();
    let mut splitter = LineSplitter { pending: Vec::new() };
    let mut body_buf: Vec<u8> = Vec::new();
    let mut piece: Vec<u8> = Vec::new();
    let mut stopped = false;
    loop {
        piece.clear();
        dec.decode(&mut piece).map_err(&err)?;
        if !piece.is_empty() {
            match sink.as_mut() {
                Some(s) => {
                    if !splitter.feed(&piece, *s)? {
                        stopped = true;
                        break;
                    }
                }
                None => body_buf.extend_from_slice(&piece),
            }
        }
        if dec.done {
            break;
        }
        match stream.read(&mut buf) {
            Ok(0) => dec.finish_on_eof().map_err(&err)?,
            Ok(n) => dec.raw.extend_from_slice(&buf[..n]),
            Err(ref e) if is_eof_like(e) => dec.finish_on_eof().map_err(&err)?,
            Err(ref e) if is_timeout(e) => return Err(err(timeout_msg())),
            Err(e) => return Err(err(format!("본문 수신 실패: {e}"))),
        }
    }
    if let (Some(s), false) = (sink.as_mut(), stopped) {
        splitter.flush(*s)?;
    }

    let mut map = crate::omap::OrderedMap::new();
    map.insert("status".to_string(), VmValue::Int(status));
    map.insert("headers".to_string(), VmValue::Map(Arc::new(headers)));
    map.insert(
        "body".to_string(),
        if streaming { VmValue::Nil } else { VmValue::Str(String::from_utf8_lossy(&body_buf).into_owned()) },
    );
    Ok(VmValue::Map(Arc::new(map)))
}

/// 이름 해석 후 주소마다 연결을 시도한다 (대기 상한이 있으면 connect_timeout).
fn connect_tcp(host: &str, port: u16, timeout: Option<std::time::Duration>) -> std::io::Result<std::net::TcpStream> {
    use std::net::ToSocketAddrs;
    let Some(t) = timeout else {
        return std::net::TcpStream::connect((host, port));
    };
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "주소를 찾을 수 없음");
    for addr in (host, port).to_socket_addrs()? {
        match std::net::TcpStream::connect_timeout(&addr, t) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}
