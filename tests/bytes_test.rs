//! 바이트 값 타입 통합 테스트 (Phase 43, 빌트인 117-122).
//! 불변 Bytes: 변환·len·인덱스·slice·==·+ / 파일·소켓 무손실 왕복 / 값 의미론 / 채널·spawn 전달.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("bang_bytes_{}_{n}_{tag}", std::process::id()))
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn run(script: &str) -> (bool, String, String) {
    let out = Command::new(BANG)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().unwrap().write_all(script.as_bytes())?;
            c.wait_with_output()
        })
        .expect("bang 실행 실패");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run_ok(script: &str) -> String {
    let (ok, out, err) = run(script);
    assert!(ok, "실행 실패\nstdout:\n{out}\nstderr:\n{err}");
    out
}

#[test]
fn test_bytes_basic_ops() {
    let out = run_ok(
        "let b = bytes_from_str(\"héllo\")\n\
         print(len(b), type(b))\n\
         print(b[0], b[-1])\n\
         print(bytes_to_hex(slice(b, 0, 3)))\n\
         print(bytes_to_str(b))\n\
         let h = bytes_from_hex(\"00FF10\")\n\
         print(h == bytes_from_hex(\"00ff10\"), h == b)\n\
         print(bytes_to_hex(h + h))\n\
         print(h)",
    );
    assert_eq!(
        out,
        "6 Bytes\n104 111\n68c3a9\nhéllo\ntrue false\n00ff1000ff10\n<bytes 3: 00ff10>\n"
    );
}

#[test]
fn test_bytes_display_truncates() {
    let out = run_ok("print(bytes_from_str(repeat(\"a\", 40)))");
    assert_eq!(out, format!("<bytes 40: {}…>\n", "61".repeat(32)));
}

#[test]
fn test_bytes_errors_are_catchable() {
    let out = run_ok(
        "let h = bytes_from_hex(\"00ff\")\n\
         try { bytes_to_str(h) } catch e { print(\"utf8\") }\n\
         try { bytes_from_hex(\"abc\") } catch e { print(\"odd\") }\n\
         try { bytes_from_hex(\"zz\") } catch e { print(\"nonhex\") }\n\
         try { h[0] = 1 } catch e { print(\"immutable\") }\n\
         try { h[2] } catch e { print(\"range\") }\n\
         try { json_stringify(h) } catch e { print(\"json\") }\n\
         try { bytes_to_hex(\"s\") } catch e { print(\"type\") }\n\
         try { write_file(\"x\", 1) } catch e { print(\"write-type\") }",
    );
    assert_eq!(
        out,
        "utf8\nodd\nnonhex\nimmutable\nrange\njson\ntype\nwrite-type\n"
    );
}

/// 값 의미론: 재바인딩·연결이 이전 바인딩에 영향을 주지 않는다.
#[test]
fn test_bytes_value_semantics() {
    let out = run_ok(
        "let a = bytes_from_hex(\"01\")\n\
         let b = a\n\
         a = a + bytes_from_hex(\"02\")\n\
         print(bytes_to_hex(a), bytes_to_hex(b))\n\
         fn grow(x) { x = x + x\n return len(x) }\n\
         print(grow(b), len(b))",
    );
    assert_eq!(out, "0102 01\n2 1\n");
}

#[test]
fn test_bytes_type_hint() {
    let out = run_ok(
        "fn hex(x: bytes) -> str { return bytes_to_hex(x) }\n\
         print(hex(bytes_from_hex(\"0a\")))\n\
         try { hex(\"s\") } catch e { print(\"hint\") }",
    );
    assert_eq!(out, "0a\nhint\n");
}

/// 0..=255 전체 바이트값(비-UTF-8 포함)이 파일 쓰기/이어쓰기/읽기에서 무손실.
#[test]
fn test_bytes_file_roundtrip_all_values() {
    let path = tmp("all.bin");
    let all: Vec<u8> = (0u16..=255).map(|b| b as u8).collect();
    let hex: String = all.iter().map(|b| format!("{b:02x}")).collect();
    let out = run_ok(&format!(
        "let p = \"{}\"\n\
         let d = bytes_from_hex(\"{hex}\")\n\
         write_file(p, slice(d, 0, 100))\n\
         file_append(p, slice(d, 100, 256))\n\
         let r = file_read_bytes(p)\n\
         print(len(r), r == d)",
        path_str(&path)
    ));
    let on_disk = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "256 true\n");
    assert_eq!(on_disk, all);
}

#[test]
fn test_file_read_bytes_missing_errors() {
    let out =
        run_ok("try { file_read_bytes(\"/no/such/file/xyz.bin\") } catch e { print(\"missing\") }");
    assert_eq!(out, "missing\n");
}

/// 소켓으로 비-UTF-8 바이트를 보내고 그대로 되돌려 받는다 (bang 서버 ↔ bang 클라이언트).
#[test]
fn test_bytes_socket_echo() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let out = run_ok(&format!(
        "let srv = tcp_listen(\"127.0.0.1:{port}\")\n\
         fn serve() {{\n\
           let c = tcp_accept(srv)\n\
           let d = tcp_read_bytes(c)\n\
           tcp_write(c, d)\n\
           tcp_close(c)\n\
           return len(d)\n\
         }}\n\
         let f = spawn serve()\n\
         let c = tcp_connect(\"127.0.0.1:{port}\")\n\
         let payload = bytes_from_hex(\"00ff80fe0a0dc3\")\n\
         tcp_write(c, payload)\n\
         let back = tcp_read_bytes(c)\n\
         print(back == payload, f)"
    ));
    assert_eq!(out, "true 7\n");
}

/// 채널(컨테이너 안쪽 포함)과 spawn 인자로 Bytes가 전달된다.
#[test]
fn test_bytes_across_channel_and_spawn() {
    let out = run_ok(
        "let h = bytes_from_hex(\"cafe\")\n\
         let ch = channel(1)\n\
         send(ch, [h, {\"k\": h}])\n\
         let got = recv(ch)\n\
         print(got[0] == h, got[1].k == h)\n\
         fn hex(x) { return bytes_to_hex(x) }\n\
         let f = spawn hex(h)\n\
         print(f)",
    );
    assert_eq!(out, "true true\ncafe\n");
}
