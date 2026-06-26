// Bang 통합 테스트 — Parser
//
// examples/ 디렉터리의 모든 .bang 파일이 에러 없이 파싱되는지 검증한다.

use bang::ast::{dump_program, ExprKind, StmtKind};
use bang::lexer::Lexer;
use bang::parser::Parser;
use std::fs;
use std::path::Path;

fn parse_source(source: &str) -> bang::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("렉싱 실패");
    Parser::new(tokens).parse().expect("파싱 실패")
}

// =============================================================================
// 전체 examples/ 파싱 통합 테스트
// =============================================================================

#[test]
fn test_all_examples_parse_without_error() {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut count = 0;

    for entry in fs::read_dir(&examples_dir).expect("examples/ 디렉터리 읽기 실패") {
        let entry = entry.expect("엔트리 읽기 실패");
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("bang") {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: 읽기 실패: {e}", path.display()));

        let mut lexer = Lexer::new(&source);
        let tokens = lexer
            .tokenize()
            .unwrap_or_else(|e| panic!("{}: 토큰화 실패: {e:?}", path.display()));

        Parser::new(tokens).parse().unwrap_or_else(|errs| {
            for e in &errs {
                eprintln!("{}: {e}", path.display());
            }
            panic!("{}: 파싱 실패", path.display());
        });

        count += 1;
    }

    assert_eq!(count, 12, "examples/ 에 .bang 파일 12개 기대, {count}개 발견");
}

// =============================================================================
// fibonacci.bang AST 구조 확인 (--dump-ast 대응)
// =============================================================================

#[test]
fn test_fibonacci_ast_dump() {
    let source = include_str!("../examples/fibonacci.bang");
    let prog = parse_source(source);

    let dump = dump_program(&prog);

    // fib, fib_iter 두 함수가 Let 으로 디슈가되어야 함
    let let_count = prog.stmts.iter().filter(|s| matches!(s.kind, StmtKind::Let { .. })).count();
    assert_eq!(let_count, 3, "Let 3개 기대 (fib, fib_iter, i)"); // fib + fib_iter + let i = 0

    // while 문 1개 (최상위)
    let while_count = prog.stmts.iter().filter(|s| matches!(s.kind, StmtKind::While { .. })).count();
    assert_eq!(while_count, 1, "While 1개 기대 (최상위)");

    // dump 에 Function 이 포함되어야 함
    assert!(dump.contains("Function(fib)"), "fib 함수 포함");
    assert!(dump.contains("Function(fib_iter)"), "fib_iter 함수 포함");
}

// =============================================================================
// 개별 예제 파싱 검증
// =============================================================================

#[test]
fn test_closures_parsed() {
    let source = include_str!("../examples/closures.bang");
    let prog = parse_source(source);
    // make_adder, make_counter 두 함수 + let 들
    let let_count = prog.stmts.iter().filter(|s| matches!(s.kind, StmtKind::Let { .. })).count();
    assert!(let_count >= 2);
}

#[test]
fn test_map_usage_parsed() {
    let source = include_str!("../examples/map_usage.bang");
    let prog = parse_source(source);
    // let person = { ... } — 첫 번째 문이 Let 이어야 함
    assert!(matches!(prog.stmts[0].kind, StmtKind::Let { .. }));
}

#[test]
fn test_list_traversal_parsed() {
    let source = include_str!("../examples/list_traversal.bang");
    let prog = parse_source(source);
    // for-in 이 있어야 함
    let has_for = prog.stmts.iter().any(|s| matches!(s.kind, StmtKind::For { .. }));
    assert!(has_for, "for-in 문 있어야 함");
}

#[test]
fn test_spawn_basic_parsed() {
    let source = include_str!("../examples/spawn_basic.bang");
    let prog = parse_source(source);
    // spawn 표현식이 포함된 Let 이 있어야 함
    let has_spawn = prog.stmts.iter().any(|s| {
        if let StmtKind::Let { value, .. } = &s.kind {
            matches!(value.kind, ExprKind::Spawn(_))
        } else {
            false
        }
    });
    assert!(has_spawn, "Spawn 식 있어야 함");
}

#[test]
fn test_parallel_block_parsed() {
    let source = include_str!("../examples/parallel_block.bang");
    let prog = parse_source(source);
    let has_parallel = prog.stmts.iter().any(|s| matches!(s.kind, StmtKind::Parallel(_)));
    assert!(has_parallel, "Parallel 문 있어야 함");
}

#[test]
fn test_channels_parsed() {
    let source = include_str!("../examples/channels.bang");
    let prog = parse_source(source);
    // fn producer, fn consumer (Let 으로 디슈가)
    let let_count = prog.stmts.iter().filter(|s| matches!(s.kind, StmtKind::Let { .. })).count();
    assert!(let_count >= 3, "Let >= 3 기대 (producer, consumer, ch)");
}

#[test]
fn test_higher_order_parsed() {
    let source = include_str!("../examples/higher_order.bang");
    let prog = parse_source(source);
    // apply, double, square 함수 + make_multiplier
    let fn_count = prog.stmts.iter().filter(|s| {
        if let StmtKind::Let { value, .. } = &s.kind {
            matches!(value.kind, ExprKind::Function { .. })
        } else {
            false
        }
    }).count();
    assert!(fn_count >= 4, "함수 >= 4개 기대");
}
