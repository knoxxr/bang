//! 시스템 빌트인 통합 테스트 (Phase 42, 109-116):
//! proc_run / env_get / env_cwd / sleep / file_remove / file_move / dir_create / file_append
//! + 채널로 참조 타입 송신 시 에러 (조용한 Nil 손실 수정).
//!
//! 외부 명령은 세 OS에 공통으로 존재하는 "빌드된 bang 바이너리 자신"을 실행한다
//! (echo 등은 Windows에서 실행 파일이 아니라 cmd 내장이라 쓰지 않는다).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("bang_sys_{}_{n}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// bang 소스 문자열 리터럴에 박을 경로 — 백슬래시가 이스케이프로 해석되지 않게 '/'로.
fn path_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 스크립트를 stdin으로 실행 → (성공 여부, stdout, stderr)
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

// ── proc_run ────────────────────────────────────────────────────────────────

#[test]
fn test_proc_run_success_captures_stdout() {
    let bang = path_str(Path::new(BANG));
    let out = run_ok(&format!(
        "let r = proc_run([\"{bang}\", \"--version\"])\n\
         print(r.code)\n\
         print(contains(r.stdout, \"0.\"))\n\
         print(type(r.stderr))"
    ));
    assert_eq!(out, "0\ntrue\nStr\n");
}

#[test]
fn test_proc_run_nonzero_exit_is_not_error() {
    let dir = tmp_dir("exit");
    std::fs::create_dir_all(&dir).unwrap();
    let child = dir.join("child.bang");
    std::fs::write(&child, "print_err(\"oops\")\nexit(3)\n").unwrap();
    let out = run_ok(&format!(
        "let r = proc_run([\"{}\", \"{}\"])\nprint(r.code)\nprint(trim(r.stderr))",
        path_str(Path::new(BANG)),
        path_str(&child)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out, "3\noops\n");
}

#[test]
fn test_proc_run_errors_are_catchable() {
    let out = run_ok(
        "try { proc_run([\"bang_no_such_command_xyz\"]) } catch e { print(\"missing\") }\n\
         try { proc_run([]) } catch e { print(\"empty\") }\n\
         try { proc_run([\"x\", 1]) } catch e { print(\"nonstr\") }\n\
         try { proc_run(\"ls\") } catch e { print(\"notlist\") }",
    );
    assert_eq!(out, "missing\nempty\nnonstr\nnotlist\n");
}

// ── env ─────────────────────────────────────────────────────────────────────

#[test]
fn test_env_get_and_cwd() {
    let dir = tmp_dir("env");
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new(BANG)
        .arg("-")
        .current_dir(&dir)
        .env("BANG_TEST_ENV_VAR", "hello")
        .env_remove("BANG_TEST_ENV_MISSING")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"print(env_get(\"BANG_TEST_ENV_VAR\"))\n\
              print(env_get(\"BANG_TEST_ENV_MISSING\"))\n\
              print(env_cwd())",
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "hello");
    assert_eq!(lines[1], "nil");
    let cwd = std::fs::canonicalize(lines[2]).unwrap();
    assert_eq!(cwd, std::fs::canonicalize(&dir).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

// ── sleep ───────────────────────────────────────────────────────────────────

#[test]
fn test_sleep_negative_errors() {
    let out = run_ok("try { sleep(-1) } catch e { print(\"neg\") }\nsleep(0)\nprint(\"ok\")");
    assert_eq!(out, "neg\nok\n");
}

/// spawn된 작업 32개가 각각 300ms sleep — 순차라면 ~9.6s.
/// 탄력 풀이 블로킹 작업마다 워커를 늘려 동시에 대기하므로 수 초 안에 끝나야 한다.
#[test]
fn test_sleep_in_spawn_does_not_block_other_tasks() {
    let start = Instant::now();
    let out = run_ok(
        // 인덱스 대입 우변은 자동 조인 지점이 아니므로 Future를 리스트에 모을 수 있다.
        // (parallel_map은 현재 VM에서 순차 실행이라 이 검증에 쓰지 않는다.)
        "let nap = fn(i) { sleep(300)\n return i }\n\
         let fs = range(32)\n\
         for i in range(32) { fs[i] = spawn nap(i) }\n\
         let total = 0\n\
         for i in range(32) { total = total + fs[i] }\n\
         print(total)",
    );
    let elapsed = start.elapsed();
    assert_eq!(out, "496\n");
    assert!(
        elapsed < Duration::from_secs(4),
        "sleep이 다른 작업을 막음: {elapsed:?} (순차 ≈ 9.6s)"
    );
}

// ── 파일 조작 ───────────────────────────────────────────────────────────────

#[test]
fn test_file_ops_roundtrip() {
    let dir = tmp_dir("fs");
    let nested = path_str(&dir.join("a").join("b"));
    let out = run_ok(&format!(
        "let d = \"{nested}\"\n\
         dir_create(d)\n\
         dir_create(d)\n\
         print(is_dir(d))\n\
         file_append(d + \"/x.txt\", \"hello \")\n\
         file_append(d + \"/x.txt\", \"world\")\n\
         print(read_file(d + \"/x.txt\"))\n\
         file_move(d + \"/x.txt\", d + \"/y.txt\")\n\
         print(file_exists(d + \"/x.txt\"), file_exists(d + \"/y.txt\"))\n\
         file_remove(d + \"/y.txt\")\n\
         print(file_exists(d + \"/y.txt\"))"
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out, "true\nhello world\nfalse true\nfalse\n");
}

#[test]
fn test_file_ops_errors_are_catchable() {
    let dir = tmp_dir("fserr");
    std::fs::create_dir_all(&dir).unwrap();
    let d = path_str(&dir);
    std::fs::write(dir.join("plain.txt"), "x").unwrap();
    let out = run_ok(&format!(
        "let d = \"{d}\"\n\
         try {{ file_remove(d + \"/nope.txt\") }} catch e {{ print(\"rm-missing\") }}\n\
         try {{ file_remove(d) }} catch e {{ print(\"rm-dir\") }}\n\
         try {{ file_move(d + \"/nope.txt\", d + \"/z.txt\") }} catch e {{ print(\"mv-missing\") }}\n\
         try {{ dir_create(d + \"/plain.txt\") }} catch e {{ print(\"mkdir-over-file\") }}\n\
         try {{ file_append(d + \"/no/such/dir/f.txt\", \"x\") }} catch e {{ print(\"append-bad\") }}"
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out,
        "rm-missing\nrm-dir\nmv-missing\nmkdir-over-file\nappend-bad\n"
    );
}

// ── 채널: 참조 타입 송신은 에러 (이전: 조용히 Nil) ──────────────────────────

#[test]
fn test_channel_rejects_reference_types() {
    let out = run_ok(
        "let ch = channel(4)\n\
         try { send(ch, fn() { return 1 }) } catch e { print(\"fn\") }\n\
         try { send(ch, [1, fn() { return 2 }]) } catch e { print(\"nested-list\") }\n\
         try { send(ch, {\"c\": ch}) } catch e { print(\"nested-map\") }\n\
         send(ch, [1, {\"k\": \"v\"}])\n\
         print(recv(ch))",
    );
    assert_eq!(out, "fn\nnested-list\nnested-map\n[1, {k: v}]\n");
}

#[test]
fn test_channel_reference_error_message() {
    let (ok, _out, err) = run("let ch = channel(1)\nsend(ch, fn() { return 1 })");
    assert!(!ok);
    assert!(err.contains("채널로 보낼 수 없음"), "stderr: {err}");
}
