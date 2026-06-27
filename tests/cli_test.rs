//! CLI 통합 테스트 — 빌드된 `bang` 바이너리를 직접 실행해
//! Python 같은 실행 UX(베어 파일 실행, shebang, stdin, --version)를 검증한다.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 빌드된 bang 바이너리 경로 (cargo가 주입).
const BANG: &str = env!("CARGO_BIN_EXE_bang");

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_bang(contents: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("bang_cli_{}_{n}.bang", std::process::id()));
    fs::write(&path, contents).expect("임시 파일 쓰기 실패");
    path
}

fn stdout_of(args: &[&str]) -> String {
    let out = Command::new(BANG).args(args).output().expect("실행 실패");
    assert!(
        out.status.success(),
        "비정상 종료: {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout UTF-8 아님")
}

/// `bang --version` → "bang <version>" 출력.
#[test]
fn test_version_flag() {
    let out = stdout_of(&["--version"]);
    assert!(out.starts_with("bang "), "got: {out}");
    // Cargo 패키지 버전과 일치해야 한다.
    assert!(out.contains(env!("CARGO_PKG_VERSION")), "got: {out}");
}

#[test]
fn test_version_subcommand() {
    assert_eq!(stdout_of(&["version"]), stdout_of(&["--version"]));
}

/// 베어 파일 실행: `bang script.bang` (run 생략).
#[test]
fn test_bare_file_execution() {
    let f = temp_bang("print(6 * 7)\n");
    let out = stdout_of(&[f.to_str().unwrap()]);
    assert_eq!(out, "42\n");
    let _ = fs::remove_file(&f);
}

/// `bang run script.bang` (명시적 서브커맨드)도 동일하게 동작.
#[test]
fn test_explicit_run_still_works() {
    let f = temp_bang("print(\"hi\")\n");
    let out = stdout_of(&["run", f.to_str().unwrap()]);
    assert_eq!(out, "hi\n");
    let _ = fs::remove_file(&f);
}

/// shebang 줄이 있는 파일도 정상 실행된다.
#[test]
fn test_shebang_file_runs() {
    let f = temp_bang("#!/usr/bin/env bang\nprint(123)\n");
    let out = stdout_of(&[f.to_str().unwrap()]);
    assert_eq!(out, "123\n");
    let _ = fs::remove_file(&f);
}

/// 표준 입력에서 소스 읽기: `echo '...' | bang -`.
#[test]
fn test_stdin_execution() {
    let mut child = Command::new(BANG)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn 실패");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"print(1 + 2)\n")
        .expect("stdin 쓰기 실패");
    let out = child.wait_with_output().expect("대기 실패");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "3\n");
}

/// 존재하지 않는 파일/알 수 없는 명령 → 비정상 종료 + 안내.
#[test]
fn test_unknown_arg_fails() {
    let out = Command::new(BANG)
        .arg("does_not_exist.bang")
        .output()
        .expect("실행 실패");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("알 수 없는 명령") || stderr.contains("존재하지 않는"));
}
