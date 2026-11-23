// Bang 통합 테스트 — 바이트코드 VM (Phase 5)
// 예제는 상단 "// 기대 출력:" 주석과 비교 (Phase 44: 인터프리터 제거 후 주석이 정답지)

use bang::compiler::compile;
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

/// 예제 상단의 "// 기대 출력:" 주석 블록 → 기대 출력 줄 목록.
/// (CLAUDE.md 규약: examples/의 기대 출력 주석이 통합 테스트 정답지)
fn expected_from_header(source: &str) -> Option<Vec<String>> {
    let mut lines = source.lines();
    lines.by_ref().find(|l| l.trim() == "// 기대 출력:")?;
    Some(
        lines
            .take_while(|l| l.starts_with("//"))
            .map(|l| l.strip_prefix("// ").unwrap_or(l.trim_start_matches('/')).to_string())
            .collect(),
    )
}

/// examples/ 파일을 VM으로 실행하여 상단 기대 출력 주석과 비교.
/// (Phase 44 전에는 트리 워킹 인터프리터 출력과 비교했다 — 인터프리터 제거 후 주석이 정답지)
fn diff_file(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name}: 파일 읽기 실패: {e}"))
        .replace("\r\n", "\n");
    let expected = expected_from_header(&source)
        .unwrap_or_else(|| panic!("{name}: '// 기대 출력:' 주석이 없음"));
    let vm_out = run_vm(&source);
    assert_eq!(
        vm_out, expected,
        "{name}: VM 출력이 기대 출력 주석과 다름\nVM: {vm_out:?}\n기대: {expected:?}"
    );
}

// ============================================================================
// 12개 예제 — 기대 출력 주석 검증
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
// 기대 출력 주석이 있는데 빠져 있던 예제들 (Phase 48에서 추가)
#[test] fn test_vm_error_handling()  { diff_file("error_handling.bang"); }
#[test] fn test_vm_type_hints()      { diff_file("type_hints.bang"); }
#[test] fn test_vm_json_demo()       { diff_file("json_demo.bang"); }
#[test] fn test_vm_regex_demo()      { diff_file("regex_demo.bang"); }
#[test] fn test_vm_concurrency_demo() { diff_file("concurrency_demo.bang"); }
#[test] fn test_vm_event_loop()      { diff_file("event_loop.bang"); }

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
    // 맵 키는 삽입 순서 그대로 (Phase 51 — 이전에는 정렬)
    assert_eq!(run_vm("print(json_stringify({\"b\": 2, \"a\": 1}))"), vec!["{\"b\":2,\"a\":1}"]);
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

// ============================================================================
// math + 집합 연산 — Phase 24
// ============================================================================

#[test]
fn test_vm_math_basics() {
    assert_eq!(run_vm("print(gcd(12, 18))"), vec!["6"]);
    assert_eq!(run_vm("print(clamp(15, 0, 10))"), vec!["10"]);
    assert_eq!(run_vm("print(clamp(5, 0, 10))"), vec!["5"]);
    assert_eq!(run_vm("print(sign(-3))"), vec!["-1"]);
    assert_eq!(run_vm("print(sign(0))"), vec!["0"]);
    assert_eq!(run_vm("print(sign(7))"), vec!["1"]);
}

#[test]
fn test_vm_math_trig_log() {
    assert_eq!(run_vm("print(round(sin(0.0)))"), vec!["0"]);
    assert_eq!(run_vm("print(round(cos(0.0)))"), vec!["1"]);
    assert_eq!(run_vm("print(round(exp(0.0)))"), vec!["1"]);
    assert_eq!(run_vm("print(round(log(exp(1.0))))"), vec!["1"]);
    assert_eq!(run_vm("print(round(pi() * 100.0))"), vec!["314"]);
}

#[test]
fn test_vm_set_ops() {
    assert_eq!(run_vm("print(union([1,2,3], [3,4,5]))"), vec!["[1, 2, 3, 4, 5]"]);
    assert_eq!(run_vm("print(intersect([1,2,3], [2,3,4]))"), vec!["[2, 3]"]);
    assert_eq!(run_vm("print(difference([1,2,3], [2]))"), vec!["[1, 3]"]);
    // 중복 제거
    assert_eq!(run_vm("print(union([1,1,2], [2,2,3]))"), vec!["[1, 2, 3]"]);
}

// ============================================================================
// 채널 구조화 데이터 전송 — Phase 29 (회귀: 이전엔 List/Map이 Nil로 떨어짐)
// ============================================================================

#[test]
fn test_vm_channel_carries_map() {
    let src = r#"
let ch = channel()
send(ch, {"kind": "login", "id": 7})
close(ch)
let msg = recv(ch)
print(msg.kind)
print(msg.id)
"#;
    assert_eq!(run_vm(src), vec!["login", "7"]);
}

#[test]
fn test_vm_channel_carries_list() {
    let src = r#"
let ch = channel()
send(ch, [1, 2, 3])
close(ch)
let xs = recv(ch)
print(sum(xs))
"#;
    assert_eq!(run_vm(src), vec!["6"]);
}

#[test]
fn test_vm_event_dispatch_over_channel() {
    // spawn된 소비자가 채널로 받은 맵을 필드 접근 + 핸들러 디스패치
    let src = r#"
let handlers = {"a": fn(d) { return d * 2 }}
let bus = channel()
let out = channel()
fn loop_ev(ch, o) {
    for ev in ch {
        let h = get(handlers, ev.kind, nil)
        if h != nil {
            send(o, h(ev.val))
        }
    }
    close(o)
}
spawn loop_ev(bus, out)
send(bus, {"kind": "a", "val": 21})
close(bus)
print(recv(out))
"#;
    assert_eq!(run_vm(src), vec!["42"]);
}

// ============================================================================
// select — 멀티 채널 대기 — Phase 30
// ============================================================================

#[test]
fn test_vm_select_ready_channel() {
    // a에 값이 있으면 [0, 값] 반환 (a를 먼저 검사하므로 결정적)
    let src = r#"
let a = channel()
let b = channel()
send(a, "x")
close(a)
close(b)
let r = select([a, b])
print(r[0])
print(r[1])
"#;
    assert_eq!(run_vm(src), vec!["0", "x"]);
}

#[test]
fn test_vm_select_all_closed_nil() {
    let src = r#"
let a = channel()
let b = channel()
close(a)
close(b)
print(select([a, b]))
"#;
    assert_eq!(run_vm(src), vec!["nil"]);
}

// ============================================================================
// 상수 풀 인덱스 폭 — Phase 32 (회귀: u8 wrap-around로 256↑ 상수가 엉뚱한 값)
// ============================================================================

#[test]
fn test_vm_constant_pool_over_256() {
    // 서로 다른 문자열 상수 270개 선언 후 256 이후 인덱스를 출력 → 정확해야 함
    let mut src = String::new();
    for i in 0..270 {
        src.push_str(&format!("let s{i} = \"VAL_{i}\"\n"));
    }
    src.push_str("print(s260)\nprint(s269)\n");
    assert_eq!(run_vm(&src), vec!["VAL_260", "VAL_269"]);
}

#[test]
fn test_vm_constant_dedup() {
    // 같은 상수는 풀에서 1슬롯만 차지 (중복 제거) — 300번 같은 값 써도 정상
    let mut src = String::from("let total = 0\n");
    for _ in 0..300 {
        src.push_str("total = total + 7\n");
    }
    src.push_str("print(total)\n");
    assert_eq!(run_vm(&src), vec!["2100"]);
}

// ============================================================================
// 정수 오버플로 의미 통일 — Phase 36
// (회귀: debug는 panic, release는 조용히 wrap이던 것을 catch 가능한 에러로 통일)
// ============================================================================

#[test]
fn test_vm_int_overflow_catchable() {
    let src = r#"
let max = 9223372036854775807
let min = -9223372036854775807 - 1
try { print(max + 1) } catch e { print("add") }
try { print(min - 1) } catch e { print("sub") }
try { print(max * 2) } catch e { print("mul") }
try { print(idiv(min, -1)) } catch e { print("div") }
try { print(min % -1) } catch e { print("mod") }
try { print(-min) } catch e { print("neg") }
try { print(abs(min)) } catch e { print("abs") }
try { print(sum([max, 1])) } catch e { print("sum") }
try { print(gcd(min, min)) } catch e { print("gcd") }
print(max + 0)
"#;
    assert_eq!(run_vm(src), vec![
        "add", "sub", "mul", "div", "mod", "neg", "abs", "sum", "gcd",
        "9223372036854775807",
    ]);
}

#[test]
fn test_vm_const_fold_overflow_not_folded() {
    // 리터럴끼리의 오버플로도 (폴딩 포기 후) 런타임 에러로 잡힘
    let src = "try {\n print(9223372036854775807 + 1)\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_mod_zero_still_errors() {
    let src = "try {\n print(10 % 0)\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

// ============================================================================
// 스택 오버플로(abort) → 잡히는 에러 — Phase 39
// (회귀: 깊은 중첩/순환이 fatal stack overflow로 프로세스를 죽이던 문제)
// ============================================================================

#[test]
fn test_vm_deep_json_nesting_catchable() {
    let opens = "[".repeat(5000);
    let closes = "]".repeat(5000);
    let src = format!(
        "let s = \"{opens}{closes}\"\ntry {{\n json_parse(s)\n print(\"no\")\n}} catch e {{\n print(\"caught\")\n}}"
    );
    assert_eq!(run_vm(&src), vec!["caught"]);
}

#[test]
fn test_vm_deep_regex_group_nesting_catchable() {
    let pat = format!("{}x{}", "(".repeat(5000), ")".repeat(5000));
    let src = format!(
        "try {{\n regex_match(\"x\", \"{pat}\")\n}} catch e {{\n print(\"caught\")\n}}"
    );
    assert_eq!(run_vm(&src), vec!["caught"]);
}

#[test]
fn test_vm_regex_huge_repeat_catchable() {
    let src = "try {\n regex_match(\"x\", \"a{999999}\")\n} catch e {\n print(\"caught\")\n}";
    assert_eq!(run_vm(src), vec!["caught"]);
}

#[test]
fn test_vm_reasonable_nesting_still_works() {
    // 정상적인 깊이(100겹)는 여전히 통과
    let src = format!("print({}1{})", "(".repeat(100), ")".repeat(100));
    assert_eq!(run_vm(&src), vec!["1"]);
}

// ============================================================================
// Phase 45 — 제자리 대입(COW 실효화) · 중첩 경로 대입 · parallel_map 병렬
// ============================================================================

#[test]
fn test_vm_nested_path_assign() {
    let src = "let m = {\"a\": [1, 2], \"b\": {\"c\": 1}}\nm[\"a\"][0] = 9\nm.b.c = 5\nm.b[\"d\"] = 6\nprint(m)\nlet g = [[0, 0], [0, 0]]\ng[1][0] = 3\ng[-1][-1] = 4\nprint(g)";
    assert_eq!(run_vm(src), vec!["{a: [9, 2], b: {c: 5, d: 6}}", "[[0, 0], [3, 4]]"]);
}

#[test]
fn test_vm_path_assign_keeps_value_semantics() {
    // 별칭·인자·클로저 캡처는 여전히 격리 (제자리 변경은 관찰 불가능해야 함)
    let src = "let a = {\"k\": [1]}\nlet b = a\na.k[0] = 2\nprint(b)\nfn f(v) {\n v.k[0] = 100\n return v\n}\nprint(f(a))\nprint(a)\nlet xs = [1]\nlet ys = xs\nxs = push(xs, 2)\nprint(ys)";
    assert_eq!(run_vm(src), vec!["{k: [1]}", "{k: [100]}", "{k: [2]}", "[1]"]);
}

#[test]
fn test_vm_path_assign_errors_leave_value_intact() {
    let src = "let m = {\"a\": [1]}\ntry { m[\"zz\"][0] = 1 } catch e { print(e.message) }\ntry { m.a[5] = 1 } catch e { print(e.message) }\ntry { m.a[\"x\"] = 1 } catch e { print(e.message) }\nprint(m)";
    assert_eq!(
        run_vm(src),
        vec![
            "인덱스 대입: 맵에 키 'zz' 없음",
            "인덱스 범위 초과: 5 (길이 1)",
            "인덱스 대입: List 인덱스는 Int여야 함, Str 발견",
            "{a: [1]}",
        ]
    );
}

#[test]
fn test_vm_path_assign_root_must_be_variable() {
    let prog = lex_parse("f()[0] = 1");
    assert!(compile(&prog).is_err());
}

#[test]
fn test_vm_assign_expression_value_is_rhs() {
    assert_eq!(run_vm("let a = [1, 2]\nprint(a[0] = 7)\nprint(a)"), vec!["7", "[7, 2]"]);
}

#[test]
fn test_vm_in_place_updates_are_linear() {
    // 이전: 원소 대입·맵 삽입·push 누적이 매번 전체 복사(O(n²)) — 2만 회에 수 초.
    // 수정 후 선형이어야 한다. 넉넉한 상한으로 회귀만 잡는다 (debug 빌드 기준).
    let src = "let n = 20000\nlet a = range(n)\nfor i in range(n) { a[i] = i * 2 }\nlet m = {}\nfor i in range(n) { m[str(i)] = i }\nlet xs = []\nfor i in range(n) { xs = push(xs, i) }\nlet ys = []\nfor i in range(n) { ys = ys + [i] }\nprint(a[n - 1] + len(m) + len(xs) + len(ys))";
    let t = std::time::Instant::now();
    assert_eq!(run_vm(src), vec!["99998"]);
    assert!(t.elapsed().as_secs_f64() < 10.0, "제자리 갱신이 선형이 아님: {:?}", t.elapsed());
}

#[test]
fn test_vm_update_var_reassigned_during_rhs() {
    // x = push(x, e)에서 e 평가 중 x가 재대입되면 원래 의미(먼저 읽은 x 기준)를 따른다
    let src = "let xs = [1]\nfn g() {\n xs = [9]\n return 2\n}\nxs = push(xs, g())\nprint(xs)\nlet n = 1\nn = n + 1\nlet s = \"a\"\ns = s + \"b\"\nprint(n)\nprint(s)";
    assert_eq!(run_vm(src), vec!["[1, 2]", "2", "ab"]);
}

#[test]
fn test_vm_parallel_map_is_parallel() {
    let src = "let t = now_ms()\nlet r = parallel_map([1, 2, 3, 4, 5, 6, 7, 8], fn(x) {\n sleep(200)\n return x * 10\n})\nprint(r)\nprint(now_ms() - t < 1200)";
    assert_eq!(run_vm(src), vec!["[10, 20, 30, 40, 50, 60, 70, 80]", "true"]);
}

#[test]
fn test_vm_parallel_map_order_errors_and_builtins() {
    let src = "print(parallel_map(range(1000), fn(x) { return x * 2 })[999])\nprint(parallel_map([1, 2], str))\nprint(parallel_map([], fn(x) { return x }))\ntry {\n parallel_map([1, 0, 2], fn(x) { return 10 / x })\n} catch e {\n print(e.kind, e.message)\n}";
    assert_eq!(run_vm(src), vec!["1998", "[1, 2]", "[]", "zero_division 0으로 나눌 수 없음"]);
}

// ============================================================================
// Phase 46 — 문법 편의: 복합 대입·in·순회 확장·슬라이스·리터럴·주석·한글 식별자
// ============================================================================

#[test]
fn test_vm_compound_assign() {
    let src = "let x = 10\nx += 5\nx -= 1\nx *= 2\nx /= 4\nprint(x)\nlet n = 7\nn %= 4\nprint(n)\nlet s = \"a\"\ns += \"b\"\nprint(s)\nlet xs = [1]\nxs += [2]\nprint(xs)";
    assert_eq!(run_vm(src), vec!["7.0", "3", "ab", "[1, 2]"]);
}

#[test]
fn test_vm_compound_assign_path_evaluates_key_once() {
    let src = "let calls = 0\nfn k() {\n calls += 1\n return \"a\"\n}\nlet m = {\"a\": 1, \"b\": [1, 2]}\nm[k()] += 10\nm.b[-1] *= 7\nprint(m)\nprint(calls)\ntry { m[\"zz\"] += 1 } catch e { print(e.message) }\nprint(m)";
    assert_eq!(
        run_vm(src),
        vec!["{a: 11, b: [1, 14]}", "1", "복합 대입: 맵에 키 'zz' 없음 (get(m, k, 기본값) 사용)", "{a: 11, b: [1, 14]}"]
    );
}

#[test]
fn test_vm_in_operator() {
    let src = "let m = {\"a\": 1}\nprint(2 in [1, 2])\nprint(5 not in [1, 2])\nprint(\"a\" in m)\nprint(\"z\" not in m)\nprint(\"ell\" in \"hello\")\nprint([1] in [[1], 2])\nprint(255 in bytes_from_hex(\"00ff\"))\ntry { print(1 in 5) } catch e { print(e.message) }";
    assert_eq!(
        run_vm(src),
        vec!["true", "true", "true", "true", "true", "true", "true", "in: Int 안에서 Int 찾기 지원 안 됨"]
    );
}

#[test]
fn test_vm_for_over_map_str_bytes_and_pairs() {
    let src = "for k in {\"b\": 2, \"a\": 1} { print(k) }\nfor k, v in {\"b\": 2, \"a\": 1} { print(k + str(v)) }\nfor c in \"한글\" { print(c) }\nfor x in bytes_from_hex(\"0aff\") { print(x) }\nfor i, v in enumerate([\"x\", \"y\"]) { print(str(i) + v) }\ntry {\n for p, q in [[1, 2, 3]] { print(p + q) }\n} catch e { print(e.message) }";
    assert_eq!(
        run_vm(src),
        vec!["b", "a", "b2", "a1", "한", "글", "10", "255", "0x", "1y", "분해: 값 2개 기대, 3개 발견"]
    );
}

#[test]
fn test_vm_slice_syntax() {
    let src = "let a = [0, 1, 2, 3, 4, 5]\nprint(a[1:3])\nprint(a[:2])\nprint(a[4:])\nprint(a[-2:])\nprint(a[:-4])\nprint(a[:])\nprint(a[10:20])\nprint(a[4:1])\nprint(\"héllo\"[1:3])\nprint(bytes_to_hex(bytes_from_hex(\"0102ff\")[1:]))\ntry { print(a[\"x\":]) } catch e { print(e.message) }";
    assert_eq!(
        run_vm(src),
        vec!["[1, 2]", "[0, 1]", "[4, 5]", "[4, 5]", "[0, 1]", "[0, 1, 2, 3, 4, 5]", "[]", "[]", "él", "02ff",
             "슬라이스 경계는 Int 또는 생략이어야 함, Str 발견"]
    );
}

#[test]
fn test_vm_literals_comments_unicode_idents() {
    let src = "/* 블록\n주석 */\nlet 이름 = \"방\"\nlet 합 = 1_000 + 0xff + 0b101 + 0o17 /* 인라인 */\nprint(이름 + str(합))\nprint(1e3)\nlet s = \"\"\"a\n\"b\" \"\"\"\nprint(s)";
    assert_eq!(run_vm(src), vec!["방1275", "1000.0", "a\n\"b\" "]);
}

// ============================================================================
// Phase 47 — 메서드 문법(UFCS)·기본값 매개변수·분해 선언·if 식
// ============================================================================

#[test]
fn test_vm_default_params() {
    let src = "fn greet(name, g = \"안녕\", p = \"!\") {\n return g + \", \" + name + p\n}\nprint(greet(\"방\"))\nprint(greet(\"방\", \"hi\"))\nprint(greet(\"방\", \"hi\", \"?\"))\nfn acc(x, xs = []) {\n xs += [x]\n return xs\n}\nprint(acc(1))\nprint(acc(2))\nfn scale(v, k = v * 2) { return k }\nprint(scale(5))\nprint(scale(5, nil))\nlet h = fn(a, b = 10) { return a + b }\nprint(parallel_map([1, 2], h))";
    // 기본값은 호출마다 새로 평가(Python의 가변 기본값 함정 없음), 명시적 nil은 기본값으로 바뀌지 않음
    assert_eq!(run_vm(src), vec!["안녕, 방!", "hi, 방!", "hi, 방?", "[1]", "[2]", "10", "nil", "[11, 12]"]);
}

#[test]
fn test_vm_default_params_runtime_arity_error() {
    // 정적 검사를 피하는 간접 호출 → 런타임 에러 메시지에 범위 표시
    let src = "let fs = [fn(a, b = 1) { return a + b }]\ntry { fs[0]() } catch e { print(e.message) }\ntry { fs[0](1, 2, 3) } catch e { print(e.message) }";
    assert_eq!(run_vm(src), vec!["인자 개수 불일치: 1~2개 기대, 0개 전달", "인자 개수 불일치: 1~2개 기대, 3개 전달"]);
}

#[test]
fn test_vm_let_unpack() {
    let src = "let [a, b] = [1, 2]\nprint(a + b)\nfn pair() { return [\"x\", 3] }\nfn use_pair() {\n let [name, n] = pair()\n return name + str(n)\n}\nprint(use_pair())\ntry {\n let [p, q] = [1, 2, 3]\n print(p + q)\n} catch e { print(e.message) }\ntry {\n let [p] = \"ab\"\n print(p)\n} catch e { print(e.message) }";
    assert_eq!(run_vm(src), vec!["3", "x3", "분해: 값 2개 기대, 3개 발견", "분해: List 필요, Str 발견"]);
}

#[test]
fn test_vm_if_expression() {
    let src = "let a = 3\nprint(if a > 0 { \"양\" } else { \"음\" })\nlet w = if a > 5 { \"big\" } else if a > 0 { \"small\" } else { \"neg\" }\nprint(w)\nlet z = if true {\n let t = 40\n t + 2\n} else { 0 }\nprint(z)\nfn sign_of(n) { return if n < 0 { -1 } else if n == 0 { 0 } else { 1 } }\nprint([sign_of(-5), sign_of(0), sign_of(9)])";
    assert_eq!(run_vm(src), vec!["양", "small", "42", "[-1, 0, 1]"]);
}

#[test]
fn test_vm_if_expression_requires_else_and_value() {
    for src in ["let x = if true { 1 }", "let x = if true { let y = 1 } else { 2 }"] {
        let tokens = Lexer::new(src).tokenize().expect("tokenize");
        let parsed = Parser::new(tokens).parse();
        let failed = match parsed {
            Err(_) => true,
            Ok(prog) => compile(&prog).is_err(),
        };
        assert!(failed, "{src} 는 에러여야 함");
    }
}

#[test]
fn test_vm_method_call_syntax() {
    let src = "let s = \"Hello\"\nprint(s.upper())\nprint(s.lower().replace(\"l\", \"L\"))\nlet xs = [3, 1, 2]\nprint(xs.sort())\nprint(xs)\nprint(xs.len())\nprint({\"a\": 1}.keys())\nprint(\"a,b\".split(\",\"))\nprint([1, 2, 3].map(fn(x) { return x * 2 }).filter(fn(x) { return x > 2 }))\nxs = xs.push(9)\nprint(xs)";
    assert_eq!(run_vm(src), vec!["HELLO", "heLLo", "[1, 2, 3]", "[3, 1, 2]", "3", "[a]", "[a, b]", "[4, 6]", "[3, 1, 2, 9]"]);
}

#[test]
fn test_vm_method_call_map_member_wins_and_errors() {
    let src = "let obj = {\"push\": fn(x) { return x * 100 }, \"n\": 1}\nprint(obj.push(2))\nobj.push(3)\nlet xs = [1]\ntry { xs.push(2) } catch e { print(e.message) }\nprint(xs)\ntry { print([1].frob()) } catch e { print(e.message) }\nlet f = spawn \"future\".upper()\nprint(f)";
    assert_eq!(
        run_vm(src),
        vec![
            "200",
            "x.push(...)의 결과가 버려짐 — push은(는) 새 값을 돌려줄 뿐 x를 바꾸지 않습니다. x = x.push(...)처럼 대입하세요 (리스트 추가는 xs += [v])",
            "[1]",
            "필드 접근: List 에 필드 'frob' 없음",
            "FUTURE",
        ]
    );
}

#[test]
fn test_vm_map_equality_is_structural() {
    // Phase 49 이전: 맵 ==는 항상 false였다 (값 의미론 위반)
    let src = "print({\"a\": 1, \"b\": [1]} == {\"b\": [1], \"a\": 1})\nprint({\"a\": 1} == {\"a\": 2})\nprint({\"a\": 1} == {\"a\": 1, \"b\": 2})\nprint({\"k\": 1} in [{\"k\": 1}])\nprint(unique([{\"a\": 1}, {\"a\": 1}]))\nlet f = fn() { return 1 }\nlet g = f\nprint(f == g)\nprint(f == fn() { return 1 })";
    assert_eq!(run_vm(src), vec!["true", "false", "false", "true", "[{a: 1}]", "true", "false"]);
}

#[test]
fn test_vm_assert_eq() {
    let src = "assert_eq([1, {\"a\": \"x\"}], [1, {\"a\": \"x\"}])\ntry { assert_eq(\"1\", 1) } catch e { print(e.message) }\ntry { assert_eq([1, 2], [1]) } catch e { print(e.message) }";
    assert_eq!(run_vm(src), vec!["assert_eq 실패: 기대 1, 실제 \"1\"", "assert_eq 실패: 기대 [1], 실제 [1, 2]"]);
}

// ============================================================================
// Phase 48 — 에러 모델: catch e = {kind, message, line, col}, 없는 키는 에러
// ============================================================================

#[test]
fn test_vm_caught_builtin_error_is_map() {
    let src = "try {\n let x = 1 / 0\n} catch e {\n print(e.kind)\n print(e.message)\n print(e.line, e.col)\n print(type(e))\n}\ntry { throw \"사용자\" } catch e { print(e) }\ntry { throw {\"code\": 7} } catch e { print(e.code) }";
    assert_eq!(run_vm(src), vec!["zero_division", "0으로 나눌 수 없음", "2 10", "Map", "사용자", "7"]);
}

#[test]
fn test_vm_error_kinds() {
    let cases = [
        ("[1][5]", "index"),
        ("{\"a\": 1}[\"b\"]", "key"),
        ("{\"a\": 1}.b", "key"),
        ("1 + \"a\"", "type"),
        ("9223372036854775807 + 1", "overflow"),
        ("10 % 0", "zero_division"),
        ("len(1, 2)", "argument"),
        ("json_parse(\"{bad\")", "value"),
        ("read_file(\"/없는/경로/x.txt\")", "io"),
        ("assert(false)", "assertion"),
        ("5()", "type"),
    ];
    for (expr, want) in cases {
        let src = format!("let f = fn() {{ return {expr} }}\ntry {{ f() }} catch e {{ print(e.kind) }}");
        assert_eq!(run_vm(&src), vec![want.to_string()], "{expr}");
    }
}

#[test]
fn test_vm_missing_key_is_error_and_get_has_still_work() {
    let src = "let m = {\"a\": 1}\nprint(get(m, \"b\", 0))\nprint(has(m, \"b\"))\nprint(\"b\" in m)\ntry { print(m[\"b\"]) } catch e { print(e.message) }\ntry { print(m.b) } catch e { print(e.kind) }\nprint(m.a)";
    assert_eq!(
        run_vm(src),
        vec!["0", "false", "false", "맵에 키 'b' 없음 (없을 수 있으면 get(m, k, 기본값) 또는 has(m, k))", "key", "1"]
    );
}

#[test]
fn test_vm_rethrown_builtin_error_keeps_original() {
    // catch 후 throw e로 다시 던진 내장 에러가 끝내 잡히지 않으면 원래 메시지·위치·종류로 보고
    let prog = lex_parse("fn f() {\n try {\n  return [1][9]\n } catch e {\n  throw e\n }\n}\nf()");
    let out = compile(&prog).expect("컴파일");
    let mut vm = Vm::new(out.global_count as usize, Arc::new(Mutex::new(Vec::new())));
    let err = vm.run(out.main_fn).expect_err("잡히지 않아야 함");
    assert_eq!(err.kind, "index");
    assert!(err.message.contains("인덱스 범위 초과"), "{}", err.message);
    assert_eq!((err.span.line, err.span.col), (3, 10));
}

// ============================================================================
// Phase 50 — f-문자열 + format()
// ============================================================================

#[test]
fn test_vm_fstring_basic_and_nested() {
    let src = "let name = \"방\"\nlet n = 3\nlet m = {\"k\": [10, 20]}\nprint(f\"안녕, {name}! {n}개 — {n * 2}\")\nprint(f\"{m[\"k\"][1]} {m.k[0] + 1} { {\"a\": 1}[\"a\"] }\")\nprint(f\"중괄호 {{그대로}} {len([1, 2])}\")\nprint(f\"{[1, 2]} {nil} {true} {name.upper()}\")\nlet fut = spawn len(\"abcd\")\nprint(f\"미래 {fut}\")\nprint(f\"\")";
    assert_eq!(run_vm(src), vec!["안녕, 방! 3개 — 6", "20 11 1", "중괄호 {그대로} 2", "[1, 2] nil true 방", "미래 4", ""]);
}

#[test]
fn test_vm_fstring_format_specs() {
    let src = "let name = \"방\"\nprint(f\"{3.14159:.2f} / {1234567:,} / {0.256:.1%} / {1234.5:,.1f}\")\nprint(f\"[{name:>5}] [{name:<5}] [{name:^5}] [{42:05d}] [{-42:05}] [{255:x}] [{5:b}] [{7:*>3}]\")\nprint(f\"{\"abcdef\":.3} {2:.1f}\")\ntry { print(f\"{name:.2d}\") } catch e { print(e.kind, e.message) }";
    assert_eq!(
        run_vm(src),
        vec![
            "3.14 / 1,234,567 / 25.6% / 1,234.5",
            "[    방] [방    ] [  방  ] [00042] [-0042] [ff] [101] [**7]",
            "abc 2.0",
            "value 형식 '.2d': 'd' 형식은 Int 필요, Str 발견",
        ]
    );
}

#[test]
fn test_vm_fstring_errors_have_positions() {
    for (src, want) in [
        ("print(f\"{1 2}\")", "식 하나만"),
        ("print(f\"{x:zz}\")", "알 수 없는 종류"),
        ("let a = 1\nprint(f\"ok {a +}\")", "f-문자열"),
    ] {
        let tokens = Lexer::new(src).tokenize();
        let msg = match tokens {
            Err(e) => e[0].to_string(),
            Ok(t) => match Parser::new(t).parse() {
                Err(e) => e[0].to_string(),
                Ok(_) => panic!("{src}: 에러여야 함"),
            },
        };
        assert!(msg.contains(want) || msg.contains("파스 오류"), "{src}: {msg}");
    }
}

#[test]
fn test_vm_format_builtin() {
    let src = "print(format(\"{} + {} = {:.1f}\", 1, 2, 3))\nprint(format(\"{:*^9}\", \"가운데\"))\nprint(format(\"{{}} 없음\"))\ntry { format(\"{} {}\", 1) } catch e { print(e.kind) }\ntry { format(\"{}\", 1, 2) } catch e { print(e.kind) }\ntry { format(\"{x}\", 1) } catch e { print(e.kind) }";
    assert_eq!(run_vm(src), vec!["1 + 2 = 3.0", "***가운데***", "{} 없음", "argument", "argument", "value"]);
}

// ============================================================================
// Phase 51 — 맵은 삽입 순서를 지킨다 (Python dict와 같음)
// ============================================================================

#[test]
fn test_vm_map_insertion_order() {
    let src = "let m = {\"z\": 1, \"a\": 2}\nm[\"m\"] = 3\nm[\"z\"] = 10\nprint(m)\nprint(keys(m), values(m))\nfor k, v in m { print(k, v) }\nprint(merge(m, {\"b\": 0, \"a\": 99}))\nprint(json_stringify(json_parse(\"{\\\"y\\\": 1, \\\"x\\\": [2]}\")))\nprint({\"b\": 1, \"a\": 2} == {\"a\": 2, \"b\": 1})";
    assert_eq!(
        run_vm(src),
        vec![
            "{z: 10, a: 2, m: 3}",
            "[z, a, m] [10, 2, 3]",
            "z 10",
            "a 2",
            "m 3",
            "{z: 10, a: 99, m: 3, b: 0}",
            "{\"y\":1,\"x\":[2]}",
            "true",
        ]
    );
}

#[test]
fn test_vm_map_order_survives_channel_and_spawn() {
    let src = "let ch = channel(1)\nsend(ch, {\"c\": 1, \"b\": 2, \"a\": 3})\nprint(recv(ch))\nfn mk() { return {\"q\": 1, \"p\": 2} }\nlet f = spawn mk()\nprint(f)";
    assert_eq!(run_vm(src), vec!["{c: 1, b: 2, a: 3}", "{q: 1, p: 2}"]);
}

// ============================================================================
// Phase 52 — 경로 · CSV 빌트인
// ============================================================================

#[test]
fn test_vm_path_builtins() {
    // 구분자는 OS마다 다르므로 조합 결과를 다시 분해해 확인한다
    let src = "let p = path_join(\"a\", \"b\", \"c.tar.gz\")\nprint(path_basename(p), path_ext(p), path_basename(path_dirname(p)))\nprint(path_ext(\"Makefile\") == \"\", path_dirname(\"c.txt\") == \"\", path_basename(\"\") == \"\")\nprint(path_abs(\"x\") == path_join(env_cwd(), \"x\"))\ntry { path_join() } catch e { print(e.kind) }";
    assert_eq!(run_vm(src), vec!["c.tar.gz gz b", "true true true", "true", "argument"]);
}

#[test]
fn test_vm_csv_roundtrip() {
    let src = "let rows = csv_parse(\"name,note\\r\\n방,\\\"a, \\\"\\\"b\\\"\\\"\\nc\\\"\\n\")\nprint(len(rows), rows[1][0], rows[1][1] == \"a, \\\"b\\\"\\nc\")\nprint(csv_stringify([[\"id\", \"text\"], [1, \"쉼표, 포함\"], [2, nil]]))\nprint(csv_parse(csv_stringify(rows)) == rows)\ntry { csv_parse(\"a,\\\"open\") } catch e { print(e.kind) }\ntry { csv_stringify([1]) } catch e { print(e.kind) }";
    assert_eq!(run_vm(src), vec!["2 방 true", "id,text\n1,\"쉼표, 포함\"\n2,\n", "true", "value", "argument"]);
}

// ============================================================================
// Phase 53 — 클로저 상태: 한 작업 안에서는 공유, 작업 경계(spawn)에서는 격리
// ============================================================================

const OBJ: &str = "fn make() {\n let n = 0\n return {\"inc\": fn() { n += 1 }, \"get\": fn() { return n }}\n}\n";

#[test]
fn test_vm_closures_share_variables_within_task() {
    // Python과 같은 늦은 바인딩: 클로저는 값이 아니라 변수를 붙잡는다
    let src = "let fs = []\nfor i in range(3) { fs += [fn() { return i }] }\nprint([fs[0](), fs[1](), fs[2]()])\nfn outer() {\n let n = 1\n let get = fn() { return n }\n n = 99\n return get()\n}\nprint(outer())\n".to_string()
        + OBJ + "let s = make()\nlet t = s\nt.inc()\nprint(s.get(), t.get())";
    assert_eq!(run_vm(&src), vec!["[2, 2, 2]", "99", "1 1"]);
}

#[test]
fn test_vm_spawn_isolates_objects_in_args_and_captures() {
    // 이전: 인자·맵 안의 객체 상태가 스레드 사이에 공유되어 경쟁(결과가 매번 다름)
    let src = OBJ.to_string()
        + "let o = make()\no.inc()\nfn work(obj) {\n for _ in range(2000) { obj.inc() }\n return obj.get()\n}\nlet a = spawn work(o)\nlet b = spawn work(o)\nprint(a, b, o.get())\nparallel {\n spawn (fn() { o.inc() })()\n}\nprint(o.get())\nlet r = parallel_map([o, o], fn(x) {\n x.inc()\n return x.get()\n})\nprint(r, o.get())";
    // 자식마다 부모 상태(1)에서 시작한 독립 복사본, 같은 객체의 메서드끼리는 자식 안에서 공유
    assert_eq!(run_vm(&src), vec!["2001 2001 1", "1", "[2, 2] 1"]);
}

#[test]
fn test_vm_isolation_sees_closures_added_after_caching() {
    // 큰 리스트를 "클로저 없음"으로 캐시한 뒤 클로저를 넣어도 다음 spawn에서 격리된다
    let src = OBJ.to_string()
        + "let big = range(1000)\nfn touch(xs) { return len(xs) }\nprint(wait(spawn touch(big)))\nlet o = make()\nbig[0] = o\nfn use(xs) {\n for _ in range(100) { xs[0].inc() }\n return xs[0].get()\n}\nprint(wait(spawn use(big)), o.get())";
    assert_eq!(run_vm(&src), vec!["1000", "100 0"]);
}

#[test]
fn test_vm_nested_named_function_recursion() {
    // 이전: 함수 안의 이름 있는 함수가 자기 자신을 부르면 컴파일 에러
    let src = "fn outer() {\n fn fact(k) { return if k <= 1 { 1 } else { k * fact(k - 1) } }\n return fact\n}\nlet f = outer()\nprint(f(5), wait(spawn f(10)))";
    assert_eq!(run_vm(src), vec!["120 3628800"]);
}

#[test]
fn test_vm_future_result_isolated_per_task_and_field_autowait() {
    // Future로 받은 객체를 여러 작업에 넘겨도 상태가 공유되지 않는다. 필드 접근도 Future를 자동 대기.
    let src = OBJ.to_string()
        + "let f = spawn make()\nfn work(obj) {\n for _ in range(500) { obj.inc() }\n return obj.get()\n}\nlet a = spawn work(f)\nlet b = spawn work(f)\nprint(a, b, f.get())\nlet g = spawn (fn() { return {\"k\": [7]} })()\nprint(g.k[0], g[\"k\"])";
    assert_eq!(run_vm(&src), vec!["500 500 0", "7 [7]"]);
}

// ============================================================================
// Phase 54 — print 출력 대상: Stdout(쌓지 않음) / Capture(테스트용)
// ============================================================================

#[test]
fn test_vm_capture_output_includes_spawned_tasks() {
    let src = "fn hello(n) { print(f\"작업 {n}\") }\nparallel {\n spawn hello(1)\n}\nprint(\"끝\")\nlet r = parallel_map([2], fn(n) {\n print(f\"맵 {n}\")\n return n\n})";
    assert_eq!(run_vm(src), vec!["작업 1", "끝", "맵 2"]);
}

#[test]
fn test_vm_stdout_mode_keeps_no_buffer() {
    // Stdout 모드는 버퍼 자체가 없다 — 오래 도는 프로그램의 메모리가 출력량에 비례해 늘지 않는다
    let out = bang::vm::Output::Stdout;
    assert!(matches!(out, bang::vm::Output::Stdout));
    let buf = Arc::new(Mutex::new(Vec::new()));
    let cap: bang::vm::Output = buf.clone().into();
    cap.write_line("x".into()).unwrap();
    assert_eq!(*buf.lock().unwrap(), vec!["x".to_string()]);
}

// ============================================================================
// Phase 55 — 구조적 동시성: spawn은 함수 본문에 뿌리내려 함수 반환 시 조인된다
// ============================================================================

#[test]
fn test_vm_function_body_joins_spawned_tasks() {
    // 이전: f가 자식을 기다리지 않고 먼저 반환 → "f 반환"이 "자식 끝"보다 먼저 출력
    let src = "fn slow() {\n sleep(100)\n print(\"자식 끝\")\n}\nfn f() {\n spawn slow()\n return \"f 반환\"\n}\nprint(f())\nprint(\"다음 줄\")";
    assert_eq!(run_vm(src), vec!["자식 끝", "f 반환", "다음 줄"]);
}

#[test]
fn test_vm_child_error_propagates_to_caller_unless_observed() {
    let src = "fn bad() { return 1 / 0 }\nfn f() {\n spawn bad()\n return \"ok\"\n}\ntry { print(f()) } catch e { print(\"잡음\", e.kind) }\nfn g() {\n let x = spawn bad()\n try { print(x + 1) } catch e { print(\"처리함\") }\n return \"정상 반환\"\n}\nprint(g())";
    assert_eq!(run_vm(src), vec!["잡음 zero_division", "처리함", "정상 반환"]);
}

#[test]
fn test_vm_unhandled_child_error_fails_program() {
    // 이전: 경고만 내고 정상 종료(exit 0) — 실패를 감지할 수 없었다
    let prog = lex_parse("fn bad() { return 1 / 0 }\nspawn bad()\nprint(\"끝\")");
    let out = compile(&prog).expect("컴파일");
    let mut vm = Vm::new(out.global_count as usize, Arc::new(Mutex::new(Vec::new())));
    let err = vm.run(out.main_fn).expect_err("자식 에러가 프로그램 에러가 되어야 함");
    assert_eq!(err.kind, "zero_division");
}

#[test]
fn test_vm_parallel_joins_all_before_error_and_scopes_survive_throw() {
    // parallel: 첫 에러 뒤에도 나머지 작업을 끝까지 기다린다. try 안 parallel에서 throw해도 스코프가 정리된다.
    let src = "fn bad() { return 1 / 0 }\nfn slow() {\n sleep(100)\n print(\"느린 작업 끝\")\n}\ntry {\n parallel {\n  spawn bad()\n  spawn slow()\n }\n} catch e { print(\"잡음\") }\ntry {\n parallel {\n  spawn sleep(10)\n  throw \"x\"\n }\n} catch e { print(\"잡음\", e) }\nparallel { spawn sleep(10) }\nprint(\"스코프 정상\")";
    assert_eq!(run_vm(src), vec!["느린 작업 끝", "잡음", "잡음 x", "스코프 정상"]);
}

#[test]
fn test_vm_returned_future_is_complete() {
    // 함수가 spawn 결과를 돌려주면, 그 작업은 함수 반환 전에 이미 끝나 있다 (뿌리내림)
    let src = "fn slow() {\n sleep(50)\n return 7\n}\nfn f() { return spawn slow() }\nlet t = now_ms()\nlet x = f()\nprint(now_ms() - t >= 40, x + 1)";
    assert_eq!(run_vm(src), vec!["true 8"]);
}

// ============================================================================
// Phase 56 — 모든 런타임 에러에 위치, 작업 에러는 원래 위치를 유지
// ============================================================================

fn run_vm_err(source: &str) -> bang::runtime::RuntimeError {
    let prog = lex_parse(source);
    let out = compile(&prog).expect("컴파일 실패");
    let mut vm = Vm::new(out.global_count as usize, Arc::new(Mutex::new(Vec::new())));
    vm.run(out.main_fn).expect_err("에러여야 함")
}

#[test]
fn test_vm_task_start_errors_have_location() {
    // 이전: "런타임 오류: 인자 개수 불일치…"만 나오고 위치가 없었다
    let e = run_vm_err("print(parallel_map([1], fn(a, b) { return a }))");
    assert_eq!((e.span.line, e.span.col), (1, 7), "{e}");
    let e = run_vm_err("fn h(a, b) { return a }\nlet fs = [h]\nlet x = spawn fs[0](1)\nprint(x)");
    assert_eq!(e.span.line, 3, "spawn한 줄을 가리켜야 함: {e}");
}

#[test]
fn test_vm_task_error_keeps_origin_line() {
    // 작업 안에서 난 에러는 값을 쓴 줄이 아니라 실제로 난 줄을 가리킨다
    let src = "fn risky() {\n    let a = 1\n    return a / 0\n}\nlet x = spawn risky()\ntry { print(x) } catch e { print(e.line, e.kind) }";
    assert_eq!(run_vm(src), vec!["3 zero_division"]);
}

// ============================================================================
// Phase 57 — 몰래 바뀌는 동작 제거: / 는 항상 float, Python 참거짓, 정수 맵 키
// ============================================================================

#[test]
fn test_vm_division_is_always_float_and_idiv() {
    // 이전: 8 / 2 → int 4, 7 / 2 → float 3.5 (값에 따라 타입이 바뀜)
    let src = "print(7 / 2, 8 / 2, type(8 / 2), 1e3, sqrt(16))\nprint(idiv(7, 2), idiv(-7, 2), idiv(7.5, 2), type(idiv(8, 2)))\ntry { idiv(1, 0) } catch e { print(e.kind) }\nlet x: int = idiv(9, 3)\nprint(x)";
    assert_eq!(run_vm(src), vec!["3.5 4.0 Float 1000.0 4.0", "3 -4 3.0 Int", "zero_division", "3"]);
}

#[test]
fn test_vm_python_truthiness() {
    // 이전: nil·false만 거짓 → 빈 리스트·0·빈 문자열이 참
    let src = "let out = []\nfor v in [0, 1, \"\", \"a\", [], [0], {}, {\"a\": 1}, nil, 0.0, false, true] {\n out += [if v { 1 } else { 0 }]\n}\nprint(out)\nlet items = []\nif not items { print(\"비었음\") }\nprint(0 or \"기본값\", [] and 1, bool(\"\"))\nlet n = 3\nwhile n { n -= 1 }\nprint(n)";
    assert_eq!(run_vm(src), vec!["[0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 1]", "비었음", "기본값 [] false", "0"]);
}

#[test]
fn test_vm_int_map_keys() {
    // 이전: m[1]이 조용히 문자열 키 "1"로 바뀌었다 (1과 "1"이 같은 키)
    let src = "let m = {1: \"일\", \"1\": \"문자 일\", -2: \"마이너스\"}\nm[3] = \"삼\"\nprint(m[1], m[\"1\"], m[-2], len(m))\nprint(1 in m, \"1\" in m, 2 in m, has(m, 3), get(m, 9, \"없음\"))\nfor k, v in m { print(type(k), k) }\nlet counts = {}\nfor n in [3, 1, 3] { counts[n] = get(counts, n, 0) + 1 }\nprint(counts[3], keys(counts))\ntry { m[1.5] = 1 } catch e { print(e.kind, e.message) }\ntry { print(m[true]) } catch e { print(e.kind) }\nprint(json_stringify({1: \"a\", \"b\": 2.0}))";
    assert_eq!(
        run_vm(src),
        vec![
            "일 문자 일 마이너스 4",
            "true true false true 없음",
            "Int 1", "Str 1", "Int -2", "Int 3",
            "2 [3, 1]",
            "type 맵 키는 str 또는 int여야 함, Float 발견",
            "type",
            "{\"1\":\"a\",\"b\":2.0}",
        ]
    );
}

#[test]
fn test_vm_int_keys_survive_channel_and_spawn() {
    let src = "let ch = channel(1)\nsend(ch, {7: \"x\"})\nlet r = recv(ch)\nprint(r[7], keys(r))\nfn mk() { return {5: [1]} }\nlet f = spawn mk()\nprint(f[5])";
    assert_eq!(run_vm(src), vec!["x [7]", "[1]"]);
}

// ============================================================================
// Phase 60 — 순환 참조 수집: 쓰레기 순환은 해제, 살아 있는 값은 절대 건드리지 않는다
// ============================================================================

#[test]
fn test_cycle_collector_keeps_live_objects_across_many_collections() {
    // 객체를 많이 만들어 수집을 여러 번 일으키는 동안, 일부는 전역 리스트에 보관 → 끝까지 멀쩡해야 함
    let src = "fn make(n) {\n let me = {}\n let count = n\n me = {\"inc\": fn() { count += 1 }, \"get\": fn() { return count }, \"self\": fn() { return me }}\n return me\n}\nlet keep = []\nfor i in range(6000) {\n let o = make(i)\n if i % 1000 == 0 { keep += [o] }\n}\nlet s = 0\nfor o in keep {\n o.inc()\n s += o.get() + o.self().get()\n}\nprint(len(keep), s)";
    // keep: i = 0,1000,...,5000 (6개). inc 후 get = i+1, self().get()도 같은 상태 → 2*(i+1) 합
    let expected: i64 = (0..6).map(|k| 2 * (k * 1000 + 1)).sum();
    assert_eq!(run_vm(src), vec![format!("6 {expected}")]);
}

#[test]
fn test_cycle_collector_does_not_touch_running_frames() {
    // 실행 중인 함수의 지역 변수를 클로저 수천 개가 붙잡는 동안 수집이 일어나도 변수는 유지
    let src = "fn work() {\n let total = 0\n let fs = []\n for i in range(5000) { fs += [fn() { total += 1 }] }\n for f in fs { f() }\n fn rec(k) { return if k <= 0 { total } else { rec(k - 1) } }\n return rec(3)\n}\nprint(work(), work())";
    assert_eq!(run_vm(src), vec!["5000 5000"]);
}

#[test]
fn test_cycle_collector_task_results_survive() {
    // 작업이 돌려준 객체(작업의 상태를 붙잡은 클로저)는 작업이 끝나 수집돼도 부모가 계속 쓸 수 있다
    let src = "fn make() {\n let n = 10\n let me = {}\n me = {\"inc\": fn() { n += 1 }, \"get\": fn() { return n }, \"me\": fn() { return me }}\n return me\n}\nlet objs = []\nfor i in range(300) { objs += [spawn make()] }\nlet s = 0\nfor f in objs {\n f.inc()\n s += f.me().get()\n}\nprint(s)\nlet ps = parallel_map(range(200), fn(i) {\n let k = i\n return fn() { return k * 2 }\n})\nprint(ps[199](), len(ps))";
    assert_eq!(run_vm(src), vec!["3300", "398 200"]);
}

#[test]
fn test_cycle_collector_frees_garbage_cycles() {
    // 버린 순환 객체는 실제로 해제된다 (전역 카운터가 늘어남 — 다른 테스트와 같은 프로세스라 "이상"으로 확인)
    let before = bang::cycles::freed_cells_total();
    let src = "fn make() {\n let me = {}\n me = {\"get\": fn() { return me }}\n return me\n}\nfor i in range(4000) { make() }\nfn outer() {\n fn rec(k) { return if k <= 0 { 0 } else { rec(k - 1) } }\n return rec(1)\n}\nfor i in range(2500) { outer() }\nfn w(n) { return n }\nfor i in range(1500) { wait(spawn w(i)) }\nprint(\"ok\")";
    assert_eq!(run_vm(src), vec!["ok"]);
    let freed = bang::cycles::freed_cells_total() - before;
    assert!(freed >= 4000, "순환이 충분히 해제되지 않음: {freed}");
}
