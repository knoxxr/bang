// Bang 프로그래밍 언어 — CLI 진입점

use bang::lexer::Lexer;
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "repl" => cmd_repl(),
        "tokenize" => cmd_tokenize(&args[2..]),
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("알 수 없는 명령: {}", args[1]);
            print_usage();
            process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("Bang 프로그래밍 언어 v0.1.0");
    eprintln!();
    eprintln!("사용법: bang <명령> [인자...]");
    eprintln!();
    eprintln!("명령:");
    eprintln!("  run <파일>        .bang 파일 실행");
    eprintln!("  repl              대화형 셸(REPL) 시작");
    eprintln!("  tokenize <파일>   소스 파일을 토큰화하여 출력 (디버그)");
    eprintln!("  help              도움말 출력");
}

fn cmd_run(args: &[String]) {
    if args.is_empty() {
        eprintln!("오류: 파일 경로가 필요합니다");
        eprintln!("사용법: bang run <파일.bang>");
        process::exit(1);
    }

    let path = &args[0];
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("오류: '{path}' 파일을 읽을 수 없습니다: {e}");
            process::exit(1);
        }
    };

    // TODO: 렉서 → 파서 → 인터프리터 파이프라인 구현
    eprintln!("[stub] '{path}' 소스 읽기 완료 ({} bytes)", source.len());
    eprintln!("[stub] 인터프리터 미구현 — Phase 2+ 에서 구현 예정");
}

fn cmd_repl() {
    eprintln!("Bang REPL v0.1.0");
    eprintln!("[stub] REPL 미구현 — Phase 2+ 에서 구현 예정");
}

fn cmd_tokenize(args: &[String]) {
    if args.is_empty() {
        eprintln!("오류: 파일 경로가 필요합니다");
        eprintln!("사용법: bang tokenize <파일.bang>");
        process::exit(1);
    }

    let path = &args[0];
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("오류: '{path}' 파일을 읽을 수 없습니다: {e}");
            process::exit(1);
        }
    };

    let mut lexer = Lexer::new(&source);
    match lexer.tokenize() {
        Ok(tokens) => {
            for token in &tokens {
                println!("{token}");
            }
            eprintln!("--- 토큰 {}개 ---", tokens.len());
        }
        Err(errors) => {
            for err in &errors {
                eprintln!("{err}");
            }
            process::exit(1);
        }
    }
}
