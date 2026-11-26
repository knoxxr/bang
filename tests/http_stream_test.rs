//! Phase 63: HTTP 대기 시간 옵션·줄 단위 스트리밍(on_line)·잘린 응답 에러 + LLM 예제 종단 테스트.
//! Rust 스레드로 띄운 가짜 HTTP 서버에 bang 클라이언트를 붙인다 (외부 네트워크 불필요).

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 요청 하나를 읽는다: (경로, 헤더 원문, 본문).
fn read_request(c: &mut TcpStream) -> Option<(String, String, String)> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        let n = c.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&buf[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let len: usize = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
        .unwrap_or(0);
    let mut body = raw[head_end + 4..].to_vec();
    while body.len() < len {
        let n = c.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
    Some((path, head, String::from_utf8_lossy(&body).into_owned()))
}

fn chunk(c: &mut TcpStream, data: &str) {
    let _ = c.write_all(format!("{:x}\r\n{data}\r\n", data.len()).as_bytes());
    let _ = c.flush();
}

/// 공식 문서의 Messages API 형식을 흉내 낸다 (비스트리밍 JSON / 스트리밍 SSE).
fn messages_api(c: &mut TcpStream, head: &str, body: &str) {
    let lower = head.to_ascii_lowercase();
    if !lower.contains("x-api-key: test-key") || !lower.contains("anthropic-version: 2026-10-21") {
        let b = r#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#;
        let _ = c.write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\n\r\n{b}", b.len()).as_bytes());
        return;
    }
    if body.contains("\"stream\":true") {
        let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n");
        chunk(c, "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\"}}\n\n");
        chunk(c, "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n");
        chunk(c, "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"\"}}\n\n");
        for piece in ["첫 줄", "이요\\n둘째", " 줄\\n", "셋째"] {
            std::thread::sleep(Duration::from_millis(50));
            chunk(c, &format!("event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":1,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{piece}\"}}}}\n\n"));
        }
        chunk(c, "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12}}\n\n");
        chunk(c, "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
        let _ = c.write_all(b"0\r\n\r\n");
        return;
    }
    // 질문에 든 단어로 답을 정한다 (병렬 응답이 질문 순서대로 모이는지 확인용 — 일부러 늦게 끝나게)
    let (answer, delay) = if body.contains("Rust") {
        ("소유권 답", 300)
    } else if body.contains("GIL") {
        ("GIL 답", 150)
    } else {
        ("구조적 답", 0)
    };
    std::thread::sleep(Duration::from_millis(delay));
    let b = format!(
        r#"{{"id":"msg_x","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{{"type":"thinking","thinking":"","signature":"s"}},{{"type":"text","text":" {answer} "}}],"stop_reason":"end_turn","usage":{{"input_tokens":5,"output_tokens":3}}}}"#
    );
    let _ = c.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{b}", b.len()).as_bytes());
}

fn serve(mut c: TcpStream) {
    let Some((path, head, body)) = read_request(&mut c) else { return };
    match path.as_str() {
        "/v1/messages" => messages_api(&mut c, &head, &body),
        "/lines" => {
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
            for i in 0..5 {
                chunk(&mut c, &format!("line {i}\r\n"));
                std::thread::sleep(Duration::from_millis(100));
            }
            chunk(&mut c, "no newline at end");
            let _ = c.write_all(b"0\r\n\r\n");
        }
        "/slow" => {
            std::thread::sleep(Duration::from_millis(1500));
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nslow");
        }
        "/cut" => {
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort");
        }
        "/tricky" => {
            // 데이터 안에 "0\r\n\r\n"이 들어 있다 — 이전 구현은 여기서 끝으로 오판했다
            let data = "x0\r\n\r\nafter";
            let _ = c.write_all(format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{data}\r\n0\r\n\r\n", data.len()).as_bytes());
        }
        "/err" => {
            let b = r#"{"error": "rate limited"}"#;
            let _ = c.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nContent-Length: {}\r\n\r\n{b}", b.len()).as_bytes());
        }
        _ => {
            let _ = c.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    }
}

/// 가짜 서버를 띄우고 포트를 돌려준다 (연결마다 스레드, 테스트 프로세스가 끝나면 사라짐).
fn start_server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            std::thread::spawn(move || serve(c));
        }
    });
    port
}

fn run(src: &str, envs: &[(&str, String)]) -> (bool, String, String) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path: PathBuf = std::env::temp_dir().join(format!("bang_http_stream_{}_{n}.bang", std::process::id()));
    fs::write(&path, src).unwrap();
    let mut cmd = Command::new(BANG);
    cmd.arg(&path);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("실행 실패");
    let _ = fs::remove_file(&path);
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn test_http_timeout_option() {
    let port = start_server();
    let (ok, out, err) = run(&format!(r#"
let u = "http://127.0.0.1:{port}/slow"
try {{ http_request("GET", u, nil, nil, {{"timeout_ms": 300}}) }} catch e {{ print(e.kind, ends_with(e.message, "응답 대기 시간 초과 (300ms — 옵션 timeout_ms로 늘릴 수 있음)")) }}
print(http_request("GET", u, nil, nil, {{"timeout_ms": 5000}}).body)
print(http_request("GET", u, nil, nil, {{"timeout_ms": 0}}).body)
print(http_get(u).body)
"#), &[]);
    assert!(ok, "{err}");
    assert_eq!(out, "io true\nslow\nslow\nslow\n");
}

#[test]
fn test_http_truncated_body_is_error_and_chunk_data_safe() {
    let port = start_server();
    let (ok, out, err) = run(&format!(r#"
try {{ print(http_get("http://127.0.0.1:{port}/cut").body) }} catch e {{ print(e.kind, ends_with(e.message, "응답 본문이 끝나기 전에 연결이 끊김")) }}
print(json_stringify(http_get("http://127.0.0.1:{port}/tricky").body))
"#), &[]);
    assert!(ok, "{err}");
    // 이전: 잘린 본문 "short"를 조용히 돌려줌 / "x0" 뒤를 잘라 먹음
    assert_eq!(out, "io true\n\"x0\\r\\n\\r\\nafter\"\n");
}

#[test]
fn test_http_on_line_streams_incrementally() {
    let port = start_server();
    let (ok, out, err) = run(&format!(r#"
let start = now_ms()
let seen = []
let r = http_request("GET", "http://127.0.0.1:{port}/lines", nil, nil, {{"on_line": fn(line) {{
    seen += [[line, now_ms() - start]]
}}}})
print(r.status, r.body)
print(seen.map(fn(p) {{ return p[0] }}))
print(seen[0][1] < 300, seen[-1][1] >= 400)
"#), &[]);
    assert!(ok, "{err}");
    // 첫 줄은 응답 전체(약 500ms)를 기다리지 않고 바로 도착한다. 끝의 줄바꿈 없는 조각도 마지막 줄로 넘어온다.
    assert_eq!(out, "200 nil\n[line 0, line 1, line 2, line 3, line 4, no newline at end]\ntrue true\n");
}

#[test]
fn test_http_on_line_stop_throw_and_error_status() {
    let port = start_server();
    let (ok, out, err) = run(&format!(r#"
let base = "http://127.0.0.1:{port}"
let got = []
http_request("GET", base + "/lines", nil, nil, {{"on_line": fn(l) {{
    got += [l]
    if len(got) == 2 {{ return false }}
}}}})
print(got)
try {{
    http_request("GET", base + "/lines", nil, nil, {{"on_line": fn(l) {{ throw {{"stopped_at": l}} }}}})
}} catch e {{ print(e.stopped_at) }}
let r = http_post(base + "/err", {{"q": 1}}, nil, {{"on_line": fn(_) {{ print("호출되면 안 됨") }}}})
print(r.status, r.body)
for bad in [{{"timeout": 1}}, {{"timeout_ms": -1}}, {{"on_line": 3}}, "x"] {{
    try {{ http_request("GET", base + "/err", nil, nil, bad) }} catch e {{ print(e.kind, e.message) }}
}}
"#), &[]);
    assert!(ok, "{err}");
    assert_eq!(
        out,
        "[line 0, line 1]\nline 0\n429 {\"error\": \"rate limited\"}\n\
         argument http_request(): 알 수 없는 옵션 'timeout' (가능: timeout_ms, on_line)\n\
         argument http_request(): timeout_ms는 0 이상의 int여야 함 (0 = 무제한), -1 발견\n\
         argument http_request(): on_line은 함수여야 함, Int 발견\n\
         argument http_request(): options는 Map이어야 함, Str 발견\n"
    );
}

/// examples/llm_fanout.bang을 공식 문서 형식의 가짜 Messages API로 끝까지 돌린다.
#[test]
fn test_llm_fanout_example_against_mock_api() {
    let port = start_server();
    let src = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/llm_fanout.bang")).unwrap();
    let (ok, out, err) = run(
        &src,
        &[("ANTHROPIC_API_KEY", "test-key".into()), ("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}"))],
    );
    assert!(ok, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "Q1. Rust의 소유권을 한 문장으로 설명해 줘.");
    assert_eq!(lines[1], "A1. 소유권 답");
    assert_eq!(lines[3], "A2. GIL 답");
    assert_eq!(lines[5], "A3. 구조적 답");
    // 세 요청(300ms·150ms·0ms)이 동시에 나갔다면 합(450ms)보다 짧다
    let ms: u64 = lines[6].trim_start_matches("(3개 동시 호출: ").trim_end_matches("ms)").parse().unwrap();
    assert!(ms < 440, "병렬이 아님: {ms}ms");
    assert_eq!(&lines[7..], ["", "스트리밍:", "첫 줄이요", "둘째 줄", "셋째", "(stop_reason: end_turn)"]);

    // 키가 틀리면 401 → 예제가 상태와 본문을 담아 던진다 (조용히 넘어가지 않음)
    let (ok, _, err) = run(
        &src,
        &[("ANTHROPIC_API_KEY", "wrong".into()), ("ANTHROPIC_BASE_URL", format!("http://127.0.0.1:{port}"))],
    );
    assert!(!ok);
    assert!(err.contains("401"), "{err}");
}
