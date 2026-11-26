//! Phase 64: 권한 모드 (--sandbox / --allow-*) — 빌드된 bang 바이너리로 종단 검증.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// data/a.txt, secret.txt, out/ 이 있는 작업 디렉터리 (+ unix면 data/link → ../secret.txt).
fn workdir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("bang_perm_it_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("data")).unwrap();
    fs::create_dir_all(d.join("out")).unwrap();
    fs::write(d.join("data/a.txt"), "hi").unwrap();
    fs::write(d.join("secret.txt"), "secret").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("../secret.txt", d.join("data/link")).unwrap();
    d
}

/// 시도 목록을 실행해 줄마다 "OK 이름" 또는 "DENY 이름 kind"를 찍는 스크립트.
const PROBE: &str = r#"fn try_it(label, f) {
    try {
        f()
        print("OK", label)
    } catch e {
        print("DENY", label, e.kind)
    }
}
try_it("read-data", fn() { return read_file("data/a.txt") })
try_it("read-secret", fn() { return read_file("secret.txt") })
try_it("read-dotdot", fn() { return read_file("data/../secret.txt") })
try_it("exists-secret", fn() { return file_exists("secret.txt") })
try_it("write-out", fn() { return write_file("out/x.txt", "x") })
try_it("write-data", fn() { return write_file("data/x.txt", "x") })
try_it("move-out-to-data", fn() { return file_move("out/x.txt", "data/y.txt") })
try_it("env-home", fn() { return env_get("HOME") })
try_it("env-secret", fn() { return env_get("BANG_TEST_SECRET") })
try_it("run-allowed", fn() { return proc_run(["bang", "--version"]) })
try_it("run-sh", fn() { return proc_run(["sh", "-c", "true"]) })
try_it("net-other", fn() { return http_get("https://example.com/") })
try_it("net-listen", fn() { return tcp_listen("127.0.0.1:0") })
try_it("spawn-read", fn() { return wait(spawn read_file("secret.txt")) })
try_it("callback-read", fn() { return map(["secret.txt"], read_file) })
try_it("method-read", fn() { return "secret.txt".read_file() })
try_it("import-nonbang", fn() { return import("./secret.txt") })
"#;

fn run_in(dir: &Path, args: &[&str], src: &str) -> (i32, String, String) {
    fs::write(dir.join("probe.bang"), src).unwrap();
    // proc_run(["bang", ...])가 찾을 수 있게 테스트 바이너리 디렉터리를 PATH 앞에 둔다
    let bin_dir = Path::new(BANG).parent().unwrap();
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default());
    let out = Command::new(BANG)
        .args(args)
        .arg("probe.bang")
        .current_dir(dir)
        .env("PATH", path)
        .output()
        .expect("실행 실패");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn verdicts(out: &str) -> Vec<String> {
    out.lines().map(|l| l.to_string()).collect()
}

#[test]
#[cfg(unix)]
fn no_flags_allows_everything_as_before() {
    let d = workdir();
    let (code, out, err) = run_in(&d, &[], PROBE);
    assert_eq!(code, 0, "{err}");
    for l in verdicts(&out) {
        // 권한 에러는 하나도 없다 (네트워크·import는 각자의 io/import 에러일 수 있음)
        assert!(!l.ends_with("permission"), "{l}");
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
#[cfg(unix)]
fn sandbox_denies_everything() {
    let d = workdir();
    let (code, out, err) = run_in(&d, &["--sandbox"], PROBE);
    assert_eq!(code, 0, "{err}");
    for l in verdicts(&out) {
        assert!(l.starts_with("DENY") && l.ends_with("permission"), "권한 모드에서 거부되어야 함: {l}");
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
#[cfg(unix)]
fn selective_allow_lists() {
    let d = workdir();
    let (code, out, err) = run_in(
        &d,
        &["run", "--allow-read=data", "--allow-write=out", "--allow-env=HOME", "--allow-run=bang", "--allow-net=127.0.0.1"],
        PROBE,
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        verdicts(&out),
        [
            "OK read-data",
            "DENY read-secret permission",
            "DENY read-dotdot permission",
            "DENY exists-secret permission",
            "OK write-out",
            "DENY write-data permission",
            "DENY move-out-to-data permission",
            "OK env-home",
            "DENY env-secret permission",
            "OK run-allowed",
            "DENY run-sh permission",
            "DENY net-other permission",
            "OK net-listen",
            "DENY spawn-read permission",
            "DENY callback-read permission",
            "DENY method-read permission",
            "DENY import-nonbang permission",
        ]
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
#[cfg(unix)]
fn symlink_cannot_escape_allowed_dir() {
    let d = workdir();
    let src = "try { print(read_file(\"data/link\")) } catch e { print(e.kind) }\nprint(read_file(\"data/a.txt\"))\n";
    let (code, out, err) = run_in(&d, &["--allow-read=data"], src);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "permission\nhi\n");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn uncaught_permission_error_exits_1_with_hint() {
    let d = workdir();
    let (code, _, err) = run_in(&d, &["--sandbox"], "print(read_file(\"secret.txt\"))\n");
    assert_eq!(code, 1);
    assert!(err.contains("권한 없음: read_file()의 read 'secret.txt' — 허용하려면 --allow-read=secret.txt"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn bad_flag_is_rejected() {
    let d = workdir();
    let (code, _, err) = run_in(&d, &["--allow-disk"], "print(1)\n");
    assert_eq!(code, 1);
    assert!(err.contains("알 수 없는 권한 '--allow-disk'"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn bang_test_accepts_permission_flags() {
    let d = workdir();
    fs::write(d.join("sec_test.bang"), "fn test_reads() {\n    assert_eq(read_file(\"data/a.txt\"), \"hi\")\n}\n").unwrap();
    let run = |flags: &[&str]| {
        Command::new(BANG).arg("test").args(flags).arg("sec_test.bang").current_dir(&d).output().expect("실행 실패")
    };
    let denied = run(&["--sandbox"]);
    assert_eq!(denied.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&denied.stdout).contains("권한 없음"));
    let allowed = run(&["--allow-read=data"]);
    assert!(allowed.status.success(), "{}", String::from_utf8_lossy(&allowed.stdout));
    let _ = fs::remove_dir_all(&d);
}
