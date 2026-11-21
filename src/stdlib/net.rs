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

/// 132: http_post(url, body[, headers]) → {status, headers, body}
pub(crate) fn b_http_post(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if !(2..=3).contains(&args.len()) {
        return Err(RuntimeError::new(
            format!("http_post(): 2~3개 인자 필요 (url, body[, headers]), {}개 전달", args.len()),
            span,
        )
        .with_kind(crate::runtime::kind::ARGUMENT));
    }
    let url = str_arg("http_post", &args[0], span)?;
    let (body, hs) = http_body_and_headers("http_post", args.get(1), args.get(2), span)?;
    http_request("http_post", "POST", &url, body.as_deref().or(Some(&[])), &hs, span)
}

/// 133: http_request(method, url[, body[, headers]]) → {status, headers, body}
pub(crate) fn b_http_request(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if !(2..=4).contains(&args.len()) {
        return Err(RuntimeError::new(
            format!("http_request(): 2~4개 인자 필요 (method, url[, body[, headers]]), {}개 전달", args.len()),
            span,
        )
        .with_kind(crate::runtime::kind::ARGUMENT));
    }
    let method = str_arg("http_request", &args[0], span)?;
    let url = str_arg("http_request", &args[1], span)?;
    let (body, hs) = http_body_and_headers("http_request", args.get(2), args.get(3), span)?;
    http_request("http_request", &method, &url, body.as_deref(), &hs, span)
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

/// Transfer-Encoding: chunked 본문 디코딩 (크기 줄은 16진수 [;확장] CRLF).
fn decode_chunked(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        // 크기 줄 찾기
        let line_end = find_subseq(&body[pos..], b"\r\n")? + pos;
        let size_str = std::str::from_utf8(&body[pos..line_end]).ok()?;
        let size_hex = size_str.split(';').next()?.trim();
        let size = usize::from_str_radix(size_hex, 16).ok()?;
        pos = line_end + 2;
        if size == 0 {
            return Some(out); // 마지막 청크 (트레일러 무시)
        }
        if pos + size > body.len() {
            return None; // 잘린 응답
        }
        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2; // 청크 뒤 CRLF
    }
}

/// http_get(url) → {status: Int, headers: Map(소문자 키), body: Str}
pub(crate) fn http_get(url: &str, span: Span) -> Result<VmValue, RuntimeError> {
    http_request("http_get", "GET", url, None, &[], span)
}

/// HTTP/1.1 요청 하나 (Phase 52: http_get을 일반화 — http_post/http_request가 공유).
/// `headers`는 기본 헤더(Host·Connection·User-Agent·Accept·Content-Length) 뒤에 붙고,
/// 같은 이름(대소문자 무시)이면 기본값 대신 쓰인다 (Host·Connection·Content-Length는 고정).
pub(crate) fn http_request(
    fname: &str,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    headers: &[(String, String)],
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

    let tcp = std::net::TcpStream::connect((host.as_str(), port))
        .map_err(|e| err(format!("연결 실패: {e}")))?;
    let timeout = Some(std::time::Duration::from_secs(10));
    let _ = tcp.set_read_timeout(timeout);
    let _ = tcp.set_write_timeout(timeout);
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
    stream
        .write_all(&wire)
        .map_err(|e| err(format!("요청 전송 실패: {e}")))?;

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

    // 헤더 이후 이미 읽은 본문 바이트 + 남은 본문 수신.
    let mut body_buf = raw[sep + 4..].to_vec();
    // 종료 조건: chunked면 "0\r\n\r\n", Content-Length면 그 길이, 둘 다 없으면 EOF(close).
    loop {
        let done = if chunked {
            find_subseq(&body_buf, b"0\r\n\r\n").is_some()
        } else if let Some(len) = content_length {
            body_buf.len() >= len
        } else {
            false // EOF까지
        };
        if done {
            break;
        }
        let n = match stream.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(ref e) if is_eof_like(e) => break,
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break
            }
            Err(e) => return Err(err(format!("본문 수신 실패: {e}"))),
        };
        body_buf.extend_from_slice(&buf[..n]);
    }

    let body = if chunked {
        decode_chunked(&body_buf).ok_or_else(|| err("chunked 본문 디코딩 실패".into()))?
    } else if let Some(len) = content_length {
        body_buf.truncate(len);
        body_buf
    } else {
        body_buf
    };

    let mut map = crate::omap::OrderedMap::new();
    map.insert("status".to_string(), VmValue::Int(status));
    map.insert("headers".to_string(), VmValue::Map(Arc::new(headers)));
    map.insert(
        "body".to_string(),
        VmValue::Str(String::from_utf8_lossy(&body).into_owned()),
    );
    Ok(VmValue::Map(Arc::new(map)))
}
