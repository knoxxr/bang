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

/// bang new <dir> 가 프로젝트를 스캐폴딩하고, 생성된 main.bang 이 실행된다.
#[test]
fn test_bang_new_scaffold() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bang_new_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    // 생성
    let out = Command::new(BANG).arg("new").arg(&dir).output().expect("실행 실패");
    assert!(out.status.success(), "bang new 실패: {}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join("main.bang").exists(), "main.bang 없음");
    assert!(dir.join("bang.toml").exists(), "bang.toml 없음");

    // 생성된 main.bang 실행 → 정상 종료 + 인사말 출력
    let run = Command::new(BANG).arg(dir.join("main.bang")).output().expect("실행 실패");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "생성된 main.bang 실행 실패: {}", String::from_utf8_lossy(&run.stderr));
    assert!(stdout.contains("bang"), "인사말 출력 기대: {stdout}");
    assert!(stdout.contains("[2, 4, 6, 8]"), "parallel_map 출력 기대: {stdout}");

    // 이미 존재하면 에러
    let dup = Command::new(BANG).arg("new").arg(&dir).output().expect("실행 실패");
    assert!(!dup.status.success(), "이미 존재하는 디렉토리는 에러여야 함");

    let _ = fs::remove_dir_all(&dir);
}

/// 제거된 백엔드 플래그(--interp/--jit)는 파일 경로로 오인되지 않고 명확한 에러를 낸다 (Phase 44).
#[test]
fn test_removed_backend_flags_error_clearly() {
    let path = temp_bang("print(1)\n");
    for flag in ["--interp", "--jit"] {
        let out = Command::new(BANG)
            .args(["run", flag])
            .arg(&path)
            .output()
            .expect("실행 실패");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{flag}: 실패해야 함");
        assert!(stderr.contains("제거"), "{flag}: stderr = {stderr}");
        assert!(out.stdout.is_empty(), "{flag}: 실행되면 안 됨");
    }
    let _ = fs::remove_file(&path);
}

// ---- Phase 49: bang test / bang fmt ----

fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bang_cli_{tag}_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("임시 디렉터리 생성 실패");
    dir
}

#[test]
fn test_bang_test_runs_test_functions_isolated() {
    let dir = temp_dir("test");
    fs::create_dir_all(dir.join("bang_modules/dep")).unwrap();
    // bang_modules 안의 테스트는 발견하지 않는다
    fs::write(dir.join("bang_modules/dep/x_test.bang"), "fn test_never() { assert(false) }\n").unwrap();
    fs::write(dir.join("helper.bang"), "fn test_ignored() { assert(false) }\n").unwrap();
    fs::write(
        dir.join("math_test.bang"),
        "let counter = 0\nfn test_one() {\n    counter += 1\n    assert_eq(counter, 1)\n}\n\
         fn test_two() {\n    counter += 1\n    assert_eq(counter, 1)\n}\n",
    )
    .unwrap();
    let out = Command::new(BANG).arg("test").arg(&dir).output().expect("실행 실패");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("ok    test_one") && stdout.contains("ok    test_two"), "{stdout}");
    assert!(stdout.contains("2개 통과, 0개 실패"), "{stdout}");
    assert!(!stdout.contains("test_never") && !stdout.contains("test_ignored"), "{stdout}");

    // 실패하면 exit 1 + 실패 위치
    fs::write(dir.join("fail_test.bang"), "fn test_bad() {\n    assert_eq(1 + 1, 3)\n}\n").unwrap();
    let out = Command::new(BANG).arg("test").arg(&dir).output().expect("실행 실패");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("FAIL  test_bad") && stdout.contains("assert_eq 실패: 기대 3, 실제 2"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_bang_fmt_check_and_write() {
    let dir = temp_dir("fmt");
    let file = dir.join("messy.bang");
    fs::write(&file, "fn f(x) {\nif x {\n      return 1\n}\nreturn 2   \n}\n\n\nprint(f(true))\n").unwrap();
    let out = Command::new(BANG).args(["fmt", "--check"]).arg(&dir).output().expect("실행 실패");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("포맷 필요"));
    let out = Command::new(BANG).arg("fmt").arg(&file).output().expect("실행 실패");
    assert!(out.status.success());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "fn f(x) {\n    if x {\n        return 1\n    }\n    return 2\n}\n\nprint(f(true))\n"
    );
    let out = Command::new(BANG).args(["fmt", "--check"]).arg(&dir).output().expect("실행 실패");
    assert!(out.status.success(), "포맷 후에는 --check 통과");
    // 렉싱 오류 파일은 건드리지 않는다
    let bad = dir.join("bad.bang");
    fs::write(&bad, "let s = \"open\n").unwrap();
    let out = Command::new(BANG).arg("fmt").arg(&bad).output().expect("실행 실패");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&bad).unwrap(), "let s = \"open\n");
    let _ = fs::remove_dir_all(&dir);
}

/// 읽는 쪽이 파이프를 닫으면(bang x | head) 에러 메시지 없이 141(128+SIGPIPE)로 조용히 끝난다.
#[cfg(unix)]
#[test]
fn test_broken_pipe_exits_quietly() {
    use std::io::{BufRead, BufReader};
    let script = temp_bang("for i in range(1000000) { print(i) }\n");
    let mut child = Command::new(BANG)
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("실행 실패");
    {
        let stdout = child.stdout.take().unwrap();
        let mut first = String::new();
        BufReader::new(stdout).read_line(&mut first).unwrap();
        assert_eq!(first, "0\n");
    } // 여기서 파이프가 닫힘
    let out = child.wait_with_output().expect("대기 실패");
    assert_eq!(out.status.code(), Some(141));
    assert!(out.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

/// Phase 62: 스크립트 뒤의 인자는 스크립트의 것 — args()는 [스크립트, 인자...] (이전: 마지막 인자를 파일로 오인).
#[test]
fn test_script_args_passed_to_script() {
    let p = temp_bang("print(args())\nprint(len(args()))\n");
    let ps = p.to_str().unwrap();
    assert_eq!(stdout_of(&[ps, "x", "y z", "--flag"]), format!("[{ps}, x, y z, --flag]\n4\n"));
    assert_eq!(stdout_of(&["run", ps, "-v"]), format!("[{ps}, -v]\n2\n"));
    let _ = fs::remove_file(&p);
}

/// Phase 62: input()은 입력 끝(EOF)에서 nil — 빈 줄("")과 구별된다.
#[test]
fn test_input_returns_nil_at_eof() {
    let p = temp_bang("let lines = []\nwhile true {\n    let l = input()\n    if l == nil { break }\n    lines += [l]\n}\nprint(lines, len(lines))\n");
    let mut child = Command::new(BANG)
        .arg(&p)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn 실패");
    child.stdin.take().unwrap().write_all(b"a\n\nb\n").expect("stdin 쓰기 실패");
    let out = child.wait_with_output().expect("대기 실패");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "[a, , b] 3\n");
    let _ = fs::remove_file(&p);
}
