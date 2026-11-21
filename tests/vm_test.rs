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
try { print(min / -1) } catch e { print("div") }
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
    let src = "let m = {\"a\": [1]}\ntry { m[\"zz\"][0] = 1 } catch e { print(e) }\ntry { m.a[5] = 1 } catch e { print(e) }\ntry { m.a[\"x\"] = 1 } catch e { print(e) }\nprint(m)";
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
    let src = "print(parallel_map(range(1000), fn(x) { return x * 2 })[999])\nprint(parallel_map([1, 2], str))\nprint(parallel_map([], fn(x) { return x }))\ntry {\n parallel_map([1, 0, 2], fn(x) { return 10 / x })\n} catch e {\n print(e)\n}";
    assert_eq!(run_vm(src), vec!["1998", "[1, 2]", "[]", "0으로 나눌 수 없음"]);
}

// ============================================================================
// Phase 46 — 문법 편의: 복합 대입·in·순회 확장·슬라이스·리터럴·주석·한글 식별자
// ============================================================================

#[test]
fn test_vm_compound_assign() {
    let src = "let x = 10\nx += 5\nx -= 1\nx *= 2\nx /= 4\nprint(x)\nlet n = 7\nn %= 4\nprint(n)\nlet s = \"a\"\ns += \"b\"\nprint(s)\nlet xs = [1]\nxs += [2]\nprint(xs)";
    assert_eq!(run_vm(src), vec!["7", "3", "ab", "[1, 2]"]);
}

#[test]
fn test_vm_compound_assign_path_evaluates_key_once() {
    let src = "let calls = 0\nfn k() {\n calls += 1\n return \"a\"\n}\nlet m = {\"a\": 1, \"b\": [1, 2]}\nm[k()] += 10\nm.b[-1] *= 7\nprint(m)\nprint(calls)\ntry { m[\"zz\"] += 1 } catch e { print(e) }\nprint(m)";
    assert_eq!(
        run_vm(src),
        vec!["{a: 11, b: [1, 14]}", "1", "복합 대입: 맵에 키 'zz' 없음 (get(m, k, 기본값) 사용)", "{a: 11, b: [1, 14]}"]
    );
}

#[test]
fn test_vm_in_operator() {
    let src = "let m = {\"a\": 1}\nprint(2 in [1, 2])\nprint(5 not in [1, 2])\nprint(\"a\" in m)\nprint(\"z\" not in m)\nprint(\"ell\" in \"hello\")\nprint([1] in [[1], 2])\nprint(255 in bytes_from_hex(\"00ff\"))\ntry { print(1 in 5) } catch e { print(e) }";
    assert_eq!(
        run_vm(src),
        vec!["true", "true", "true", "true", "true", "true", "true", "in: Int 안에서 Int 찾기 지원 안 됨"]
    );
}

#[test]
fn test_vm_for_over_map_str_bytes_and_pairs() {
    let src = "for k in {\"b\": 2, \"a\": 1} { print(k) }\nfor k, v in {\"b\": 2, \"a\": 1} { print(k + str(v)) }\nfor c in \"한글\" { print(c) }\nfor x in bytes_from_hex(\"0aff\") { print(x) }\nfor i, v in enumerate([\"x\", \"y\"]) { print(str(i) + v) }\ntry {\n for p, q in [[1, 2, 3]] { print(p + q) }\n} catch e { print(e) }";
    assert_eq!(
        run_vm(src),
        vec!["a", "b", "a1", "b2", "한", "글", "10", "255", "0x", "1y", "분해: 값 2개 기대, 3개 발견"]
    );
}

#[test]
fn test_vm_slice_syntax() {
    let src = "let a = [0, 1, 2, 3, 4, 5]\nprint(a[1:3])\nprint(a[:2])\nprint(a[4:])\nprint(a[-2:])\nprint(a[:-4])\nprint(a[:])\nprint(a[10:20])\nprint(a[4:1])\nprint(\"héllo\"[1:3])\nprint(bytes_to_hex(bytes_from_hex(\"0102ff\")[1:]))\ntry { print(a[\"x\":]) } catch e { print(e) }";
    assert_eq!(
        run_vm(src),
        vec!["[1, 2]", "[0, 1]", "[4, 5]", "[4, 5]", "[0, 1]", "[0, 1, 2, 3, 4, 5]", "[]", "[]", "él", "02ff",
             "슬라이스 경계는 Int 또는 생략이어야 함, Str 발견"]
    );
}

#[test]
fn test_vm_literals_comments_unicode_idents() {
    let src = "/* 블록\n주석 */\nlet 이름 = \"방\"\nlet 합 = 1_000 + 0xff + 0b101 + 0o17 /* 인라인 */\nprint(이름 + str(합))\nprint(1e3)\nlet s = \"\"\"a\n\"b\" \"\"\"\nprint(s)";
    assert_eq!(run_vm(src), vec!["방1275", "1000", "a\n\"b\" "]);
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
    let src = "let fs = [fn(a, b = 1) { return a + b }]\ntry { fs[0]() } catch e { print(e) }\ntry { fs[0](1, 2, 3) } catch e { print(e) }";
    assert_eq!(run_vm(src), vec!["인자 개수 불일치: 1~2개 기대, 0개 전달", "인자 개수 불일치: 1~2개 기대, 3개 전달"]);
}

#[test]
fn test_vm_let_unpack() {
    let src = "let [a, b] = [1, 2]\nprint(a + b)\nfn pair() { return [\"x\", 3] }\nfn use_pair() {\n let [name, n] = pair()\n return name + str(n)\n}\nprint(use_pair())\ntry {\n let [p, q] = [1, 2, 3]\n print(p + q)\n} catch e { print(e) }\ntry {\n let [p] = \"ab\"\n print(p)\n} catch e { print(e) }";
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
    let src = "let obj = {\"push\": fn(x) { return x * 100 }, \"n\": 1}\nprint(obj.push(2))\nobj.push(3)\nlet xs = [1]\ntry { xs.push(2) } catch e { print(e) }\nprint(xs)\ntry { print([1].frob()) } catch e { print(e) }\nlet f = spawn \"future\".upper()\nprint(f)";
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
    let src = "assert_eq([1, {\"a\": \"x\"}], [1, {\"a\": \"x\"}])\ntry { assert_eq(\"1\", 1) } catch e { print(e) }\ntry { assert_eq([1, 2], [1]) } catch e { print(e) }";
    assert_eq!(run_vm(src), vec!["assert_eq 실패: 기대 1, 실제 \"1\"", "assert_eq 실패: 기대 [1], 실제 [1, 2]"]);
}
