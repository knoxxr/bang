//! http_get / tcp_connect (아웃바운드 네트워킹) 통합 테스트.
//! bang serve_event 서버를 띄우고 bang http_get 클라이언트로 왕복을 검증한다.

use std::fs;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bang_http_{}_{n}_{tag}.bang", std::process::id()))
}

fn spawn_server(port: u16) -> Child {
    let script = tmp("server");
    fs::write(&script, format!(r#"
fn handle(req) {{
    let body = json_stringify({{"ok": true, "path": "x"}})
    return "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: " + str(len(body)) + "\r\n\r\n" + body
}}
serve_event("127.0.0.1:{port}", handle)
"#)).unwrap();
    let child = Command::new(BANG).arg(&script)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().expect("서버 실행 실패");
    // listen 대기
    let addr = format!("127.0.0.1:{port}");
    for _ in 0..50 {
        if TcpStream::connect(&addr).is_ok() { break; }
        std::thread::sleep(Duration::from_millis(100));
    }
    child
}

fn run_client(script: &str) -> (bool, String, String) {
    let path = tmp("client");
    fs::write(&path, script).unwrap();
    let out = Command::new(BANG).arg(&path).output().expect("실행 실패");
    let _ = fs::remove_file(&path);
    (out.status.success(),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn test_http_get_basic() {
    let port = 9300 + (std::process::id() % 200) as u16 + COUNTER.fetch_add(1, Ordering::Relaxed) as u16;
    let mut server = spawn_server(port);

    let (ok, stdout, stderr) = run_client(&format!(r#"
let r = http_get("http://127.0.0.1:{port}/hello")
print(r.status)
print(r.headers["content-type"])
print(json_parse(r.body).ok)
"#));

    let _ = server.kill();
    let _ = server.wait();
    assert!(ok, "http_get 실패: {stderr}");
    assert_eq!(stdout, "200\napplication/json\ntrue\n");
}

#[test]
fn test_http_get_parallel_fanout() {
    // 킬러 데모의 핵심: parallel_map + http_get 이 동시 실행돼 모두 200
    let port = 9500 + (std::process::id() % 200) as u16 + COUNTER.fetch_add(1, Ordering::Relaxed) as u16;
    let mut server = spawn_server(port);

    let (ok, stdout, stderr) = run_client(&format!(r#"
let urls = ["http://127.0.0.1:{port}/a", "http://127.0.0.1:{port}/b", "http://127.0.0.1:{port}/c", "http://127.0.0.1:{port}/d"]
let pages = parallel_map(urls, fn(u) {{ return http_get(u).status }})
print(pages)
"#));

    let _ = server.kill();
    let _ = server.wait();
    assert!(ok, "팬아웃 실패: {stderr}");
    assert_eq!(stdout.trim(), "[200, 200, 200, 200]");
}

/// TLS가 아닌 응답을 주는 로컬 서버에 https:// 로 접속 → 잡히는 에러 (네트워크 불필요).
/// `https` feature가 꺼진 빌드에서는 "미지원" 에러가 잡혀야 한다.
#[test]
fn test_https_failure_is_catchable() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            use std::io::Write;
            let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        }
    });
    let (ok, stdout, stderr) = run_client(&format!(
        "try {{\n http_get(\"https://127.0.0.1:{port}/\")\n}} catch e {{\n print(e)\n}}"));
    let _ = server.join();
    assert!(ok, "프로세스가 비정상 종료: {stderr}");
    let msg = stdout.trim();
    assert!(msg.contains("http_get"), "에러 메시지: {msg}");
    if cfg!(feature = "https") {
        assert!(!msg.contains("지원하지 않음"), "https 빌드인데 미지원 에러: {msg}");
    } else {
        assert!(msg.contains("지원하지 않음"), "미지원 에러여야 함: {msg}");
    }
}

#[test]
fn test_http_get_rejects_unknown_scheme() {
    let (ok, stdout, _) = run_client(
        "try {\n http_get(\"ftp://example.com/\")\n} catch e {\n print(\"caught\")\n}");
    assert!(ok);
    assert_eq!(stdout.trim(), "caught");
}

/// 실제 인터넷 https (수동 실행: cargo test --test http_client_test -- --ignored).
/// CI가 외부 사이트 상태에 흔들리지 않도록 기본 제외.
#[test]
#[ignore]
#[cfg(feature = "https")]
fn test_https_real_internet() {
    let (ok, stdout, stderr) = run_client(
        "let r = http_get(\"https://example.com/\")\nprint(r.status)\nprint(contains(r.body, \"Example Domain\"))\n\
         let p = parallel_map([\"https://example.com/\", \"https://example.org/\"], fn(u) { return http_get(u).status })\nprint(p)");
    assert!(ok, "https 실패: {stderr}");
    assert_eq!(stdout.replace("\r\n", "\n"), "200\ntrue\n[200, 200]\n");
}

#[test]
fn test_http_get_connection_refused_errors() {
    // 닫힌 포트 → 잡히는 에러 (프로세스 abort 아님)
    let (ok, stdout, _) = run_client(
        "try {\n http_get(\"http://127.0.0.1:1/x\")\n} catch e {\n print(\"caught\")\n}");
    assert!(ok);
    assert_eq!(stdout.trim(), "caught");
}

#[test]
fn test_tcp_connect_lowlevel() {
    let port = 9700 + (std::process::id() % 200) as u16 + COUNTER.fetch_add(1, Ordering::Relaxed) as u16;
    let mut server = spawn_server(port);

    let (ok, stdout, stderr) = run_client(&format!(r#"
let c = tcp_connect("127.0.0.1:{port}")
tcp_write(c, "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
let resp = tcp_read(c)
print(contains(resp, "200 OK"))
tcp_close(c)
"#));

    let _ = server.kill();
    let _ = server.wait();
    assert!(ok, "tcp_connect 실패: {stderr}");
    assert_eq!(stdout.trim(), "true");
}

// ---- Phase 52: http_post / http_request ----

/// 요청 하나를 받아 {method, ct, token, body}를 JSON으로 돌려주는 1회용 서버 (Rust 스레드).
fn echo_server_once() -> (u16, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            let n = s.read(&mut chunk).unwrap();
            buf.extend_from_slice(&chunk[..n]);
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break p;
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let header = |name: &str| {
            head.lines()
                .find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
        };
        let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut body = buf[head_end + 4..].to_vec();
        while body.len() < len {
            let n = s.read(&mut chunk).unwrap();
            body.extend_from_slice(&chunk[..n]);
        }
        let method = head.split(' ').next().unwrap_or("").to_string();
        let esc = |v: Option<String>| v.map(|x| format!("{x:?}")).unwrap_or("null".into());
        let out = format!(
            "{{\"method\":\"{method}\",\"ct\":{},\"token\":{},\"body\":{:?}}}",
            esc(header("content-type")),
            esc(header("x-token")),
            String::from_utf8_lossy(&body)
        );
        let resp = format!("HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
        s.write_all(resp.as_bytes()).unwrap();
    });
    (port, h)
}

#[test]
fn test_http_post_json_body() {
    let (port, h) = echo_server_once();
    let (ok, out, err) = run_client(&format!(
        "let r = http_post(\"http://127.0.0.1:{port}/x\", {{\"a\": 1, \"b\": [2]}})\nlet j = json_parse(r.body)\nprint(r.status, j.method, j.ct, j.body)"
    ));
    h.join().unwrap();
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "201 POST application/json {\"a\":1,\"b\":[2]}");
}

#[test]
fn test_http_request_method_headers_bytes() {
    let (port, h) = echo_server_once();
    let (ok, out, err) = run_client(&format!(
        "let r = http_request(\"PUT\", \"http://127.0.0.1:{port}/\", bytes_from_str(\"raw\"), {{\"Content-Type\": \"text/csv\", \"X-Token\": \"t1\"}})\nlet j = json_parse(r.body)\nprint(j.method, j.ct, j.token, j.body)"
    ));
    h.join().unwrap();
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "PUT text/csv t1 raw");
}

#[test]
fn test_http_request_rejects_bad_method_and_header() {
    let (ok, out, err) = run_client(
        "try { http_request(\"get\", \"http://127.0.0.1:9/\") } catch e { print(e.kind) }\ntry { http_post(\"http://127.0.0.1:9/\", \"x\", {\"Bad Name\": \"v\"}) } catch e { print(e.kind) }\ntry { http_post(\"http://127.0.0.1:9/\", \"x\", {\"X\": \"a\\r\\nInjected: 1\"}) } catch e { print(e.kind) }",
    );
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "value\nvalue\nvalue");
}
