// Bang 통합 테스트 — 인터프리터 (Phase 3)

use bang::interpreter::Interpreter;
use bang::lexer::Lexer;
use bang::parser::Parser;
use std::fs;
use std::path::Path;

/// 소스 문자열을 실행하고 출력 줄 목록 반환
fn run_source(source: &str) -> Vec<String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("tokenize 실패");
    let prog = Parser::new(tokens).parse().expect("parse 실패");
    let interp = Interpreter::new();
    interp.run(&prog).expect("run 실패");
    let out = interp.output.lock().unwrap().clone();
    out
}

/// examples/ 파일 실행
fn run_file(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name}: 파일 읽기 실패: {e}"));
    run_source(&source)
}

// =============================================================================
// 기본 예제
// =============================================================================

#[test]
fn test_hello() {
    assert_eq!(run_file("hello.bang"), vec!["hello world"]);
}

#[test]
fn test_fibonacci() {
    assert_eq!(
        run_file("fibonacci.bang"),
        vec!["0", "1", "1", "2", "3", "5", "8", "13", "21", "34"],
    );
}

#[test]
fn test_recursion() {
    assert_eq!(
        run_file("recursion.bang"),
        vec![
            "120", "1024",
            "A -> C", "A -> B", "C -> B",
            "A -> C",
            "B -> A", "B -> C", "A -> C",
        ],
    );
}

#[test]
fn test_closures() {
    assert_eq!(
        run_file("closures.bang"),
        vec!["15", "20", "1", "2", "3"],
    );
}

#[test]
fn test_list_traversal() {
    assert_eq!(
        run_file("list_traversal.bang"),
        vec!["apple", "banana", "cherry", "total: 15", "doubled: [2, 4, 6, 8, 10]"],
    );
}

#[test]
fn test_map_usage() {
    assert_eq!(
        run_file("map_usage.bang"),
        vec!["Alice", "30", "name: Alice", "age: 30", "city: Seoul"],
    );
}

#[test]
fn test_conditionals() {
    assert_eq!(
        run_file("conditionals.bang"),
        vec!["positive", "exactly ten"],
    );
}

#[test]
fn test_string_ops() {
    assert_eq!(
        run_file("string_ops.bang"),
        vec!["hello world", "11", "***", "H", "true", "false"],
    );
}

#[test]
fn test_higher_order() {
    assert_eq!(
        run_file("higher_order.bang"),
        vec!["10", "16", "21"],
    );
}

// =============================================================================
// 동시성 예제
// =============================================================================

#[test]
fn test_spawn_basic() {
    assert_eq!(
        run_file("spawn_basic.bang"),
        vec!["30", "70", "100"],
    );
}

#[test]
fn test_parallel_block() {
    assert_eq!(
        run_file("parallel_block.bang"),
        vec!["2", "4"],
    );
}

#[test]
fn test_channels() {
    assert_eq!(run_file("channels.bang"), vec!["42"]);
}

// =============================================================================
// 단위 테스트 — 인터프리터 의미 검증
// =============================================================================

#[test]
fn test_arithmetic() {
    assert_eq!(run_source("print(1 + 2)"), vec!["3"]);
    assert_eq!(run_source("print(10 - 3)"), vec!["7"]);
    assert_eq!(run_source("print(4 * 5)"), vec!["20"]);
    assert_eq!(run_source("print(10 / 2)"), vec!["5"]);
    assert_eq!(run_source("print(7 / 2)"), vec!["3.5"]);
    assert_eq!(run_source("print(10 % 3)"), vec!["1"]);
}

#[test]
fn test_value_semantics_list() {
    // 리스트는 값 의미론 — 대입 후 원본 수정 불가
    let out = run_source(
        "let a = [1, 2, 3]\nlet b = a\nb = b + [4]\nprint(len(a))\nprint(len(b))"
    );
    assert_eq!(out, vec!["3", "4"]);
}

#[test]
fn test_auto_wait_binary() {
    // spawn 반환값이 이항 연산에서 자동 해소되어야 함
    let out = run_source(
        "fn id(x) { return x }\nlet f = spawn id(42)\nprint(f + 0)"
    );
    assert_eq!(out, vec!["42"]);
}

#[test]
fn test_auto_wait_condition() {
    // spawn 반환값이 조건에서 자동 해소되어야 함
    let out = run_source(
        "fn yes() { return true }\nlet f = spawn yes()\nif f { print(\"ok\") }"
    );
    assert_eq!(out, vec!["ok"]);
}

#[test]
fn test_channel_basic() {
    let out = run_source(
        "let ch = channel()\nsend(ch, 100)\nclose(ch)\nprint(recv(ch))"
    );
    assert_eq!(out, vec!["100"]);
}

#[test]
fn test_channel_for_in() {
    let out = run_source(
        "let ch = channel()\nsend(ch, 1)\nsend(ch, 2)\nsend(ch, 3)\nclose(ch)\nfor v in ch {\n    print(v)\n}"
    );
    assert_eq!(out, vec!["1", "2", "3"]);
}

#[test]
fn test_parallel_map() {
    let out = run_source(
        "fn double(x) { return x * 2 }\nlet result = parallel_map([1, 2, 3], double)\nprint(result)"
    );
    assert_eq!(out, vec!["[2, 4, 6]"]);
}

#[test]
fn test_wait_builtin() {
    let out = run_source(
        "fn add(a, b) { return a + b }\nlet f = spawn add(3, 4)\nlet v = wait(f)\nprint(v)"
    );
    assert_eq!(out, vec!["7"]);
}

#[test]
fn test_nested_closures() {
    let out = run_source(
        "let make = fn(n) { return fn(x) { return x * n } }\nlet triple = make(3)\nprint(triple(7))"
    );
    assert_eq!(out, vec!["21"]);
}

#[test]
fn test_break_continue() {
    let out = run_source(
        "let i = 0\nwhile i < 10 {\n    if i == 3 { break }\n    print(i)\n    i = i + 1\n}"
    );
    assert_eq!(out, vec!["0", "1", "2"]);
}

#[test]
fn test_str_builtin() {
    assert_eq!(run_source("print(str(42))"), vec!["42"]);
    assert_eq!(run_source("print(str(3.14))"), vec!["3.14"]);
    assert_eq!(run_source("print(str(true))"), vec!["true"]);
}

#[test]
fn test_len_builtin() {
    assert_eq!(run_source("print(len([1, 2, 3]))"), vec!["3"]);
    assert_eq!(run_source("print(len(\"hello\"))"), vec!["5"]);
}

#[test]
fn test_map_field_access() {
    let out = run_source(
        "let m = {\"x\": 1, \"y\": 2}\nprint(m[\"x\"])\nprint(m[\"y\"])"
    );
    assert_eq!(out, vec!["1", "2"]);
}

#[test]
fn test_string_index() {
    assert_eq!(run_source("print(\"hello\"[0])"), vec!["h"]);
    assert_eq!(run_source("print(\"hello\"[4])"), vec!["o"]);
}
