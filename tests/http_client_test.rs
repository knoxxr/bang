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

#[test]
fn test_http_get_https_rejected() {
    // https 는 1차 미지원 — 조용히 실패하지 말고 잡히는 에러여야 함
    let (ok, stdout, _) = run_client(
        "try {\n http_get(\"https://example.com/\")\n} catch e {\n print(\"caught\")\n}");
    assert!(ok);
    assert_eq!(stdout.trim(), "caught");
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
