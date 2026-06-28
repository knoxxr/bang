//! 바이너리 파일 빌트인 통합 테스트 (file_size / tcp_send_file).
//! 빌드된 bang 바이너리로 서버를 띄우고 소켓으로 바이너리를 무손실 전송하는지 검증.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp(tag: &str, ext: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bang_bin_{}_{n}_{tag}.{ext}", std::process::id()))
}

/// file_size 가 실제 바이트 수를 반환한다 (UTF-8 무관).
#[test]
fn test_file_size() {
    let path = tmp("size", "dat");
    // 모든 바이트값 0..=255 (비-UTF8 포함)
    let data: Vec<u8> = (0u16..=255).map(|b| b as u8).collect();
    fs::write(&path, &data).unwrap();

    let script = format!("print(file_size(\"{}\"))", path.to_string_lossy());
    let out = Command::new(BANG).arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().unwrap().write_all(script.as_bytes())?;
            c.wait_with_output()
        })
        .expect("실행 실패");
    let _ = fs::remove_file(&path);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "256");
}

/// file_size 가 없는 파일이면 에러(비정상 종료).
#[test]
fn test_file_size_missing_errors() {
    let out = Command::new(BANG).arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().unwrap().write_all(b"print(file_size(\"/no/such/file/xyz\"))")?;
            c.wait_with_output()
        })
        .expect("실행 실패");
    assert!(!out.status.success(), "없는 파일은 에러여야 함");
}

/// tcp_send_file 로 바이너리(모든 바이트값)를 무손실 전송한다.
#[test]
fn test_tcp_send_file_binary_roundtrip() {
    // 256바이트 × 8 = 2048바이트, 모든 바이트값 포함
    let bin = tmp("payload", "bin");
    let data: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
    fs::write(&bin, &data).unwrap();

    let port = 9000 + (std::process::id() % 500) as u16 + COUNTER.fetch_add(1, Ordering::Relaxed) as u16;
    let addr = format!("127.0.0.1:{port}");

    let script = tmp("server", "bang");
    fs::write(&script, format!(r#"
let path = "{}"
fn handle(conn) {{
    let req = tcp_read_until(conn, "\r\n\r\n")
    let size = file_size(path)
    tcp_write(conn, "HTTP/1.1 200 OK\r\nContent-Length: " + str(size) + "\r\nConnection: close\r\n\r\n")
    tcp_send_file(conn, path)
    tcp_close(conn)
}}
let server = tcp_listen("{}")
print("ready")
while true {{
    let conn = tcp_accept(server)
    spawn handle(conn)
}}
"#, bin.to_string_lossy(), addr)).unwrap();

    let mut child: Child = Command::new(BANG).arg(&script)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("서버 실행 실패");

    // 서버가 listen 할 때까지 연결 재시도
    let mut stream = None;
    for _ in 0..50 {
        if let Ok(s) = TcpStream::connect(&addr) {
            stream = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let result: Vec<u8> = {
        let mut s = stream.expect("서버 연결 실패");
        s.write_all(b"GET /file HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut resp = Vec::new();
        s.read_to_end(&mut resp).unwrap();
        // 헤더/바디 분리
        let sep = resp.windows(4).position(|w| w == b"\r\n\r\n").expect("헤더 끝 없음");
        resp[sep + 4..].to_vec()
    };

    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(&bin);
    let _ = fs::remove_file(&script);

    assert_eq!(result.len(), data.len(), "전송 바이트 수 불일치");
    assert_eq!(result, data, "바이너리 내용 불일치(손실 발생)");
}
