// Bang 프로그래밍 언어 — CLI 진입점

use bang::ast::dump_program;
use bang::compiler::compile;
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

    // 인자 없이 실행 → REPL 진입 (Python 동작)
    if args.len() < 2 {
        cmd_repl();
        return;
    }

    match args[1].as_str() {
        "run"      => cmd_run(&args[2..]),
        "check"    => cmd_check(&args[2..]),
        "build"    => cmd_build(&args[2..]),
        "compile"  => cmd_compile(&args[2..]),
        "repl"     => cmd_repl(),
        "tokenize" => cmd_tokenize(&args[2..]),
        "parse"    => cmd_parse(&args[2..]),
        "add"      => cmd_add(&args[2..]),
        "install"  => cmd_install(&args[2..]),
        "new"      => cmd_new(&args[2..]),
        "test"     => cmd_test(&args[2..]),
        "fmt"      => cmd_fmt(&args[2..]),
        "help" | "--help" | "-h" => print_usage(),
        "version" | "--version" | "-V" => print_version(),
        other => {
            // 알려진 명령이 아니면 파일 경로로 간주해 실행한다.
            // (bang script.bang  /  ./script.bang  /  bang - )
            if other == "-" || std::path::Path::new(other).is_file() {
                cmd_run(&args[1..]);
            } else {
                eprintln!("알 수 없는 명령 또는 존재하지 않는 파일: {other}");
                print_usage();
                process::exit(1);
            }
        }
    }
}

fn print_version() {
    println!("bang {}", env!("CARGO_PKG_VERSION"));
}

fn print_usage() {
    eprintln!("Bang 프로그래밍 언어 v{}", env!("CARGO_PKG_VERSION"));
    eprintln!();
    eprintln!("사용법:");
    eprintln!("  bang <파일.bang>          .bang 파일 실행 (run 생략 가능)");
    eprintln!("  bang                      인자 없이 실행하면 REPL 진입");
    eprintln!("  bang <명령> [옵션] [인자...]");
    eprintln!();
    eprintln!("명령:");
    eprintln!("  run     [--dump-ast] <파일|->  .bang 파일/표준입력 실행 (바이트코드 VM)");
    eprintln!("  compile -o <출력> <파일>  [실험적] AOT 컴파일 (C 트랜스파일 + cc -O2,");
    eprintln!("                    정수·문자열·함수만 지원 — 리스트/맵/동시성 미지원)");
    eprintln!("  check   <파일>   오류 검사 (실행 없음)");
    eprintln!("  build   <파일>   컴파일 검증 + 통계 출력");
    eprintln!("  parse   <파일>   AST 출력");
    eprintln!("  tokenize <파일>  토큰화 출력 (디버그)");
    eprintln!("  repl             대화형 셸(REPL) 시작");
    eprintln!("  test [경로...]   테스트 실행: *_test.bang / test_*.bang 파일의 test_* 함수");
    eprintln!("  fmt [--check] [경로...]  들여쓰기 정규화 (--check: 바뀔 파일만 보고, exit 1)");
    eprintln!("  new <디렉토리>   새 프로젝트 스캐폴딩 (main.bang + bang.toml)");
    eprintln!("  add <이름> <git-url[@rev]>  의존성 추가 (bang.toml + bang_modules/ 설치)");
    eprintln!("  install          bang.toml의 모든 의존성 설치");
    eprintln!("  version          버전 출력");
    eprintln!("  help             도움말 출력");
}

// ============================================================================
// 소스 파일 읽기 / 렉싱+파싱
// ============================================================================

fn read_source(path: &str) -> String {
    // `-` 는 표준 입력에서 소스를 읽는다 (echo '...' | bang -).
    if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut s) {
            eprintln!("오류: 표준 입력을 읽을 수 없습니다: {e}");
            process::exit(1);
        }
        return s;
    }
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

/// 런타임 에러 출력: import한 모듈 안에서 난 에러면 그 모듈 파일의 줄을 보여 준다 (Phase 56).
fn runtime_error_text(e: &bang::runtime::RuntimeError, main_source: &str) -> String {
    match &e.file {
        Some(path) => match fs::read_to_string(path.as_ref()) {
            Ok(src) => format_with_context(&e.to_string(), e.span, &src),
            Err(_) => e.to_string(),
        },
        None => format_with_context(&e.to_string(), e.span, main_source),
    }
}

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
    let mut dump_ast = false;
    let mut path: Option<&str> = None;
    let mut script_args: Vec<String> = Vec::new();

    // 옵션은 파일 경로 앞에만 온다. 경로 뒤의 모든 인자는 스크립트의 것이다 (args() — Phase 62).
    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--dump-ast" => dump_ast = true,
            // v0.30.0에서 제거된 백엔드 — 파일 경로로 오인하지 않게 명확히 알린다.
            "--interp" | "--jit" => {
                eprintln!("오류: '{arg}'는 v0.30.0에서 제거되었습니다 — 바이트코드 VM이 유일한 실행 엔진입니다");
                eprintln!("      ('{arg}' 없이 'bang run <파일>'로 실행하세요)");
                process::exit(1);
            }
            _ => {
                path = Some(arg);
                script_args = args[i..].to_vec();
                break;
            }
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            eprintln!("오류: 파일 경로가 필요합니다");
            eprintln!("사용법: bang run [--dump-ast] <파일.bang>");
            process::exit(1);
        }
    };

    bang::stdlib::set_script_args(script_args);
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

    run_vm_program(&prog, &source, path);
}

fn run_vm_program(prog: &bang::ast::Program, source: &str, path: &str) {
    // 상대 경로 import는 스크립트 위치 기준 (Phase 62). 표준 입력(-)은 현재 디렉터리 기준.
    let compiled = if path == "-" { compile(prog) } else { bang::compiler::compile_main(prog, path) };
    let out = match compiled {
        Ok(o) => o,
        Err(errors) => {
            for e in &errors {
                eprintln!("{}", format_with_context(&e.to_string(), e.span, source));
            }
            process::exit(1);
        }
    };
    let mut vm = bang::vm::Vm::new(out.global_count as usize, bang::vm::Output::Stdout);
    if let Err(e) = vm.run(out.main_fn) {
        eprintln!("{}", runtime_error_text(&e, source));
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

    // 정적 타입 검사 (gradual): 확실한 타입 충돌만 보고
    let type_errors = bang::typeck::check(&prog);
    if !type_errors.is_empty() {
        for e in &type_errors {
            eprintln!("{}", format_with_context(
                &format!("타입 오류: {}", e.message), e.span, &source));
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
    eprintln!("Bang REPL v{}  (종료: exit(0) 또는 Ctrl+C)", env!("CARGO_PKG_VERSION"));
    eprintln!();

    // VM 기반 REPL: 전역(name→slot 맵 + 값 배열)을 스니펫 간 공유해 상태를 유지한다.
    // try/catch·import·타입힌트·JSON/정규식/tcp 등 VM 전용 기능이 REPL에서도 동작.
    let shared_globals: Arc<Mutex<Vec<bang::vm::VmValue>>> = Arc::new(Mutex::new(Vec::new()));
    let output = bang::vm::Output::Stdout;
    let mut globals_map: std::collections::HashMap<String, u16> = std::collections::HashMap::new();

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

        // 증분 컴파일 (이전 스니펫의 전역 슬롯 배치를 시드로 유지)
        let out = match bang::compiler::compile_repl(&prog, &globals_map) {
            Ok(o) => o,
            Err(errors) => {
                for e in &errors {
                    eprintln!("{}", format_with_context(&e.to_string(), e.span, trimmed));
                }
                continue;
            }
        };
        globals_map = out.global_names.clone();

        // 전역 배열을 새 global_count 크기로 확장 (기존 값 유지)
        {
            let mut g = shared_globals.lock().unwrap();
            if g.len() < out.global_count as usize {
                g.resize(out.global_count as usize, bang::vm::VmValue::Nil);
            }
        }

        // 스니펫마다 새 VM(공유 전역) — 이전 에러의 잔여 상태가 남지 않음
        let mut vm = bang::vm::Vm::with_globals(shared_globals.clone(), output.clone());
        if let Err(e) = vm.run(out.main_fn) {
            eprintln!("{}", runtime_error_text(&e, trimmed));
        }
        // (print는 VM이 stdout에 직접 출력하므로 별도 echo 불필요)
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
    let c_code = match bang::codegen::transpile_with_source(&prog, Some(&source)) {
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

    // C 컴파일러 후보를 순서대로 시도한다 (cc → clang → gcc).
    // macOS/Linux엔 보통 cc가 있고, Windows(MSVC)엔 없을 수 있으나
    // MinGW(gcc)나 LLVM(clang)을 설치하면 동일한 unix 플래그로 동작한다.
    let candidates = ["cc", "clang", "gcc"];
    let mut last_spawn_err: Option<std::io::Error> = None;
    let mut compiled = false;
    for compiler in candidates {
        match std::process::Command::new(compiler)
            .args(["-O2", "-lm", "-o", output_path, &c_path])
            .status()
        {
            Ok(s) if s.success() => {
                compiled = true;
                break;
            }
            Ok(s) => {
                // 컴파일러는 있으나 컴파일 자체가 실패 → 같은 C이므로 다른 후보도 무의미.
                let _ = fs::remove_file(&c_path);
                eprintln!("{compiler} 컴파일 실패 (종료 코드: {:?})", s.code());
                process::exit(1);
            }
            // 이 컴파일러가 PATH에 없음 → 다음 후보 시도.
            Err(e) => last_spawn_err = Some(e),
        }
    }

    // 임시 파일 삭제 (실패해도 계속)
    let _ = fs::remove_file(&c_path);

    if compiled {
        println!("컴파일 완료: {source_path} → {output_path}");
    } else {
        eprintln!("오류: C 컴파일러를 찾을 수 없습니다 (시도: cc, clang, gcc)");
        if let Some(e) = last_spawn_err {
            eprintln!("  마지막 실행 오류: {e}");
        }
        eprintln!("힌트: macOS는 'xcode-select --install', Linux는 gcc/clang,");
        eprintln!("      Windows는 MSYS2(MinGW) 또는 LLVM(clang)을 PATH에 설치하세요.");
        process::exit(1);
    }
}

// ============================================================================
// 패키지 관리: bang add / bang install
// ============================================================================

const MANIFEST: &str = "bang.toml";

fn read_deps() -> Vec<bang::pkg::Dependency> {
    match fs::read_to_string(MANIFEST) {
        Ok(content) => bang::pkg::parse_manifest(&content),
        Err(_) => Vec::new(),
    }
}

fn write_deps(deps: &[bang::pkg::Dependency]) {
    // [package] 등 다른 섹션은 보존하고 [dependencies]만 바꾼다 (Phase 62)
    let existing = fs::read_to_string(MANIFEST).unwrap_or_default();
    let text = bang::pkg::update_manifest(&existing, deps);
    if let Err(e) = fs::write(MANIFEST, text) {
        eprintln!("오류: {MANIFEST} 쓰기 실패: {e}");
        process::exit(1);
    }
}

/// git clone 으로 의존성을 bang_modules/<name> 에 설치한다.
fn install_dep(dep: &bang::pkg::Dependency) -> bool {
    let dest = format!("bang_modules/{}", dep.name);
    if std::path::Path::new(&dest).exists() {
        println!("이미 설치됨: {} ({dest})", dep.name);
        return true;
    }
    let _ = fs::create_dir_all("bang_modules");
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(rev) = &dep.rev {
        cmd.arg("--branch").arg(rev);
    }
    cmd.arg(&dep.url).arg(&dest);
    println!("설치 중: {} ← {}{}", dep.name, dep.url,
        dep.rev.as_ref().map(|r| format!("@{r}")).unwrap_or_default());
    match cmd.status() {
        Ok(s) if s.success() => { println!("완료: {}", dep.name); true }
        Ok(s) => { eprintln!("git clone 실패 ({}): 종료 코드 {:?}", dep.name, s.code()); false }
        Err(e) => { eprintln!("git 실행 오류: {e} (git이 설치돼 있는지 확인)"); false }
    }
}

/// bang new <dir> — 새 프로젝트 스캐폴딩 (main.bang + bang.toml + .gitignore).
fn cmd_new(args: &[String]) {
    if args.is_empty() {
        eprintln!("사용법: bang new <디렉토리>");
        process::exit(1);
    }
    let dir = std::path::Path::new(&args[0]);
    let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("app");

    if dir.exists() {
        eprintln!("이미 존재합니다: {}", dir.display());
        process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("디렉토리 생성 실패: {e}");
        process::exit(1);
    }

    let main_bang = "\
// 순차적으로 생각하고, 병렬로 실행한다.\n\
fn greet(who) {\n    return \"안녕, \" + who + \"!\"\n}\n\n\
print(greet(\"bang\"))\n\n\
// spawn 으로 병렬 — async/await 색칠도, 락도 없이.\n\
let doubled = parallel_map([1, 2, 3, 4], fn(n) { return n * 2 })\n\
print(doubled)\n";

    let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n");
    let gitignore = "bang_modules/\n";

    let files = [
        ("main.bang", main_bang.to_string()),
        ("bang.toml", manifest),
        (".gitignore", gitignore.to_string()),
    ];
    for (fname, content) in &files {
        if let Err(e) = std::fs::write(dir.join(fname), content) {
            eprintln!("{fname} 쓰기 실패: {e}");
            process::exit(1);
        }
    }

    println!("새 프로젝트 생성: {}", dir.display());
    println!("  {name}/main.bang, bang.toml, .gitignore");
    println!();
    println!("시작하기:");
    println!("  cd {}", dir.display());
    println!("  bang main.bang");
}

fn cmd_add(args: &[String]) {
    if args.len() < 2 {
        eprintln!("사용법: bang add <이름> <git-url[@rev]>");
        process::exit(1);
    }
    let name = args[0].clone();
    let spec = &args[1];
    let (url, rev) = match spec.rsplit_once('@') {
        Some((u, r)) if u.contains("://") => (u.to_string(), Some(r.to_string())),
        _ => (spec.clone(), None),
    };
    let dep = bang::pkg::Dependency { name: name.clone(), url, rev };

    if !install_dep(&dep) {
        process::exit(1);
    }
    let mut deps = read_deps();
    bang::pkg::upsert(&mut deps, dep);
    write_deps(&deps);
    println!("{MANIFEST} 에 '{name}' 추가됨");
}

fn cmd_install(_args: &[String]) {
    let deps = read_deps();
    if deps.is_empty() {
        println!("{MANIFEST} 에 의존성이 없습니다.");
        return;
    }
    let mut failed = 0;
    for dep in &deps {
        if !install_dep(dep) {
            failed += 1;
        }
    }
    if failed > 0 {
        eprintln!("{failed}개 의존성 설치 실패");
        process::exit(1);
    }
    println!("모든 의존성 설치 완료 ({}개)", deps.len());
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

// ============================================================================
// bang test / bang fmt (Phase 49)
// ============================================================================

/// 경로 목록 → .bang 파일 목록 (디렉터리는 재귀, bang_modules/.git/target 제외, 정렬).
/// `only_tests`면 디렉터리 안에서는 *_test.bang / test_*.bang만 고른다 (직접 준 파일은 그대로).
fn collect_bang_files(paths: &[String], only_tests: bool) -> Vec<std::path::PathBuf> {
    fn is_test_file(p: &std::path::Path) -> bool {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        name.ends_with("_test.bang") || (name.starts_with("test_") && name.ends_with(".bang"))
    }
    fn walk(dir: &std::path::Path, only_tests: bool, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_dir() {
                if !matches!(name, "bang_modules" | ".git" | "target" | "node_modules") {
                    walk(&p, only_tests, out);
                }
            } else if name.ends_with(".bang") && (!only_tests || is_test_file(&p)) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    let roots: Vec<String> = if paths.is_empty() { vec![".".to_string()] } else { paths.to_vec() };
    for r in &roots {
        let p = std::path::Path::new(r);
        if p.is_dir() {
            walk(p, only_tests, &mut out);
        } else if p.is_file() {
            out.push(p.to_path_buf());
        } else {
            eprintln!("오류: '{r}' 파일 또는 디렉터리가 없습니다");
            process::exit(1);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// bang test [경로...] — 테스트 파일의 최상위를 실행한 뒤 이름이 test_로 시작하는 최상위 함수를
/// 하나씩 격리 실행한다. 함수가 에러 없이 끝나면 통과 (assert / assert_eq / throw로 실패).
fn cmd_test(args: &[String]) {
    let files = collect_bang_files(args, true);
    if files.is_empty() {
        eprintln!("테스트 파일이 없습니다 (*_test.bang 또는 test_*.bang)");
        process::exit(1);
    }
    let started = std::time::Instant::now();
    let (mut passed, mut failed, mut broken_files) = (0usize, 0usize, 0usize);
    for file in &files {
        let path = file.display().to_string();
        println!("{path}");
        let source = read_source(&path);
        // 파일 단위 실패(파스·리졸브·컴파일·최상위 실행 에러)는 그 파일의 테스트를 건너뛴다
        let tokens = match Lexer::new(&source).tokenize() {
            Ok(t) => t,
            Err(errs) => {
                for e in errs { println!("  오류  {e}"); }
                broken_files += 1;
                continue;
            }
        };
        let prog = match Parser::new(tokens).parse() {
            Ok(p) => p,
            Err(errs) => {
                for e in errs { println!("  오류  {e}"); }
                broken_files += 1;
                continue;
            }
        };
        let resolved = bang::resolver::resolve(&prog);
        if !resolved.errors.is_empty() {
            for e in &resolved.errors { println!("  오류  {e}"); }
            broken_files += 1;
            continue;
        }
        let out = match bang::compiler::compile_main(&prog, &path) {
            Ok(o) => o,
            Err(errs) => {
                for e in &errs { println!("  오류  {e}"); }
                broken_files += 1;
                continue;
            }
        };
        let mut vm = bang::vm::Vm::new(out.global_count as usize, bang::vm::Output::Stdout);
        if let Err(e) = vm.run(out.main_fn) {
            println!("  오류  최상위 실행 실패: {e}");
            broken_files += 1;
            continue;
        }
        // 선언 순서(전역 슬롯 순)대로 test_ 함수 실행
        let mut tests: Vec<(&String, u16)> = out.global_names.iter()
            .filter(|(n, _)| n.starts_with("test_"))
            .map(|(n, s)| (n, *s))
            .collect();
        tests.sort_by_key(|(_, s)| *s);
        if tests.is_empty() {
            println!("  (test_로 시작하는 함수 없음)");
        }
        for (name, slot) in tests {
            match vm.run_global_fn_isolated(slot as usize) {
                None => continue, // 함수가 아닌 test_ 이름
                Some(Ok(_)) => {
                    passed += 1;
                    println!("  ok    {name}");
                }
                Some(Err(e)) => {
                    failed += 1;
                    println!("  FAIL  {name}");
                    println!("{}", runtime_error_text(&e, &source)
                        .lines().map(|l| format!("        {l}")).collect::<Vec<_>>().join("\n"));
                }
            }
        }
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let file_note = if broken_files > 0 { format!(", 파일 오류 {broken_files}개") } else { String::new() };
    println!("\n결과: {passed}개 통과, {failed}개 실패{file_note} ({}개 파일, {ms:.0}ms)", files.len());
    if failed > 0 || broken_files > 0 {
        process::exit(1);
    }
}

/// bang fmt [--check] [경로...] — 들여쓰기 정규화 (src/fmt.rs). 줄 안의 내용은 바꾸지 않는다.
fn cmd_fmt(args: &[String]) {
    let check = args.iter().any(|a| a == "--check");
    let paths: Vec<String> = args.iter().filter(|a| a.as_str() != "--check").cloned().collect();
    let files = collect_bang_files(&paths, false);
    let (mut changed, mut errors) = (0usize, 0usize);
    for file in &files {
        let path = file.display().to_string();
        let source = read_source(&path);
        match bang::fmt::format_source(&source) {
            Ok(formatted) if formatted == source => {}
            Ok(formatted) => {
                changed += 1;
                if check {
                    println!("포맷 필요: {path}");
                } else if let Err(e) = fs::write(file, formatted) {
                    eprintln!("오류: '{path}' 쓰기 실패: {e}");
                    errors += 1;
                } else {
                    println!("포맷함: {path}");
                }
            }
            Err(e) => {
                eprintln!("건너뜀: {path} — {e}");
                errors += 1;
            }
        }
    }
    if check && changed > 0 {
        eprintln!("{changed}개 파일이 포맷되지 않았습니다 (bang fmt로 고치세요)");
        process::exit(1);
    }
    if errors > 0 {
        process::exit(1);
    }
}
