//! serve_event (논블로킹 이벤트 루프 서버) 통합 테스트.
//! 단일 이벤트 루프 스레드가 다수의 동시 연결 + keep-alive 요청을 멀티플렉싱하는지 검증.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bang_ev_{}_{n}_{tag}.bang", std::process::id()))
}

#[test]
fn test_serve_event_many_concurrent_connections() {
    let port = 9100 + (std::process::id() % 300) as u16 + COUNTER.fetch_add(1, Ordering::Relaxed) as u16;
    let addr = format!("127.0.0.1:{port}");

    let script = tmp("server");
    fs::write(&script, format!(r#"
fn handle(req) {{
    let body = "ok"
    return "HTTP/1.1 200 OK\r\nContent-Length: " + str(len(body)) + "\r\nConnection: keep-alive\r\n\r\n" + body
}}
serve_event("{addr}", handle)
"#)).unwrap();

    let mut child: Child = Command::new(BANG).arg(&script)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("서버 실행 실패");

    // 서버가 listen 할 때까지 대기
    let mut up = false;
    for _ in 0..50 {
        if TcpStream::connect(&addr).is_ok() { up = true; break; }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(up, "서버가 뜨지 않음");

    // 100개의 동시 연결을 동시에 열어 두고(각자 응답 대기) — 단일 이벤트 루프가 멀티플렉싱
    let mut streams = Vec::new();
    for _ in 0..100 {
        if let Ok(s) = TcpStream::connect(&addr) {
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            streams.push(s);
        }
    }
    let opened = streams.len();
    // 모든 연결에 먼저 요청을 보냄 (동시 미처리 상태 유지)
    for s in &mut streams {
        s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    }
    // 그 다음 모두 응답 수신
    let mut ok = 0;
    for s in &mut streams {
        let mut buf = [0u8; 256];
        if let Ok(n) = s.read(&mut buf) {
            if String::from_utf8_lossy(&buf[..n]).contains("ok") { ok += 1; }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(&script);

    assert!(opened >= 100, "100개 연결 생성 기대, {opened}");
    assert_eq!(ok, opened, "모든 동시 연결이 응답받아야 함(단일 루프 멀티플렉싱): {ok}/{opened}");
}

#[test]
fn test_serve_event_keep_alive_multiple_requests() {
    let port = 9500 + (std::process::id() % 300) as u16 + COUNTER.fetch_add(1, Ordering::Relaxed) as u16;
    let addr = format!("127.0.0.1:{port}");

    let script = tmp("ka");
    fs::write(&script, format!(r#"
fn handle(req) {{
    let body = "pong"
    return "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n" + body
}}
serve_event("{addr}", handle)
"#)).unwrap();

    let mut child = Command::new(BANG).arg(&script)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().expect("서버 실행 실패");
    for _ in 0..50 {
        if TcpStream::connect(&addr).is_ok() { break; }
        std::thread::sleep(Duration::from_millis(100));
    }

    // 같은 연결로 3번 요청(keep-alive)
    let mut count = 0;
    if let Ok(mut s) = TcpStream::connect(&addr) {
        s.set_read_timeout(Some(Duration::from_secs(3))).ok();
        for _ in 0..3 {
            s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut buf = [0u8; 128];
            match s.read(&mut buf) {
                Ok(n) if String::from_utf8_lossy(&buf[..n]).contains("pong") => count += 1,
                _ => break,
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(&script);

    assert_eq!(count, 3, "같은 연결로 3요청 모두 응답 기대(keep-alive)");
}
