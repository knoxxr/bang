// Bang 통합 테스트 — 바이트코드 VM (Phase 5)
// 인터프리터(Phase 3) 출력과 비교하여 일치 여부 검증

use bang::compiler::compile;
use bang::interpreter::Interpreter;
use bang::lexer::Lexer;
use bang::parser::Parser;
use bang::vm::Vm;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

// ============================================================================
// 헬퍼
// ============================================================================

fn lex_parse(source: &str) -> bang::ast::Program {
    let tokens = Lexer::new(source).tokenize().expect("tokenize 실패");
    Parser::new(tokens).parse().expect("parse 실패")
}

/// VM으로 소스 실행 → 출력 줄 목록
fn run_vm(source: &str) -> Vec<String> {
    let prog = lex_parse(source);
    let out = compile(&prog).expect("컴파일 실패");
    let output = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut vm = Vm::new(out.global_count as usize, output.clone());
    vm.run(out.main_fn).expect("VM 실행 실패");
    let lines = output.lock().unwrap().clone();
    lines
}

/// 인터프리터로 소스 실행 → 출력 줄 목록
fn run_interp(source: &str) -> Vec<String> {
    let prog = lex_parse(source);
    let interp = Interpreter::new();
    interp.run(&prog).expect("인터프리터 실행 실패");
    let lines = interp.output.lock().unwrap().clone();
    lines
}

/// examples/ 파일을 VM과 인터프리터 양쪽으로 실행하여 출력 비교
fn diff_file(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name}: 파일 읽기 실패: {e}"));
    let vm_out    = run_vm(&source);
    let interp_out = run_interp(&source);
    assert_eq!(
        vm_out, interp_out,
        "{name}: VM 출력이 인터프리터 출력과 다름\nVM: {vm_out:?}\nInterp: {interp_out:?}"
    );
}

// ============================================================================
// 12개 예제 — differential tests
// ============================================================================

#[test] fn test_vm_hello()          { diff_file("hello.bang"); }
#[test] fn test_vm_fibonacci()      { diff_file("fibonacci.bang"); }
#[test] fn test_vm_recursion()      { diff_file("recursion.bang"); }
#[test] fn test_vm_closures()       { diff_file("closures.bang"); }
#[test] fn test_vm_higher_order()   { diff_file("higher_order.bang"); }
#[test] fn test_vm_conditionals()   { diff_file("conditionals.bang"); }
#[test] fn test_vm_list_traversal() { diff_file("list_traversal.bang"); }
#[test] fn test_vm_map_usage()      { diff_file("map_usage.bang"); }
#[test] fn test_vm_string_ops()     { diff_file("string_ops.bang"); }
#[test] fn test_vm_spawn_basic()    { diff_file("spawn_basic.bang"); }
#[test] fn test_vm_parallel_block() { diff_file("parallel_block.bang"); }
#[test] fn test_vm_channels()       { diff_file("channels.bang"); }

// ============================================================================
// VM 단위 테스트
// ============================================================================

#[test]
fn test_vm_arithmetic() {
    assert_eq!(run_vm("print(1 + 2 * 3)"), vec!["7"]);
    assert_eq!(run_vm("print(10 / 4)"), vec!["2.5"]);
    assert_eq!(run_vm("print(10 % 3)"), vec!["1"]);
    assert_eq!(run_vm("print(-5)"), vec!["-5"]);
}

#[test]
fn test_vm_string_concat() {
    assert_eq!(run_vm(r#"print("hello" + " " + "world")"#), vec!["hello world"]);
}

#[test]
fn test_vm_comparison() {
    assert_eq!(run_vm("print(1 < 2)"), vec!["true"]);
    assert_eq!(run_vm("print(2 == 2)"), vec!["true"]);
    assert_eq!(run_vm("print(3 != 4)"), vec!["true"]);
}

#[test]
fn test_vm_boolean_logic() {
    assert_eq!(run_vm("print(true and false)"), vec!["false"]);
    assert_eq!(run_vm("print(true or false)"), vec!["true"]);
    assert_eq!(run_vm("print(not true)"), vec!["false"]);
}

#[test]
fn test_vm_let_and_print() {
    assert_eq!(run_vm("let x = 42\nprint(x)"), vec!["42"]);
}

#[test]
fn test_vm_if_else() {
    assert_eq!(run_vm("let x = 5\nif x > 3 { print(\"big\") } else { print(\"small\") }"),
               vec!["big"]);
}

#[test]
fn test_vm_while_loop() {
    assert_eq!(run_vm("let i = 0\nwhile i < 3 { print(i)\ni = i + 1 }"),
               vec!["0", "1", "2"]);
}

#[test]
fn test_vm_for_loop() {
    assert_eq!(run_vm("for x in [10, 20, 30] { print(x) }"),
               vec!["10", "20", "30"]);
}

#[test]
fn test_vm_closure_capture() {
    let src = r#"
fn make_adder(n) {
    return fn(x) { return x + n }
}
let add5 = make_adder(5)
print(add5(3))
print(add5(10))
"#;
    assert_eq!(run_vm(src), vec!["8", "15"]);
}

#[test]
fn test_vm_upvalue_mutation() {
    let src = r#"
fn make_counter() {
    let count = 0
    return fn() {
        count = count + 1
        return count
    }
}
let c = make_counter()
print(c())
print(c())
print(c())
"#;
    assert_eq!(run_vm(src), vec!["1", "2", "3"]);
}

#[test]
fn test_vm_list_operations() {
    assert_eq!(run_vm("let a = [1,2,3]\nprint(len(a))"), vec!["3"]);
    assert_eq!(run_vm("print([1,2] + [3,4])"), vec!["[1, 2, 3, 4]"]);
}

#[test]
fn test_vm_map_access() {
    let src = "let m = {\"x\": 1, \"y\": 2}\nprint(m[\"x\"])";
    assert_eq!(run_vm(src), vec!["1"]);
}

#[test]
fn test_vm_builtins() {
    assert_eq!(run_vm("print(str(42))"),    vec!["42"]);
    assert_eq!(run_vm("print(int(3.7))"),   vec!["3"]);
    assert_eq!(run_vm("print(len([1,2]))"), vec!["2"]);
    assert_eq!(run_vm("print(type(42))"),   vec!["Int"]);
}

#[test]
fn test_vm_range() {
    assert_eq!(run_vm("print(range(3))"),    vec!["[0, 1, 2]"]);
    assert_eq!(run_vm("print(range(1,4))"),  vec!["[1, 2, 3]"]);
}

#[test]
fn test_vm_many_allocs() {
    // Stress test: many list allocations to ensure no crash
    let src = r#"
let i = 0
let total = 0
while i < 1000 {
    let lst = [i, i+1, i+2]
    total = total + lst[0]
    i = i + 1
}
print(total)
"#;
    assert_eq!(run_vm(src), vec!["499500"]);
}

// ============================================================================
// try / catch / throw (VM 전용 — Phase 13)
// ============================================================================

#[test]
fn test_vm_try_catch_user_throw() {
    let src = "try {\n throw \"boom\"\n} catch e {\n print(e)\n}";
    assert_eq!(run_vm(src), vec!["boom"]);
}

#[test]
fn test_vm_try_catch_runtime_error() {
    // 0 나눗셈 같은 내장 런타임 에러도 catch 가능 (메시지 문자열로 바인딩)
    let src = "try {\n let x = 1 / 0\n print(x)\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_try_continues_after_catch() {
    let src = "try {\n throw \"x\"\n} catch e {\n print(\"handled\")\n}\nprint(\"after\")";
    assert_eq!(run_vm(src), vec!["handled", "after"]);
}

#[test]
fn test_vm_throw_propagates_from_function() {
    let src = r#"
fn risky(n) {
    if n < 0 {
        throw "negative"
    }
    return n
}
try {
    print(risky(3))
    print(risky(-1))
    print("unreached")
} catch e {
    print(e)
}
"#;
    assert_eq!(run_vm(src), vec!["3", "negative"]);
}

#[test]
fn test_vm_nested_try_rethrow() {
    let src = r#"
try {
    try {
        throw "inner"
    } catch e {
        throw "outer"
    }
} catch e {
    print(e)
}
"#;
    assert_eq!(run_vm(src), vec!["outer"]);
}

#[test]
fn test_vm_throw_non_string_value() {
    let src = "try {\n throw {\"code\": 7}\n} catch e {\n print(e[\"code\"])\n}";
    assert_eq!(run_vm(src), vec!["7"]);
}

#[test]
fn test_vm_break_inside_try_no_leak() {
    // try 안에서 break — 핸들러가 누수되지 않아야 (이후 정상 실행)
    let src = r#"
let i = 0
while i < 3 {
    try {
        if i == 1 {
            break
        }
        print(i)
    } catch e {
        print("never")
    }
    i = i + 1
}
print("done")
"#;
    assert_eq!(run_vm(src), vec!["0", "done"]);
}

#[test]
fn test_vm_uncaught_throw_is_error() {
    let prog = lex_parse("throw \"oops\"");
    let out = compile(&prog).expect("컴파일 실패");
    let output = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut vm = Vm::new(out.global_count as usize, output);
    let result = vm.run(out.main_fn);
    assert!(result.is_err(), "미캐치 throw는 에러여야 함");
    assert!(result.unwrap_err().message.contains("oops"));
}

// ============================================================================
// 값 의미론 + copy-on-write (Arc COW) — Phase 14
// 클론은 Arc 공유(O(1))지만, 변경 시 COW로 관찰상 독립성이 유지되어야 한다.
// ============================================================================

#[test]
fn test_vm_cow_list_alias_isolation() {
    let src = "let a = [1,2,3]\nlet b = a\nb[0] = 99\nprint(a)\nprint(b)";
    assert_eq!(run_vm(src), vec!["[1, 2, 3]", "[99, 2, 3]"]);
}

#[test]
fn test_vm_cow_map_alias_isolation() {
    let src = "let m = {\"k\": 1}\nlet n = m\nn[\"k\"] = 42\nprint(m[\"k\"])\nprint(n[\"k\"])";
    assert_eq!(run_vm(src), vec!["1", "42"]);
}

#[test]
fn test_vm_cow_function_arg_isolation() {
    let src = r#"
fn mutate(lst) {
    lst[0] = -1
    return lst
}
let orig = [10, 20]
let changed = mutate(orig)
print(orig)
print(changed)
"#;
    assert_eq!(run_vm(src), vec!["[10, 20]", "[-1, 20]"]);
}

#[test]
fn test_vm_cow_push_does_not_mutate_source() {
    let src = "let xs = [1, 2]\nlet ys = push(xs, 3)\nprint(xs)\nprint(ys)";
    assert_eq!(run_vm(src), vec!["[1, 2]", "[1, 2, 3]"]);
}

#[test]
fn test_vm_cow_large_data_cheap_clone() {
    // COW면 큰 리스트를 여러 번 전달해도 빠르게 완료 (deep copy면 매우 느림)
    let src = r#"
fn touch(lst) { return len(lst) }
let big = range(5000)
let i = 0
let acc = 0
while i < 5000 {
    acc = acc + touch(big)
    i = i + 1
}
print(acc)
"#;
    assert_eq!(run_vm(src), vec!["25000000"]);
}

// ============================================================================
// 선택적 타입 힌트 (런타임 검증) — Phase 15
// ============================================================================

#[test]
fn test_vm_typed_let_ok() {
    assert_eq!(run_vm("let x: int = 42\nprint(x)"), vec!["42"]);
    assert_eq!(run_vm("let s: str = \"hi\"\nprint(s)"), vec!["hi"]);
}

#[test]
fn test_vm_typed_fn_ok() {
    let src = "fn add(a: int, b: int) -> int { return a + b }\nprint(add(3, 4))";
    assert_eq!(run_vm(src), vec!["7"]);
}

#[test]
fn test_vm_typed_let_mismatch_catchable() {
    let src = "try {\n let x: int = \"s\"\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_typed_param_mismatch_catchable() {
    let src = r#"
fn f(a: int) { return a }
try {
    f("nope")
} catch e {
    print("caught")
}
"#;
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_typed_return_mismatch_catchable() {
    let src = r#"
fn f() -> int { return "nope" }
try {
    f()
} catch e {
    print("caught")
}
"#;
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_any_type_accepts_all() {
    let src = "fn id(v: any) -> any { return v }\nprint(id(1))\nprint(id(\"x\"))";
    assert_eq!(run_vm(src), vec!["1", "x"]);
}

#[test]
fn test_vm_untyped_still_gradual() {
    // 힌트 없는 코드는 영향 없음
    let src = "let x = 5\nfn f(a) { return a }\nprint(f(x))";
    assert_eq!(run_vm(src), vec!["5"]);
}

#[test]
fn test_vm_typed_list_map() {
    let src = "let xs: list = [1,2]\nlet m: map = {\"a\": 1}\nprint(len(xs))\nprint(m[\"a\"])";
    assert_eq!(run_vm(src), vec!["2", "1"]);
}

// ============================================================================
// stdlib 확장 (slice/has/get/merge/repeat/index_of) — Phase 16
// ============================================================================

#[test]
fn test_vm_slice() {
    assert_eq!(run_vm("print(slice([1,2,3,4,5], 1, 4))"), vec!["[2, 3, 4]"]);
    assert_eq!(run_vm("print(slice(\"hello\", 0, 3))"), vec!["hel"]);
    assert_eq!(run_vm("print(slice([1,2,3], 5, 9))"), vec!["[]"]); // 범위 밖 → 빈
}

#[test]
fn test_vm_map_has_get() {
    let src = "let m = {\"a\": 1}\nprint(has(m, \"a\"))\nprint(has(m, \"z\"))\nprint(get(m, \"a\", 0))\nprint(get(m, \"z\", -1))";
    assert_eq!(run_vm(src), vec!["true", "false", "1", "-1"]);
}

#[test]
fn test_vm_merge() {
    assert_eq!(run_vm("print(merge({\"a\": 1}, {\"a\": 9, \"b\": 2})[\"a\"])"), vec!["9"]);
}

#[test]
fn test_vm_repeat() {
    assert_eq!(run_vm("print(repeat(\"ab\", 3))"), vec!["ababab"]);
    assert_eq!(run_vm("print(repeat(\"x\", 0))"), vec![""]);
}

#[test]
fn test_vm_index_of() {
    assert_eq!(run_vm("print(index_of([10,20,30], 20))"), vec!["1"]);
    assert_eq!(run_vm("print(index_of([10,20], 99))"), vec!["-1"]);
}

// ============================================================================
// JSON / 시간 / 난수 — Phase 19
// ============================================================================

#[test]
fn test_vm_json_parse() {
    let src = r#"
let d = json_parse("{\"name\": \"Al\", \"age\": 30, \"tags\": [1, 2]}")
print(d.name)
print(d.age)
print(d.tags)
"#;
    assert_eq!(run_vm(src), vec!["Al", "30", "[1, 2]"]);
}

#[test]
fn test_vm_json_parse_primitives() {
    assert_eq!(run_vm("print(json_parse(\"true\"))"), vec!["true"]);
    assert_eq!(run_vm("print(json_parse(\"null\"))"), vec!["nil"]);
    assert_eq!(run_vm("print(json_parse(\"3.5\"))"), vec!["3.5"]);
    assert_eq!(run_vm("print(json_parse(\"42\"))"), vec!["42"]);
}

#[test]
fn test_vm_json_stringify() {
    // 맵 키는 정렬되어 안정적 출력
    assert_eq!(run_vm("print(json_stringify({\"b\": 2, \"a\": 1}))"), vec!["{\"a\":1,\"b\":2}"]);
    assert_eq!(run_vm("print(json_stringify([1, \"x\", true, nil]))"), vec!["[1,\"x\",true,null]"]);
}

#[test]
fn test_vm_json_roundtrip() {
    let src = r#"
let original = {"id": 7, "items": [1, 2, 3]}
let text = json_stringify(original)
let back = json_parse(text)
print(back.id)
print(back.items)
"#;
    assert_eq!(run_vm(src), vec!["7", "[1, 2, 3]"]);
}

#[test]
fn test_vm_json_stringify_rejects_function() {
    // 함수는 직렬화 불가 → try/catch로 잡힘
    let src = "try {\n json_stringify(fn(){ return 1 })\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_now_ms_and_random() {
    assert_eq!(run_vm("print(now_ms() > 0)"), vec!["true"]);
    assert_eq!(run_vm("let r = random()\nprint(r >= 0.0 and r < 1.0)"), vec!["true"]);
    assert_eq!(run_vm("let n = random_int(5, 5)\nprint(n)"), vec!["5"]); // [5,5] → 5
}

// ============================================================================
// stdlib 폭: list 유틸 / 시간포맷 / 문자 — Phase 20
// ============================================================================

#[test]
fn test_vm_sort_by() {
    let src = "print(sort_by([\"bbb\", \"a\", \"cc\"], fn(s) { return len(s) }))";
    assert_eq!(run_vm(src), vec!["[a, cc, bbb]"]);
}

#[test]
fn test_vm_unique() {
    assert_eq!(run_vm("print(unique([1, 2, 2, 3, 1, 3]))"), vec!["[1, 2, 3]"]);
    assert_eq!(run_vm("print(unique([\"a\", \"a\", \"b\"]))"), vec!["[a, b]"]);
}

#[test]
fn test_vm_format_time() {
    assert_eq!(run_vm("print(format_time(0))"), vec!["1970-01-01 00:00:00"]);
    assert_eq!(run_vm("print(format_time(1700000000000))"), vec!["2023-11-14 22:13:20"]);
}

#[test]
fn test_vm_ord_chr() {
    assert_eq!(run_vm("print(ord(\"A\"))"), vec!["65"]);
    assert_eq!(run_vm("print(chr(97))"), vec!["a"]);
    // 라운드트립
    assert_eq!(run_vm("print(chr(ord(\"Z\")))"), vec!["Z"]);
}

#[test]
fn test_vm_fs_predicates() {
    // file_exists / is_dir 는 환경 비의존 경로로
    assert_eq!(run_vm("print(file_exists(\"/definitely/not/here/xyz\"))"), vec!["false"]);
}

// ============================================================================
// 정규식 — Phase 21
// ============================================================================

#[test]
fn test_vm_regex_match() {
    assert_eq!(run_vm("print(regex_match(\"abc123\", \"[0-9]+\"))"), vec!["true"]);
    assert_eq!(run_vm("print(regex_match(\"hello1\", \"^[a-z]+$\"))"), vec!["false"]);
}

#[test]
fn test_vm_regex_find() {
    assert_eq!(run_vm("print(regex_find(\"order 4521 ok\", \"\\\\d+\"))"), vec!["4521"]);
    assert_eq!(run_vm("print(regex_find(\"abc\", \"\\\\d+\"))"), vec!["nil"]);
}

#[test]
fn test_vm_regex_find_all() {
    assert_eq!(run_vm("print(regex_find_all(\"a1 b22 c333\", \"\\\\d+\"))"), vec!["[1, 22, 333]"]);
}

#[test]
fn test_vm_regex_replace() {
    assert_eq!(run_vm("print(regex_replace(\"foo bar baz\", \"ba.\", \"X\"))"), vec!["foo X X"]);
}

#[test]
fn test_vm_regex_brace_and_date() {
    assert_eq!(run_vm("print(regex_find(\"2023-11-14\", \"\\\\d{4}\"))"), vec!["2023"]);
    let src = "print(regex_match(\"2023-11-14\", \"^\\\\d{4}-\\\\d{2}-\\\\d{2}$\"))";
    assert_eq!(run_vm(src), vec!["true"]);
}

#[test]
fn test_vm_regex_groups() {
    let src = r#"
let g = regex_groups("2023-11-14", "(\\d{4})-(\\d{2})-(\\d{2})")
print(g[0])
print(g[1])
print(g[3])
"#;
    assert_eq!(run_vm(src), vec!["2023-11-14", "2023", "14"]);
    assert_eq!(run_vm("print(regex_groups(\"x\", \"(\\\\d+)\"))"), vec!["nil"]);
}

#[test]
fn test_vm_regex_bad_pattern_catchable() {
    let src = "try {\n regex_match(\"x\", \"[unclosed\")\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_interp_flag_still_works() {
    // Ensure Phase 3 interpreter is accessible (not removed)
    // We test this by directly running the interpreter
    let src = "print(\"interp ok\")";
    assert_eq!(run_interp(src), vec!["interp ok"]);
}
