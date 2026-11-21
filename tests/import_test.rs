//! import 모듈 시스템 통합 테스트 (VM 실행 경로).
//!
//! 회귀 방지: 이전엔 import한 모듈의 함수가 같은 모듈의 다른 전역
//! (형제 함수·모듈 상수)을 참조하면 VM이 패닉(index out of bounds)했다.
//! 클로저가 자기 모듈 전역 Arc를 들고 다니도록 고쳐 해결했다.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BANG: &str = env!("CARGO_BIN_EXE_bang");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 고유 임시 디렉토리에 (모듈, 메인) 파일을 쓰고 메인을 실행해 stdout 반환.
fn run_with_module(module_src: &str, main_src: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bang_import_{}_{n}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let module_path = dir.join("module.bang");
    let main_path = dir.join("main.bang");
    fs::write(&module_path, module_src).expect("모듈 쓰기 실패");
    // 메인은 상대경로 import를 쓰므로 cwd를 dir로 설정해 실행.
    fs::write(&main_path, main_src).expect("메인 쓰기 실패");

    let out = Command::new(BANG)
        .arg("main.bang")
        .current_dir(&dir)
        .output()
        .expect("실행 실패");

    let _ = fs::remove_file(&module_path);
    let _ = fs::remove_file(&main_path);
    let _ = fs::remove_dir(&dir);

    assert!(
        out.status.success(),
        "비정상 종료: {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout UTF-8 아님")
}

/// 패키지: 바레 이름 import가 bang_modules/<name>/<name>.bang 으로 해석된다.
#[test]
fn test_named_module_resolution() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bang_pkg_{}_{n}", std::process::id()));
    let modir = dir.join("bang_modules").join("mathutils");
    let _ = fs::create_dir_all(&modir);
    fs::write(modir.join("mathutils.bang"),
        "let pi = 3.14159\nfn square(x) { return x * x }\n").expect("모듈 쓰기");
    fs::write(dir.join("app.bang"),
        "let m = import(\"mathutils\")\nprint(m.square(5))\n").expect("앱 쓰기");

    let out = Command::new(BANG)
        .arg("app.bang")
        .current_dir(&dir)
        .output()
        .expect("실행 실패");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = fs::remove_dir_all(&dir);

    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(stdout, "25\n");
}

/// 회귀: 모듈 함수가 형제 함수와 모듈 상수를 참조 (이전엔 패닉).
#[test]
fn test_module_function_references_siblings() {
    let module = "\
let pi = 3.0
fn square(x) { return x * x }
fn area(r) { return pi * square(r) }
";
    let main = "\
let m = import(\"module.bang\")
print(m.square(5))
print(m.area(2))
";
    // square(5)=25, area(2)=pi*square(2)=3.0*4=12
    let out = run_with_module(module, main);
    assert_eq!(out, "25\n12.0\n");
}

/// 모듈 상수 직접 접근.
#[test]
fn test_module_constant_access() {
    let module = "let answer = 42\n";
    let main = "let m = import(\"module.bang\")\nprint(m.answer)\n";
    assert_eq!(run_with_module(module, main), "42\n");
}

/// 대괄호 인덱싱으로도 export 접근 가능.
#[test]
fn test_bracket_access() {
    let module = "fn greet() { return \"hi\" }\n";
    let main = "let m = import(\"module.bang\")\nprint(m[\"greet\"]())\n";
    assert_eq!(run_with_module(module, main), "hi\n");
}

/// 순환 import(a→b→a)가 무한 재귀(스택 오버플로 abort) 대신 에러로 종료된다.
#[test]
fn test_circular_import_errors_not_crash() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bang_cyc_{}_{n}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("a.bang"), "let b = import(\"b.bang\")\nlet va = 1\n").unwrap();
    fs::write(dir.join("b.bang"), "let a = import(\"a.bang\")\nlet vb = 2\n").unwrap();
    fs::write(dir.join("main.bang"), "let a = import(\"a.bang\")\nprint(\"no\")\n").unwrap();

    let out = Command::new(BANG)
        .arg("main.bang")
        .current_dir(&dir)
        .output()
        .expect("실행 실패");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = fs::remove_dir_all(&dir);

    assert!(!out.status.success(), "순환 import는 에러 종료여야 함");
    assert!(stderr.contains("순환 import"), "순환 감지 메시지 기대: {stderr}");
}

/// 패키지 캐싱: 같은 모듈을 두 번 import해도 top-level 코드는 한 번만 실행된다.
#[test]
fn test_module_imported_once() {
    let module = "print(\"init\")\nlet value = 7\n";
    let main = "\
let a = import(\"module.bang\")
let b = import(\"module.bang\")
print(a.value + b.value)
";
    // "init"은 한 번만, 합계 14
    assert_eq!(run_with_module(module, main), "init\n14\n");
}

/// 값 의미론: 모듈이 export한 리스트를 메인에서 바꿔도 모듈 내부엔 영향 없음.
/// (모듈 함수가 자기 전역 리스트를 그대로 반환하는지 확인)
#[test]
fn test_module_function_uses_module_global_list() {
    let module = "\
let base = [1, 2, 3]
fn get() { return base }
fn total() { return sum(base) }
";
    let main = "\
let m = import(\"module.bang\")
print(m.get())
print(m.total())
";
    assert_eq!(run_with_module(module, main), "[1, 2, 3]\n6\n");
}

/// Phase 56: import한 모듈 안에서 난 런타임 에러는 그 모듈 파일 이름과 그 파일의 줄을 보여 준다.
/// (이전: 메인 파일의 같은 줄 번호를 보여 줘서 엉뚱한 줄(빈 줄)을 가리켰다)
#[test]
fn test_runtime_error_in_module_shows_module_file_and_line() {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("bang_import_err_{}_{n}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("mathx.bang"), "// 모듈\n\n\nfn risky(x) {\n    return 10 / x\n}\n").unwrap();
    fs::write(dir.join("main.bang"), "let m = import(\"mathx\")\nprint(m.risky(0))\n").unwrap();
    let out = Command::new(BANG).arg("main.bang").current_dir(&dir).output().expect("실행 실패");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("mathx.bang 5:12"), "{stderr}");
    assert!(stderr.contains("return 10 / x"), "모듈의 소스 줄을 보여야 함: {stderr}");
}
