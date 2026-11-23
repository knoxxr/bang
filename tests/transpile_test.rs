//! Phase 10 AOT 트랜스파일러 통합 테스트.
//!
//! 단위 테스트(src/codegen/transpile.rs)는 생성된 C 소스의 부분 문자열만
//! 검사하므로, 세미콜론 누락 같은 "문법적으로 깨진 C" 회귀를 잡지 못한다.
//! 이 테스트는 Bang → C 변환 결과를 실제 `cc -O2`로 컴파일하고 실행해
//! 표준 출력까지 확인한다.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use bang::codegen::transpile;
use bang::lexer::Lexer;
use bang::parser::Parser;

/// 테스트별 고유 임시 경로 생성용 카운터 (병렬 실행 충돌 방지).
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn unique_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("bang_transpile_{tag}_{pid}_{n}"))
}

/// Bang 소스를 C로 변환 → `cc -O2`로 컴파일 → 실행 → stdout 반환.
///
/// `cc`가 PATH에 없으면 테스트를 조용히 통과시킨다(환경 의존 방지).
fn compile_and_run(src: &str, tag: &str) -> Option<String> {
    let tokens = Lexer::new(src).tokenize().expect("tokenize 실패");
    let prog = Parser::new(tokens).parse().expect("parse 실패");
    let c_code = transpile(&prog).expect("transpile 실패");

    let bin_path = unique_path(tag);
    let c_path = bin_path.with_extension("c");
    fs::write(&c_path, &c_code).expect("C 소스 쓰기 실패");

    let status = Command::new("cc")
        .args(["-O2", "-lm", "-o"])
        .arg(&bin_path)
        .arg(&c_path)
        .status();

    let status = match status {
        Ok(s) => s,
        Err(_) => {
            // cc 미설치 환경 → 테스트 스킵. 조용히 통과로 보이지 않게 이유를 남긴다
            // (`cargo test -- --nocapture`로 확인. CI의 ubuntu/macos에는 cc가 있어 실제로 실행된다).
            eprintln!("SKIP transpile_test[{tag}]: cc를 찾을 수 없어 AOT 컴파일·실행 검증을 건너뜀");
            let _ = fs::remove_file(&c_path);
            return None;
        }
    };
    assert!(
        status.success(),
        "cc 컴파일 실패 — 생성된 C가 유효하지 않음:\n{c_code}"
    );

    let output = Command::new(&bin_path).output().expect("실행 실패");
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&bin_path);

    assert!(
        output.status.success(),
        "네이티브 바이너리 비정상 종료: {:?}",
        output.status.code()
    );
    // Windows C 런타임은 텍스트 모드 stdout에서 \n을 \r\n으로 출력 → 비교 전 정규화
    let s = String::from_utf8(output.stdout).expect("stdout가 UTF-8이 아님");
    Some(s.replace("\r\n", "\n"))
}

/// 회귀 테스트: print() 문장이 세미콜론을 포함한 유효한 C를 생성해야 한다.
/// (이전 버그: `bv_print(...)` 뒤 세미콜론 누락으로 cc 컴파일 실패)
#[test]
fn test_print_compiles_and_runs() {
    if let Some(out) = compile_and_run(r#"print("hello world")"#, "print") {
        assert_eq!(out, "hello world\n");
    }
}

#[test]
fn test_arithmetic_output() {
    if let Some(out) = compile_and_run("print(1 + 2 * 3)", "arith") {
        assert_eq!(out, "7\n");
    }
}

#[test]
fn test_function_call_output() {
    let src = "fn add(a, b) { return a + b }\nprint(add(40, 2))";
    if let Some(out) = compile_and_run(src, "fn") {
        assert_eq!(out, "42\n");
    }
}

/// while 루프 안의 print() — 이전 세미콜론 버그가 가장 먼저 터진 형태.
#[test]
fn test_while_loop_with_print() {
    let src = "let i = 0\nwhile i < 5 {\n    print(i)\n    i = i + 1\n}";
    if let Some(out) = compile_and_run(src, "while") {
        assert_eq!(out, "0\n1\n2\n3\n4\n");
    }
}

/// fibonacci 예제 전체 — 재귀 함수 + while + print 통합.
#[test]
fn test_fibonacci_example() {
    let src = fs::read_to_string("examples/fibonacci.bang").expect("예제 읽기 실패");
    if let Some(out) = compile_and_run(&src, "fib") {
        assert_eq!(out, "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n");
    }
}

/// print() 다중 인자 — 공백 구분, 블록 형태 emit 검증.
#[test]
fn test_print_multiple_args() {
    if let Some(out) = compile_and_run(r#"print(1, 2, 3)"#, "multiarg") {
        assert_eq!(out, "1 2 3\n");
    }
}

/// 인자 없는 print() — 빈 줄 출력.
#[test]
fn test_print_empty() {
    if let Some(out) = compile_and_run("print()", "empty") {
        assert_eq!(out, "\n");
    }
}

/// if/else 분기 + 비교 연산.
#[test]
fn test_if_else_branch() {
    let src = "let x = 10\nif x > 5 {\n    print(\"big\")\n} else {\n    print(\"small\")\n}";
    if let Some(out) = compile_and_run(src, "ifelse") {
        assert_eq!(out, "big\n");
    }
}

// ============================================================================
// Phase 58 — VM ↔ AOT 동치 (헌법 4조 "의미는 계약"): 같은 프로그램은 같은 stdout·종료 코드
// ============================================================================

/// 실행 결과: (stdout, 종료 코드, stderr의 런타임 오류 부분 — 경고 줄 제외)
type RunOut = (String, i32, String);

/// stderr에서 "런타임 오류"로 시작하는 블록(에러 줄 + 소스 문맥)만 남긴다 (VM의 리졸버 경고는 컴파일 시점 출력이라 제외).
fn runtime_error_block(stderr: &str) -> String {
    match stderr.find("런타임 오류") {
        Some(p) => {
            let start = stderr[..p].rfind('\n').map(|i| i + 1).unwrap_or(0);
            stderr[start..].trim_end().to_string()
        }
        None => String::new(),
    }
}

/// AOT 바이너리의 실행 결과. cc가 없으면 None.
fn aot_output(src: &str, tag: &str) -> Option<RunOut> {
    let tokens = Lexer::new(src).tokenize().expect("tokenize 실패");
    let prog = Parser::new(tokens).parse().expect("parse 실패");
    let c_code = bang::codegen::transpile_with_source(&prog, Some(src))
        .unwrap_or_else(|e| panic!("transpile 실패: {e:?}\n{src}"));
    let bin_path = unique_path(tag);
    let c_path = bin_path.with_extension("c");
    fs::write(&c_path, &c_code).expect("C 소스 쓰기 실패");
    let status = Command::new("cc").args(["-O2", "-o"]).arg(&bin_path).arg(&c_path).arg("-lm").status();
    let status = match status {
        Ok(s) => s,
        Err(_) => {
            eprintln!("SKIP aot_matches_vm[{tag}]: cc 없음");
            let _ = fs::remove_file(&c_path);
            return None;
        }
    };
    assert!(status.success(), "cc 컴파일 실패:\n{c_code}");
    let out = Command::new(&bin_path).output().expect("실행 실패");
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&bin_path);
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
        runtime_error_block(&String::from_utf8_lossy(&out.stderr)),
    ))
}

/// VM(`bang run`)의 실행 결과.
fn vm_output(src: &str, tag: &str) -> RunOut {
    let path = unique_path(tag).with_extension("bang");
    fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bang")).arg("run").arg(&path).output().expect("실행 실패");
    let _ = fs::remove_file(&path);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
        runtime_error_block(&String::from_utf8_lossy(&out.stderr)),
    )
}

/// AOT 지원 부분집합의 프로그램 모음. 각각 VM과 AOT의 stdout·종료 코드가 같아야 한다.
const AOT_VM_CORPUS: &[(&str, &str)] = &[
    ("arith", "print(7 + 2 * 3, 7 - 10, 6 * -7, 7 % 3, -7 % 2, 7.5 % 2)"),
    ("division", "print(7 / 2, 8 / 2, 1 / 3, 10 / 4, 1.0 / 0, -1.0 / 0, 0.0 / 1)"),
    ("float_display", "print(0.1, 0.1 + 0.2, 1e21, 1e-7, 2.5e-3, 1e15, 1e16, 123.456, -0.5, 3.0)"),
    ("str_of_float", "print(str(2.0) + \"|\" + str(0.1) + \"|\" + str(1 / 3))"),
    ("truthiness", "fn t(v) {\n if v { return 1 }\n return 0\n}\nprint(t(0), t(1), t(\"\"), t(\"a\"), t(nil), t(0.0), t(false), t(true))\nprint(not 0, not \"\", 0 or \"기본\", 1 and 2, bool(0), bool(\"x\"))"),
    ("while_truthy", "let n = 3\nlet s = 0\nwhile n {\n s = s + n\n n = n - 1\n}\nprint(s)"),
    ("compare", "print(1 < 2, 2 <= 2, 3 > 4, \"a\" < \"b\", \"가\" > \"a\", 1 == 1.0, 1 == true, \"x\" == \"x\", nil == nil)"),
    ("builtins", "print(floor(2.7), ceil(2.1), round(2.5), round(-2.5), pow(2, 10), pow(2.0, 3), pow(2, -1), sqrt(16), abs(-3), abs(-2.5), min(3, 1), max(1, 2.5))"),
    ("casts", "print(int(\"42\"), int(\" 7 \"), int(3.9), int(-3.9), int(true), float(\"1.5\"), float(2), str(12), str(true), str(nil))"),
    ("len_utf8", "print(len(\"héllo한\"), len(\"\"), len(\"abc\"))"),
    ("strings", "let s = \"안녕\" + \", \" + \"bang\"\nprint(s, len(s))"),
    ("recursion", "fn fib(n) {\n if n <= 1 { return n }\n return fib(n - 1) + fib(n - 2)\n}\nprint(fib(20))"),
    ("compound", "let x = 10\nx += 5\nx -= 1\nx *= 2\nx /= 4\nprint(x)"),
    ("overflow_add", "print(\"before\")\nlet m = 9223372036854775807\nprint(m + 1)\nprint(\"after\")"),
    ("overflow_mul_neg", "let m = -9223372036854775807 - 1\nprint(m * 1)\nprint(-m)"),
    ("overflow_abs", "let m = -9223372036854775807 - 1\nprint(abs(m))"),
    ("zero_div", "print(1)\nprint(1 / 0)"),
    ("zero_mod", "print(5 % 0)"),
    ("bad_int", "print(int(\"abc\"))"),
    ("bad_float", "print(float(\"1.5x\"))"),
    ("bad_add", "print(1 + \"a\")"),
    ("assert_msg", "print(\"a\")\nassert(1 > 2, \"틀림\")\nprint(\"b\")"),
    ("exit_code", "print(\"x\")\nexit(3)"),
    // Phase 61: 에러 메시지·위치·소스 문맥까지 같아야 한다
    ("err_types", "print(1 + \"a\")"),
    ("err_float_str", "print(2.5 * \"x\")"),
    ("err_compare", "let a = 1\nprint(a < \"b\")"),
    ("err_neg", "print(-\"a\")"),
    ("err_builtin_arg", "print(sqrt(\"a\"))"),
    ("err_len", "print(len(5))"),
    ("err_abs", "print(abs(\"x\"))"),
    ("err_int_nil", "print(int(nil))"),
    ("err_assert_plain", "print(1)\n  assert(1 > 2)"),
    ("err_exit_type", "exit(\"x\")"),
    ("err_in_function", "fn f(x) {\n    return 10 / x\n}\nprint(f(2))\nprint(f(0))"),
    ("err_compound", "let x = 9223372036854775807\nx += 1"),
    ("err_utf8_line", "let 이름 = \"방\"\nprint(이름 - 1)"),
];

#[test]
fn aot_matches_vm_on_corpus() {
    let mut mismatches = Vec::new();
    for (tag, src) in AOT_VM_CORPUS {
        let vm = vm_output(src, tag);
        let Some(aot) = aot_output(src, tag) else { return };
        if vm != aot {
            mismatches.push(format!("[{tag}]\n  VM : {vm:?}\n  AOT: {aot:?}"));
        }
    }
    assert!(mismatches.is_empty(), "VM과 AOT 결과가 다름 ({}건):\n{}", mismatches.len(), mismatches.join("\n"));
}
