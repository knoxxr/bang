// Bang 통합 테스트 — Resolver (Phase 4)

use bang::lexer::Lexer;
use bang::parser::Parser;
use bang::resolver::{self, ResolveResult};
use std::fs;
use std::path::Path;

/// 소스를 파싱 후 resolve 실행
fn resolve_source(source: &str) -> ResolveResult {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("tokenize 실패");
    let prog = Parser::new(tokens).parse().expect("parse 실패");
    resolver::resolve(&prog)
}

/// 소스가 오류 없이 resolve되는지 확인
fn expect_ok(source: &str) {
    let r = resolve_source(source);
    if !r.errors.is_empty() {
        for e in &r.errors {
            eprintln!("{e}");
        }
        panic!("resolve 오류가 없어야 하는데 발생: {} 개", r.errors.len());
    }
}

/// 소스에 특정 메시지를 포함하는 오류가 발생하는지 확인
fn expect_error(source: &str, msg_contains: &str) {
    let r = resolve_source(source);
    let found = r.errors.iter().any(|e| e.message.contains(msg_contains));
    if !found {
        let got: Vec<_> = r.errors.iter().map(|e| &e.message).collect();
        panic!(
            "'{msg_contains}' 를 포함하는 오류를 기대했으나 없음.\n실제 오류: {got:?}"
        );
    }
}

/// 소스에 특정 메시지를 포함하는 경고가 발생하는지 확인
fn expect_warning(source: &str, msg_contains: &str) {
    let r = resolve_source(source);
    let found = r.warnings.iter().any(|w| w.message.contains(msg_contains));
    if !found {
        let got: Vec<_> = r.warnings.iter().map(|w| &w.message).collect();
        panic!(
            "'{msg_contains}' 를 포함하는 경고를 기대했으나 없음.\n실제 경고: {got:?}"
        );
    }
}

fn resolve_file(name: &str) -> ResolveResult {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name}: 파일 읽기 실패: {e}"));
    resolve_source(&source)
}

fn expect_file_ok(name: &str) {
    let r = resolve_file(name);
    if !r.errors.is_empty() {
        for e in &r.errors {
            eprintln!("[{name}] {e}");
        }
        panic!("{name}: resolve 오류 {} 개", r.errors.len());
    }
}

// =============================================================================
// 기본 OK 케이스
// =============================================================================

#[test]
fn test_simple_let() {
    expect_ok("let x = 1\nprint(x)");
}

#[test]
fn test_shadowing_in_nested_scope() {
    // 중첩 스코프에서 같은 이름 let 허용
    expect_ok("let x = 1\nif true {\n    let x = 2\n    print(x)\n}\nprint(x)");
}

#[test]
fn test_function_recursion() {
    expect_ok("fn fib(n) {\n    if n < 2 { return n }\n    return fib(n - 1) + fib(n - 2)\n}\nprint(fib(10))");
}

#[test]
fn test_closure_captures() {
    expect_ok(
        "fn make(n) {\n    return fn(x) { return x + n }\n}\nlet add5 = make(5)\nprint(add5(3))",
    );
}

#[test]
fn test_for_loop_var() {
    expect_ok("let lst = [1, 2, 3]\nfor item in lst {\n    print(item)\n}");
}

#[test]
fn test_while_loop() {
    expect_ok("let i = 0\nwhile i < 5 {\n    i = i + 1\n}");
}

#[test]
fn test_parallel_block() {
    expect_ok("let result = 0\nfn work() { return 42 }\nparallel {\n    let r = spawn work()\n    result = r\n}");
}

// =============================================================================
// 오류 케이스
// =============================================================================

#[test]
fn test_undefined_variable() {
    expect_error("print(x)", "정의되지 않은 변수: 'x'");
}

#[test]
fn test_self_reference_in_let() {
    // let x = x 는 초기화 전 자기 참조
    expect_error("let x = x", "초기화 전에 참조");
}

#[test]
fn test_duplicate_let_same_scope() {
    expect_error("let x = 1\nlet x = 2", "이미 선언됨");
}

#[test]
fn test_return_outside_function() {
    expect_error("return 5", "함수 외부에서 return");
}

#[test]
fn test_non_function_literal_call() {
    expect_error("let v = 5(3)", "함수가 아닌 값을 호출");
}

#[test]
fn test_arity_mismatch_too_many() {
    expect_error(
        "fn add(a, b) { return a + b }\nadd(1, 2, 3)",
        "인자 수 불일치",
    );
}

#[test]
fn test_arity_mismatch_too_few() {
    expect_error(
        "fn add(a, b) { return a + b }\nadd(1)",
        "인자 수 불일치",
    );
}

#[test]
fn test_arity_correct() {
    expect_ok("fn add(a, b) { return a + b }\nadd(1, 2)");
}

#[test]
fn test_spawn_direct_assign_outer() {
    // spawn 직계 식에서 바깥 변수에 대입 → 오류
    expect_error(
        "let x = 0\nlet _ = spawn (x = 1)\n",
        "spawn 식 안에서 바깥 변수",
    );
}

#[test]
fn test_spawn_read_outer_ok() {
    // spawn 안에서 바깥 변수 읽기는 허용
    expect_ok("let x = 10\nfn id(v) { return v }\nlet _ = spawn id(x)");
}

#[test]
fn test_spawn_inner_assign_ok() {
    // spawn 내부 함수 안에서 지역 변수 대입은 허용
    expect_ok("fn work() { let v = 1\nv = 2\nreturn v }\nlet _ = spawn work()");
}

// =============================================================================
// 경고 케이스
// =============================================================================

#[test]
fn test_unreachable_after_return() {
    expect_warning(
        "fn f() {\n    return 1\n    print(\"dead\")\n}\nf()",
        "도달 불가",
    );
}

#[test]
fn test_unused_variable() {
    expect_warning(
        "fn f() {\n    let unused = 42\n    return 0\n}\nf()",
        "사용되지 않은 변수: 'unused'",
    );
}

#[test]
fn test_unused_variable_underscore_suppressed() {
    // _로 시작하면 경고 없음
    let r = resolve_source("fn f() {\n    let _unused = 42\n    return 0\n}\nf()");
    let found = r.warnings.iter().any(|w| w.message.contains("'_unused'"));
    assert!(!found, "_ 접두사 변수는 미사용 경고가 없어야 함");
}

// =============================================================================
// resolve 테이블 검증
// =============================================================================

#[test]
fn test_var_ref_same_scope() {
    let r = resolve_source("let x = 1\nprint(x)");
    assert!(r.errors.is_empty(), "오류 없어야 함: {:?}", r.errors);
    // 전역 스코프에서의 참조는 모두 depth=0
    let has_depth0 = r.table.ident_refs.values().any(|vr| vr.depth == 0);
    assert!(has_depth0, "전역 변수 참조는 depth=0 이어야 함");
}

#[test]
fn test_var_ref_outer_scope() {
    // fn 안에서 바깥 변수 참조 → depth=1
    let r = resolve_source("let x = 10\nfn f() { return x }\nf()");
    assert!(r.errors.is_empty(), "오류 없어야 함: {:?}", r.errors);
    // x 참조(return x 내)가 depth>=1로 기록되어야 함
    let has_depth1 = r.table.ident_refs.values().any(|vr| vr.depth >= 1);
    assert!(has_depth1, "외부 변수 참조는 depth >= 1 이어야 함");
}

#[test]
fn test_let_slots_recorded() {
    let r = resolve_source("let a = 1\nlet b = 2\nprint(a + b)");
    assert!(r.errors.is_empty());
    // 두 개의 let 슬롯이 기록되어야 함
    assert_eq!(r.table.let_slots.len(), 2, "let 슬롯 2개 기록");
}

// =============================================================================
// 12개 examples/ 파일 — 모두 오류 0개여야 함
// =============================================================================

#[test]
fn test_example_hello() {
    expect_file_ok("hello.bang");
}

#[test]
fn test_example_fibonacci() {
    expect_file_ok("fibonacci.bang");
}

#[test]
fn test_example_recursion() {
    expect_file_ok("recursion.bang");
}

#[test]
fn test_example_closures() {
    expect_file_ok("closures.bang");
}

#[test]
fn test_example_list_traversal() {
    expect_file_ok("list_traversal.bang");
}

#[test]
fn test_example_map_usage() {
    expect_file_ok("map_usage.bang");
}

#[test]
fn test_example_conditionals() {
    expect_file_ok("conditionals.bang");
}

#[test]
fn test_example_string_ops() {
    expect_file_ok("string_ops.bang");
}

#[test]
fn test_example_higher_order() {
    expect_file_ok("higher_order.bang");
}

#[test]
fn test_example_spawn_basic() {
    expect_file_ok("spawn_basic.bang");
}

#[test]
fn test_example_parallel_block() {
    expect_file_ok("parallel_block.bang");
}

#[test]
fn test_example_channels() {
    expect_file_ok("channels.bang");
}
