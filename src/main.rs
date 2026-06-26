// Bang 프로그래밍 언어 — CLI 진입점

use bang::ast::dump_program;
use bang::compiler::compile;
use bang::interpreter::Interpreter;
use bang::lexer::Lexer;
use bang::lexer::token::Span;
use bang::parser::Parser;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process;
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "run"      => cmd_run(&args[2..]),
        "check"    => cmd_check(&args[2..]),
        "build"    => cmd_build(&args[2..]),
        "compile"  => cmd_compile(&args[2..]),
        "repl"     => cmd_repl(),
        "tokenize" => cmd_tokenize(&args[2..]),
        "parse"    => cmd_parse(&args[2..]),
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("알 수 없는 명령: {}", args[1]);
            print_usage();
            process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("Bang 프로그래밍 언어 v0.5.0");
    eprintln!();
    eprintln!("사용법: bang <명령> [옵션] [인자...]");
    eprintln!();
    eprintln!("명령:");
    eprintln!("  run     [--interp] [--jit] [--dump-ast] <파일>  .bang 파일 실행");
    eprintln!("                    기본: VM, --interp: 트리-워킹 인터프리터");
    eprintln!("                    --jit: Cranelift JIT 백엔드 (--features jit 빌드 필요)");
    eprintln!("  compile -o <출력> <파일>  AOT 컴파일 (C 트랜스파일 + cc -O2)");
    eprintln!("  check   <파일>   오류 검사 (실행 없음)");
    eprintln!("  build   <파일>   컴파일 검증 + 통계 출력");
    eprintln!("  parse   <파일>   AST 출력");
    eprintln!("  tokenize <파일>  토큰화 출력 (디버그)");
    eprintln!("  repl             대화형 셸(REPL) 시작");
    eprintln!("  help             도움말 출력");
}

// ============================================================================
// 소스 파일 읽기 / 렉싱+파싱
// ============================================================================

fn read_source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("오류: '{path}' 파일을 읽을 수 없습니다: {e}");
        process::exit(1);
    })
}

fn lex_and_parse(source: &str, _path: &str) -> bang::ast::Program {
    let mut lexer = Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(errors) => {
            for e in &errors {
                let span = Span::new(e.line, e.col);
                eprintln!("{}", format_with_context(&e.to_string(), span, source));
            }
            process::exit(1);
        }
    };
    match Parser::new(tokens).parse() {
        Ok(prog) => prog,
        Err(errors) => {
            for e in &errors {
                eprintln!("{}", format_with_context(&e.to_string(), e.span, source));
            }
            process::exit(1);
        }
    }
}

// ============================================================================
// Phase 7: 소스 컨텍스트가 있는 에러 포맷
// ============================================================================

fn format_with_context(message: &str, span: Span, source: &str) -> String {
    if span.line == 0 {
        return message.to_string();
    }
    let line_num = span.line;
    let col_num  = span.col;
    let line_text = source.lines().nth(line_num.saturating_sub(1)).unwrap_or("");
    let pointer_offset = col_num.saturating_sub(1);
    format!(
        "{message}\n  |\n{:>3} | {line_text}\n  | {:>pointer_offset$}^",
        line_num, ""
    )
}

// ============================================================================
// bang run
// ============================================================================

fn cmd_run(args: &[String]) {
    let mut use_interp = false;
    let mut use_jit    = false;
    let mut dump_ast   = false;
    let mut path: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "--interp"   => use_interp = true,
            "--jit"      => use_jit    = true,
            "--dump-ast" => dump_ast   = true,
            _            => { path = Some(arg); }
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            eprintln!("오류: 파일 경로가 필요합니다");
            eprintln!("사용법: bang run [--interp] [--jit] [--dump-ast] <파일.bang>");
            process::exit(1);
        }
    };

    let source = read_source(path);
    let prog   = lex_and_parse(&source, path);

    if dump_ast {
        print!("{}", dump_program(&prog));
        return;
    }

    let resolve_result = bang::resolver::resolve(&prog);
    for w in &resolve_result.warnings {
        eprintln!("{}", format_with_context(&w.to_string(), w.span, &source));
    }
    if !resolve_result.errors.is_empty() {
        for e in &resolve_result.errors {
            eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
        }
        process::exit(1);
    }

    if use_jit {
        #[cfg(not(feature = "jit"))]
        {
            eprintln!("{}", bang::codegen::JIT_DISABLED_MSG);
            process::exit(1);
        }
        #[cfg(feature = "jit")]
        {
            // JIT 모드: VM 실행 (내부 함수 호출 시 JIT 시도, 폴백 내장)
            run_vm_program(&prog, &source);
            return;
        }
    }

    if use_interp {
        let interp = Interpreter::new();
        if let Err(e) = interp.run(&prog) {
            eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
            process::exit(1);
        }
    } else {
        run_vm_program(&prog, &source);
    }
}

fn run_vm_program(prog: &bang::ast::Program, source: &str) {
    let out = match compile(prog) {
        Ok(o) => o,
        Err(errors) => {
            for e in &errors {
                eprintln!("{}", format_with_context(&e.to_string(), e.span, source));
            }
            process::exit(1);
        }
    };
    let output = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut vm = bang::vm::Vm::new(out.global_count as usize, output);
    if let Err(e) = vm.run(out.main_fn) {
        eprintln!("{}", format_with_context(&e.to_string(), e.span, source));
        process::exit(1);
    }
}

// ============================================================================
// bang check
// ============================================================================

fn cmd_check(args: &[String]) {
    if args.is_empty() {
        eprintln!("오류: 파일 경로가 필요합니다");
        process::exit(1);
    }
    let path   = &args[0];
    let source = read_source(path);
    let prog   = lex_and_parse(&source, path);

    let resolve_result = bang::resolver::resolve(&prog);
    for w in &resolve_result.warnings {
        eprintln!("{}", format_with_context(&w.to_string(), w.span, &source));
    }
    if !resolve_result.errors.is_empty() {
        for e in &resolve_result.errors {
            eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
        }
        process::exit(1);
    }

    match compile(&prog) {
        Ok(out) => {
            println!("OK: {path}");
            println!("  전역 바인딩: {}개  opcodes: {}바이트",
                out.global_count,
                out.main_fn.chunk.code.len());
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
            }
            process::exit(1);
        }
    }
}

// ============================================================================
// bang build
// ============================================================================

fn cmd_build(args: &[String]) {
    if args.is_empty() {
        eprintln!("오류: 파일 경로가 필요합니다");
        process::exit(1);
    }
    let path   = &args[0];
    let source = read_source(path);
    let prog   = lex_and_parse(&source, path);

    let resolve_result = bang::resolver::resolve(&prog);
    for w in &resolve_result.warnings {
        eprintln!("{}", format_with_context(&w.to_string(), w.span, &source));
    }
    if !resolve_result.errors.is_empty() {
        for e in &resolve_result.errors {
            eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
        }
        process::exit(1);
    }

    match compile(&prog) {
        Ok(out) => {
            let code_size = out.main_fn.chunk.code.len();
            let const_count = out.main_fn.chunk.constants.len();
            println!("빌드 완료: {path}");
            println!("  전역: {}개  상수 풀: {}개  메인 opcode: {}바이트",
                out.global_count, const_count, code_size);
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
            }
            process::exit(1);
        }
    }
}

// ============================================================================
// bang parse / tokenize
// ============================================================================

fn cmd_parse(args: &[String]) {
    if args.is_empty() {
        eprintln!("오류: 파일 경로가 필요합니다");
        process::exit(1);
    }
    let source = read_source(&args[0]);
    let prog   = lex_and_parse(&source, &args[0]);
    print!("{}", dump_program(&prog));
}

fn cmd_tokenize(args: &[String]) {
    if args.is_empty() {
        eprintln!("오류: 파일 경로가 필요합니다");
        process::exit(1);
    }
    let source = read_source(&args[0]);
    let mut lexer = Lexer::new(&source);
    match lexer.tokenize() {
        Ok(tokens) => {
            for t in &tokens { println!("{t}"); }
            eprintln!("--- 토큰 {}개 ---", tokens.len());
        }
        Err(errors) => {
            for e in &errors { eprintln!("{e}"); }
            process::exit(1);
        }
    }
}

// ============================================================================
// Phase 7: REPL (인터프리터 기반, 지속 상태)
// ============================================================================

fn cmd_repl() {
    eprintln!("Bang REPL v0.3.0  (종료: exit(0) 또는 Ctrl+C)");
    eprintln!();

    let interp = Interpreter::new();
    let stdin = io::stdin();
    let mut buf = String::new();
    let mut depth: i32 = 0; // 열린 괄호/중괄호 깊이

    loop {
        // 프롬프트
        if depth == 0 {
            print!(">>> ");
        } else {
            print!("... ");
        }
        io::stdout().flush().unwrap();

        buf.clear();
        // 현재 행 읽기
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => { eprintln!("읽기 오류: {e}"); break; }
        }

        // 중괄호/괄호 깊이 추적 (문자열 리터럴 안은 무시)
        let in_depth_change = bracket_depth_delta(&line);
        depth += in_depth_change;
        buf.push_str(&line);

        if depth > 0 {
            // 미완성 식 — 더 읽기
            continue;
        }
        if depth < 0 { depth = 0; }

        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 렉싱+파싱+실행
        let tokens = match Lexer::new(trimmed).tokenize() {
            Ok(t) => t,
            Err(errors) => {
                for e in &errors { eprintln!("렉서 오류: {e}"); }
                continue;
            }
        };
        let prog = match Parser::new(tokens).parse() {
            Ok(p) => p,
            Err(errors) => {
                for e in &errors { eprintln!("파서 오류: {e}"); }
                continue;
            }
        };

        // 출력 버퍼 현재 위치 기록
        let out_before = interp.output.lock().unwrap().len();

        if let Err(e) = interp.run_incremental(&prog) {
            eprintln!("오류: {}", e.message);
        }

        // 새로 추가된 출력 줄 표시
        let lines = interp.output.lock().unwrap();
        for line_str in &lines[out_before..] {
            println!("{line_str}");
        }
    }
}

// ============================================================================
// Phase 10: bang compile  (AOT — C 트랜스파일 + cc -O2)
// ============================================================================

fn cmd_compile(args: &[String]) {
    // 인자 파싱: -o <output> <source>
    let mut output_path: Option<&str> = None;
    let mut source_path: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("오류: -o 뒤에 출력 파일 경로가 필요합니다");
                    process::exit(1);
                }
                output_path = Some(&args[i]);
            }
            _ => { source_path = Some(&args[i]); }
        }
        i += 1;
    }

    let source_path = match source_path {
        Some(p) => p,
        None => {
            eprintln!("오류: 소스 파일 경로가 필요합니다");
            eprintln!("사용법: bang compile -o <출력> <파일.bang>");
            process::exit(1);
        }
    };
    let output_path = output_path.unwrap_or("a.out");

    // 렉싱 + 파싱
    let source = read_source(source_path);
    let prog   = lex_and_parse(&source, source_path);

    // C 트랜스파일
    let c_code = match bang::codegen::transpile(&prog) {
        Ok(c) => c,
        Err(errors) => {
            for e in &errors {
                eprintln!("{}", format_with_context(&e.to_string(), e.span, &source));
            }
            process::exit(1);
        }
    };

    // 임시 .c 파일에 기록
    let c_path = format!("{output_path}.tmp.c");
    if let Err(e) = fs::write(&c_path, &c_code) {
        eprintln!("오류: C 소스 파일 쓰기 실패: {e}");
        process::exit(1);
    }

    // cc -O2 로 컴파일
    let status = std::process::Command::new("cc")
        .args(["-O2", "-lm", "-o", output_path, &c_path])
        .status();

    // 임시 파일 삭제 (실패해도 계속)
    let _ = fs::remove_file(&c_path);

    match status {
        Ok(s) if s.success() => {
            println!("컴파일 완료: {source_path} → {output_path}");
        }
        Ok(s) => {
            eprintln!("cc 컴파일 실패 (종료 코드: {:?})", s.code());
            process::exit(1);
        }
        Err(e) => {
            eprintln!("cc 실행 오류: {e}");
            eprintln!("힌트: GCC/Clang이 설치돼 있고 PATH에 있는지 확인하세요.");
            process::exit(1);
        }
    }
}

/// 한 줄에서 괄호 깊이 변화량 계산 (문자열 리터럴 대략 처리)
fn bracket_depth_delta(line: &str) -> i32 {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut str_char = '"';
    for ch in line.chars() {
        if escape { escape = false; continue; }
        if in_str {
            if ch == '\\' { escape = true; }
            else if ch == str_char { in_str = false; }
        } else {
            match ch {
                '"' | '\'' => { in_str = true; str_char = ch; }
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                '#' => break, // 주석
                _ => {}
            }
        }
    }
    depth
}
