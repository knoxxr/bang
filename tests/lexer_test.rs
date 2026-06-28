// Bang 통합 테스트 — Lexer

use bang::lexer::token::TokenKind;
use bang::lexer::Lexer;
use std::fs;
use std::path::Path;

/// examples/ 디렉터리의 모든 .bang 파일을 토큰화 — 패닉·에러 없어야 함
#[test]
fn test_all_examples_tokenize_without_error() {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut count = 0;

    for entry in fs::read_dir(&examples_dir).expect("examples/ 디렉터리 읽기 실패") {
        let entry = entry.expect("디렉터리 엔트리 읽기 실패");
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("bang") {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: 파일 읽기 실패: {e}", path.display()));

        let mut lexer = Lexer::new(&source);
        let result = lexer.tokenize();
        assert!(
            result.is_ok(),
            "{}: 토큰화 실패: {:?}",
            path.display(),
            result.unwrap_err()
        );

        let tokens = result.unwrap();

        assert_eq!(
            tokens.last().unwrap().kind,
            TokenKind::Eof,
            "{}: 마지막 토큰이 Eof가 아님",
            path.display()
        );

        count += 1;
    }

    assert_eq!(count, 16, "examples/ 에 .bang 파일 16개 기대, {count}개 발견");
}

/// hello.bang 파일의 토큰화 결과가 예상과 일치하는지 확인
#[test]
fn test_hello_example_snapshot() {
    let source = include_str!("../examples/hello.bang");
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("hello.bang 토큰화 실패");

    let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();

    // 1행: fn greet(name) { print("hello " + name) }
    // 2행: greet("world")
    assert_eq!(kinds[0], &TokenKind::Fn);
    assert_eq!(kinds[1], &TokenKind::Ident("greet".into()));
    assert_eq!(kinds[2], &TokenKind::LParen);
    assert_eq!(kinds[3], &TokenKind::Ident("name".into()));
    assert_eq!(kinds[4], &TokenKind::RParen);
    assert_eq!(kinds[5], &TokenKind::LBrace);
    assert_eq!(kinds[6], &TokenKind::Ident("print".into()));
    assert_eq!(kinds[12], &TokenKind::RBrace);
    assert_eq!(kinds[13], &TokenKind::Newline); // } 뒤 줄바꿈
    assert_eq!(kinds[14], &TokenKind::Ident("greet".into()));
    assert_eq!(kinds.last().unwrap(), &&TokenKind::Eof);
}

/// fibonacci.bang — 복합 프로그램 토큰화 검증
#[test]
fn test_fibonacci_tokenizes_correctly() {
    let source = include_str!("../examples/fibonacci.bang");
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("fibonacci.bang 토큰화 실패");

    // fn 키워드가 2번 등장 (fib, fib_iter)
    let fn_count = tokens.iter().filter(|t| t.kind == TokenKind::Fn).count();
    assert_eq!(fn_count, 2, "fn 키워드 2개 기대, {fn_count}개 발견");

    // while 키워드가 2번 등장
    let while_count = tokens.iter().filter(|t| t.kind == TokenKind::While).count();
    assert_eq!(while_count, 2, "while 키워드 2개 기대, {while_count}개 발견");

    // 중괄호 균형
    let lbrace = tokens.iter().filter(|t| t.kind == TokenKind::LBrace).count();
    let rbrace = tokens.iter().filter(|t| t.kind == TokenKind::RBrace).count();
    assert_eq!(lbrace, rbrace, "중괄호 불균형: {{ {lbrace} vs }} {rbrace}");
}
