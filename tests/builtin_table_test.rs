//! 빌트인 번호 계약 고정 테스트 (Phase 44 리팩터링 전 작성).
//!
//! 빌트인 번호는 바이트코드에 박히는 대외 계약이다("끝에만 추가, 번호 불변").
//! 이 표는 Phase 44 리팩터링 **전에** 현재 코드에서 추출한 정답지다.
//! 새 빌트인은 표 끝에만 추가한다. 기존 행을 바꾸거나 끼워 넣으면 이 테스트가 실패해야 한다.

use bang::stdlib::{builtin_arity, builtin_count, builtin_name, builtin_resolver_name};
use bang::vm::builtin_index;

/// (번호, 이름, 인자 수 — None = 가변)
const GOLDEN: &[(usize, &str, Option<usize>)] = &[
    (0, "print", None),
    (1, "str", Some(1)),
    (2, "int", Some(1)),
    (3, "float", Some(1)),
    (4, "bool", Some(1)),
    (5, "len", Some(1)),
    (6, "type", Some(1)),
    (7, "channel", None),
    (8, "send", Some(2)),
    (9, "recv", Some(1)),
    (10, "close", Some(1)),
    (11, "parallel_map", Some(2)),
    (12, "wait", Some(1)),
    (13, "push", Some(2)),
    (14, "pop", Some(1)),
    (15, "keys", Some(1)),
    (16, "values", Some(1)),
    (17, "range", None),
    (18, "assert", None),
    (19, "exit", None),
    (20, "split", Some(2)),
    (21, "join", Some(2)),
    (22, "trim", Some(1)),
    (23, "trim_start", Some(1)),
    (24, "trim_end", Some(1)),
    (25, "replace", Some(3)),
    (26, "contains", Some(2)),
    (27, "starts_with", Some(2)),
    (28, "ends_with", Some(2)),
    (29, "upper", Some(1)),
    (30, "lower", Some(1)),
    (31, "find", Some(2)),
    (32, "chars", Some(1)),
    (33, "sort", Some(1)),
    (34, "reverse", Some(1)),
    (35, "map", Some(2)),
    (36, "filter", Some(2)),
    (37, "reduce", Some(3)),
    (38, "any", Some(2)),
    (39, "all", Some(2)),
    (40, "flat", Some(1)),
    (41, "enumerate", Some(1)),
    (42, "zip", Some(2)),
    (43, "sum", Some(1)),
    (44, "abs", Some(1)),
    (45, "sqrt", Some(1)),
    (46, "floor", Some(1)),
    (47, "ceil", Some(1)),
    (48, "round", Some(1)),
    (49, "pow", Some(2)),
    (50, "min", None),
    (51, "max", None),
    (52, "read_file", Some(1)),
    (53, "write_file", Some(2)),
    (54, "input", None),
    (55, "print_err", None),
    (56, "args", Some(0)),
    (57, "import", Some(1)),
    (58, "slice", Some(3)),
    (59, "has", Some(2)),
    (60, "get", Some(3)),
    (61, "merge", Some(2)),
    (62, "repeat", Some(2)),
    (63, "index_of", Some(2)),
    (64, "json_parse", Some(1)),
    (65, "json_stringify", Some(1)),
    (66, "now_ms", Some(0)),
    (67, "random", Some(0)),
    (68, "random_int", Some(2)),
    (69, "list_dir", Some(1)),
    (70, "file_exists", Some(1)),
    (71, "is_dir", Some(1)),
    (72, "sort_by", Some(2)),
    (73, "unique", Some(1)),
    (74, "format_time", Some(1)),
    (75, "ord", Some(1)),
    (76, "chr", Some(1)),
    (77, "regex_match", Some(2)),
    (78, "regex_find", Some(2)),
    (79, "regex_find_all", Some(2)),
    (80, "regex_replace", Some(3)),
    (81, "regex_groups", Some(2)),
    (82, "gcd", Some(2)),
    (83, "clamp", Some(3)),
    (84, "sign", Some(1)),
    (85, "sin", Some(1)),
    (86, "cos", Some(1)),
    (87, "tan", Some(1)),
    (88, "log", Some(1)),
    (89, "log10", Some(1)),
    (90, "exp", Some(1)),
    (91, "pi", Some(0)),
    (92, "e", Some(0)),
    (93, "union", Some(2)),
    (94, "intersect", Some(2)),
    (95, "difference", Some(2)),
    (96, "tcp_listen", Some(1)),
    (97, "tcp_accept", Some(1)),
    (98, "tcp_read", Some(1)),
    (99, "tcp_write", Some(2)),
    (100, "tcp_close", Some(1)),
    (101, "tcp_read_until", Some(2)),
    (102, "tcp_set_timeout", Some(2)),
    (103, "select", Some(1)),
    (104, "file_size", Some(1)),
    (105, "tcp_send_file", Some(2)),
    (106, "serve_event", Some(2)),
    (107, "tcp_connect", Some(1)),
    (108, "http_get", Some(1)),
    (109, "proc_run", Some(1)),
    (110, "env_get", Some(1)),
    (111, "env_cwd", Some(0)),
    (112, "sleep", Some(1)),
    (113, "file_remove", Some(1)),
    (114, "file_move", Some(2)),
    (115, "dir_create", Some(1)),
    (116, "file_append", Some(2)),
    (117, "bytes_from_str", Some(1)),
    (118, "bytes_to_str", Some(1)),
    (119, "bytes_from_hex", Some(1)),
    (120, "bytes_to_hex", Some(1)),
    (121, "file_read_bytes", Some(1)),
    (122, "tcp_read_bytes", Some(1)),
];

#[test]
fn builtin_count_matches_golden() {
    assert_eq!(
        builtin_count(),
        GOLDEN.len(),
        "빌트인 개수 변경 — 끝에만 추가했다면 GOLDEN에도 추가"
    );
}

#[test]
fn builtin_names_and_indices_unchanged() {
    for &(idx, name, _) in GOLDEN {
        assert_eq!(builtin_name(idx), Some(name), "VM 번호 {idx}의 이름이 바뀜");
        assert_eq!(
            builtin_resolver_name(idx),
            Some(name),
            "resolver 번호 {idx}의 이름이 바뀜"
        );
        assert_eq!(
            builtin_index(name),
            Some(idx),
            "이름 '{name}'의 번호가 바뀜"
        );
    }
}

#[test]
fn builtin_arities_unchanged() {
    for &(idx, name, arity) in GOLDEN {
        assert_eq!(
            builtin_arity(idx),
            Some(arity),
            "'{name}'({idx})의 인자 수가 바뀜"
        );
    }
}

#[test]
fn builtin_names_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for &(_, name, _) in GOLDEN {
        assert!(seen.insert(name), "중복 빌트인 이름: {name}");
    }
}

/// 동작 스냅샷: 결정적인 빌트인 대부분을 호출한 출력이 리팩터링 전 기록과 같아야 한다.
/// (이름표만으로는 "번호 50에 다른 구현이 연결된" 실수를 못 잡으므로 실제 호출로 검증)
#[test]
fn builtin_behavior_snapshot_unchanged() {
    use std::process::Command;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let script = root.join("builtins_snapshot.bang");
    let expected = std::fs::read_to_string(root.join("builtins_snapshot.expected"))
        .unwrap()
        .replace("\r\n", "\n");
    let dir = std::env::temp_dir().join(format!("bang_snapshot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bang"))
        .arg(&script)
        .current_dir(&dir)
        .env_remove("BANG_SNAPSHOT_NO_SUCH_VAR")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let (a, e): (Vec<_>, Vec<_>) = (actual.lines().collect(), expected.lines().collect());
    for (i, (al, el)) in a.iter().zip(e.iter()).enumerate() {
        assert_eq!(al, el, "스냅샷 {}번째 줄이 다름", i + 1);
    }
    assert_eq!(a.len(), e.len(), "스냅샷 줄 수가 다름");
}
