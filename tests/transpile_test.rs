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
            // cc 미설치 환경 → 테스트 스킵
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
    Some(String::from_utf8(output.stdout).expect("stdout가 UTF-8이 아님"))
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
