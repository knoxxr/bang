//! Bang 표준 라이브러리 — 빌트인 함수의 단일 등록 테이블과 구현.
//!
//! # 빌트인 번호 계약
//! `BUILTINS`에서의 위치가 곧 빌트인 번호이며 컴파일된 바이트코드에 박힌다.
//! **끝에만 추가하고, 기존 행의 순서·이름·인자 수를 바꾸지 않는다.**
//! (tests/builtin_table_test.rs가 이름·번호·인자 수·동작 스냅샷을 고정한다.)
//!
//! resolver(이름·인자 수 검사)와 VM(디스패치)이 모두 이 테이블 하나를 읽는다.
//! 빌트인 추가 절차와 명명 규칙은 CLAUDE.md "빌트인 추가 절차" 참고.

use crate::lexer::token::Span;
use crate::runtime::RuntimeError;
use crate::vm::{Vm, VmValue};

mod bytes;
mod collections;
mod concurrency;
mod core;
pub use core::set_script_args;
mod csv;
mod fs;
mod json;
mod math;
mod net;
mod regex;
mod string;
mod sys;
mod time;

/// 각 분야 파일이 공통으로 쓰는 이름들.
pub use string::{apply_format, parse_format_spec, FormatSpec};

mod prelude {
    pub(crate) use crate::lexer::token::Span;
    pub(crate) use crate::runtime::{BangChannel, RuntimeError};
    pub(crate) use crate::vm::*;
    pub(crate) use std::cmp::Ordering;
    pub(crate) use std::sync::{Arc, Mutex, OnceLock};
}

/// 빌트인 구현 시그니처. `stop_depth`는 콜백(사용자 함수)을 부르는 빌트인이
/// VM 실행 루프를 어디까지 돌릴지 정할 때 쓴다.
pub type BuiltinFn = fn(&mut Vm, Vec<VmValue>, Span, usize) -> Result<VmValue, RuntimeError>;

/// 빌트인 한 개의 정의 — 이름·인자 수·구현이 한 곳에 있다.
pub struct Builtin {
    pub name: &'static str,
    /// `None` = 가변 인자 (resolver가 개수를 검사하지 않음)
    pub arity: Option<usize>,
    pub func: BuiltinFn,
}

/// 빌트인 등록 테이블. 위치 = 번호 (끝에만 추가).
#[rustfmt::skip]
pub static BUILTINS: &[Builtin] = &[
    Builtin { name: "print", arity: None, func: core::b_print }, // 0
    Builtin { name: "str", arity: Some(1), func: core::b_str }, // 1
    Builtin { name: "int", arity: Some(1), func: core::b_int }, // 2
    Builtin { name: "float", arity: Some(1), func: core::b_float }, // 3
    Builtin { name: "bool", arity: Some(1), func: core::b_bool }, // 4
    Builtin { name: "len", arity: Some(1), func: core::b_len }, // 5
    Builtin { name: "type", arity: Some(1), func: core::b_type }, // 6
    Builtin { name: "channel", arity: None, func: concurrency::b_channel }, // 7
    Builtin { name: "send", arity: Some(2), func: concurrency::b_send }, // 8
    Builtin { name: "recv", arity: Some(1), func: concurrency::b_recv }, // 9
    Builtin { name: "close", arity: Some(1), func: concurrency::b_close }, // 10
    Builtin { name: "parallel_map", arity: Some(2), func: concurrency::b_parallel_map }, // 11
    Builtin { name: "wait", arity: Some(1), func: concurrency::b_wait }, // 12
    Builtin { name: "push", arity: Some(2), func: collections::b_push }, // 13
    Builtin { name: "pop", arity: Some(1), func: collections::b_pop }, // 14
    Builtin { name: "keys", arity: Some(1), func: collections::b_keys }, // 15
    Builtin { name: "values", arity: Some(1), func: collections::b_values }, // 16
    Builtin { name: "range", arity: None, func: collections::b_range }, // 17
    Builtin { name: "assert", arity: None, func: core::b_assert }, // 18
    Builtin { name: "exit", arity: None, func: core::b_exit }, // 19
    Builtin { name: "split", arity: Some(2), func: string::b_split }, // 20
    Builtin { name: "join", arity: Some(2), func: string::b_join }, // 21
    Builtin { name: "trim", arity: Some(1), func: string::b_trim }, // 22
    Builtin { name: "trim_start", arity: Some(1), func: string::b_trim_start }, // 23
    Builtin { name: "trim_end", arity: Some(1), func: string::b_trim_end }, // 24
    Builtin { name: "replace", arity: Some(3), func: string::b_replace }, // 25
    Builtin { name: "contains", arity: Some(2), func: string::b_contains }, // 26
    Builtin { name: "starts_with", arity: Some(2), func: string::b_starts_with }, // 27
    Builtin { name: "ends_with", arity: Some(2), func: string::b_ends_with }, // 28
    Builtin { name: "upper", arity: Some(1), func: string::b_upper }, // 29
    Builtin { name: "lower", arity: Some(1), func: string::b_lower }, // 30
    Builtin { name: "find", arity: Some(2), func: string::b_find }, // 31
    Builtin { name: "chars", arity: Some(1), func: string::b_chars }, // 32
    Builtin { name: "sort", arity: Some(1), func: collections::b_sort }, // 33
    Builtin { name: "reverse", arity: Some(1), func: collections::b_reverse }, // 34
    Builtin { name: "map", arity: Some(2), func: collections::b_map }, // 35
    Builtin { name: "filter", arity: Some(2), func: collections::b_filter }, // 36
    Builtin { name: "reduce", arity: Some(3), func: collections::b_reduce }, // 37
    Builtin { name: "any", arity: Some(2), func: collections::b_any }, // 38
    Builtin { name: "all", arity: Some(2), func: collections::b_all }, // 39
    Builtin { name: "flat", arity: Some(1), func: collections::b_flat }, // 40
    Builtin { name: "enumerate", arity: Some(1), func: collections::b_enumerate }, // 41
    Builtin { name: "zip", arity: Some(2), func: collections::b_zip }, // 42
    Builtin { name: "sum", arity: Some(1), func: collections::b_sum }, // 43
    Builtin { name: "abs", arity: Some(1), func: math::b_abs }, // 44
    Builtin { name: "sqrt", arity: Some(1), func: math::b_sqrt }, // 45
    Builtin { name: "floor", arity: Some(1), func: math::b_floor }, // 46
    Builtin { name: "ceil", arity: Some(1), func: math::b_ceil }, // 47
    Builtin { name: "round", arity: Some(1), func: math::b_round }, // 48
    Builtin { name: "pow", arity: Some(2), func: math::b_pow }, // 49
    Builtin { name: "min", arity: None, func: math::b_min }, // 50
    Builtin { name: "max", arity: None, func: math::b_max }, // 51
    Builtin { name: "read_file", arity: Some(1), func: fs::b_read_file }, // 52
    Builtin { name: "write_file", arity: Some(2), func: fs::b_write_file }, // 53
    Builtin { name: "input", arity: None, func: core::b_input }, // 54
    Builtin { name: "print_err", arity: None, func: core::b_print_err }, // 55
    Builtin { name: "args", arity: Some(0), func: core::b_args }, // 56
    Builtin { name: "import", arity: Some(1), func: core::b_import }, // 57
    Builtin { name: "slice", arity: Some(3), func: collections::b_slice }, // 58
    Builtin { name: "has", arity: Some(2), func: collections::b_has }, // 59
    Builtin { name: "get", arity: Some(3), func: collections::b_get }, // 60
    Builtin { name: "merge", arity: Some(2), func: collections::b_merge }, // 61
    Builtin { name: "repeat", arity: Some(2), func: string::b_repeat }, // 62
    Builtin { name: "index_of", arity: Some(2), func: collections::b_index_of }, // 63
    Builtin { name: "json_parse", arity: Some(1), func: json::b_json_parse }, // 64
    Builtin { name: "json_stringify", arity: Some(1), func: json::b_json_stringify }, // 65
    Builtin { name: "now_ms", arity: Some(0), func: time::b_now_ms }, // 66
    Builtin { name: "random", arity: Some(0), func: time::b_random }, // 67
    Builtin { name: "random_int", arity: Some(2), func: time::b_random_int }, // 68
    Builtin { name: "list_dir", arity: Some(1), func: fs::b_list_dir }, // 69
    Builtin { name: "file_exists", arity: Some(1), func: fs::b_file_exists }, // 70
    Builtin { name: "is_dir", arity: Some(1), func: fs::b_is_dir }, // 71
    Builtin { name: "sort_by", arity: Some(2), func: collections::b_sort_by }, // 72
    Builtin { name: "unique", arity: Some(1), func: collections::b_unique }, // 73
    Builtin { name: "format_time", arity: Some(1), func: time::b_format_time }, // 74
    Builtin { name: "ord", arity: Some(1), func: string::b_ord }, // 75
    Builtin { name: "chr", arity: Some(1), func: string::b_chr }, // 76
    Builtin { name: "regex_match", arity: Some(2), func: regex::b_regex_match }, // 77
    Builtin { name: "regex_find", arity: Some(2), func: regex::b_regex_find }, // 78
    Builtin { name: "regex_find_all", arity: Some(2), func: regex::b_regex_find_all }, // 79
    Builtin { name: "regex_replace", arity: Some(3), func: regex::b_regex_replace }, // 80
    Builtin { name: "regex_groups", arity: Some(2), func: regex::b_regex_groups }, // 81
    Builtin { name: "gcd", arity: Some(2), func: math::b_gcd }, // 82
    Builtin { name: "clamp", arity: Some(3), func: math::b_clamp }, // 83
    Builtin { name: "sign", arity: Some(1), func: math::b_sign }, // 84
    Builtin { name: "sin", arity: Some(1), func: math::b_sin }, // 85
    Builtin { name: "cos", arity: Some(1), func: math::b_cos }, // 86
    Builtin { name: "tan", arity: Some(1), func: math::b_tan }, // 87
    Builtin { name: "log", arity: Some(1), func: math::b_log }, // 88
    Builtin { name: "log10", arity: Some(1), func: math::b_log10 }, // 89
    Builtin { name: "exp", arity: Some(1), func: math::b_exp }, // 90
    Builtin { name: "pi", arity: Some(0), func: math::b_pi }, // 91
    Builtin { name: "e", arity: Some(0), func: math::b_e }, // 92
    Builtin { name: "union", arity: Some(2), func: collections::b_union }, // 93
    Builtin { name: "intersect", arity: Some(2), func: collections::b_intersect }, // 94
    Builtin { name: "difference", arity: Some(2), func: collections::b_difference }, // 95
    Builtin { name: "tcp_listen", arity: Some(1), func: net::b_tcp_listen }, // 96
    Builtin { name: "tcp_accept", arity: Some(1), func: net::b_tcp_accept }, // 97
    Builtin { name: "tcp_read", arity: Some(1), func: net::b_tcp_read }, // 98
    Builtin { name: "tcp_write", arity: Some(2), func: net::b_tcp_write }, // 99
    Builtin { name: "tcp_close", arity: Some(1), func: net::b_tcp_close }, // 100
    Builtin { name: "tcp_read_until", arity: Some(2), func: net::b_tcp_read_until }, // 101
    Builtin { name: "tcp_set_timeout", arity: Some(2), func: net::b_tcp_set_timeout }, // 102
    Builtin { name: "select", arity: Some(1), func: concurrency::b_select }, // 103
    Builtin { name: "file_size", arity: Some(1), func: fs::b_file_size }, // 104
    Builtin { name: "tcp_send_file", arity: Some(2), func: net::b_tcp_send_file }, // 105
    Builtin { name: "serve_event", arity: Some(2), func: net::b_serve_event }, // 106
    Builtin { name: "tcp_connect", arity: Some(1), func: net::b_tcp_connect }, // 107
    Builtin { name: "http_get", arity: Some(1), func: net::b_http_get }, // 108
    Builtin { name: "proc_run", arity: Some(1), func: sys::b_proc_run }, // 109
    Builtin { name: "env_get", arity: Some(1), func: sys::b_env_get }, // 110
    Builtin { name: "env_cwd", arity: Some(0), func: sys::b_env_cwd }, // 111
    Builtin { name: "sleep", arity: Some(1), func: time::b_sleep }, // 112
    Builtin { name: "file_remove", arity: Some(1), func: fs::b_file_remove }, // 113
    Builtin { name: "file_move", arity: Some(2), func: fs::b_file_move }, // 114
    Builtin { name: "dir_create", arity: Some(1), func: fs::b_dir_create }, // 115
    Builtin { name: "file_append", arity: Some(2), func: fs::b_file_append }, // 116
    Builtin { name: "bytes_from_str", arity: Some(1), func: bytes::b_bytes_from_str }, // 117
    Builtin { name: "bytes_to_str", arity: Some(1), func: bytes::b_bytes_to_str }, // 118
    Builtin { name: "bytes_from_hex", arity: Some(1), func: bytes::b_bytes_from_hex }, // 119
    Builtin { name: "bytes_to_hex", arity: Some(1), func: bytes::b_bytes_to_hex }, // 120
    Builtin { name: "file_read_bytes", arity: Some(1), func: fs::b_file_read_bytes }, // 121
    Builtin { name: "tcp_read_bytes", arity: Some(1), func: net::b_tcp_read_bytes }, // 122
    Builtin { name: "assert_eq", arity: Some(2), func: core::b_assert_eq }, // 123
    Builtin { name: "format", arity: None, func: string::b_format }, // 124
    Builtin { name: "path_join", arity: None, func: fs::b_path_join }, // 125
    Builtin { name: "path_basename", arity: Some(1), func: fs::b_path_basename }, // 126
    Builtin { name: "path_dirname", arity: Some(1), func: fs::b_path_dirname }, // 127
    Builtin { name: "path_ext", arity: Some(1), func: fs::b_path_ext }, // 128
    Builtin { name: "path_abs", arity: Some(1), func: fs::b_path_abs }, // 129
    Builtin { name: "csv_parse", arity: Some(1), func: csv::b_csv_parse }, // 130
    Builtin { name: "csv_stringify", arity: Some(1), func: csv::b_csv_stringify }, // 131
    Builtin { name: "http_post", arity: None, func: net::b_http_post }, // 132
    Builtin { name: "http_request", arity: None, func: net::b_http_request }, // 133
    Builtin { name: "idiv", arity: Some(2), func: math::b_idiv }, // 134
];

/// 이름 → 번호.
pub fn builtin_index(name: &str) -> Option<usize> {
    BUILTINS.iter().position(|b| b.name == name)
}

/// 등록된 빌트인 개수.
pub fn builtin_count() -> usize {
    BUILTINS.len()
}

/// 번호 → 이름.
pub fn builtin_name(idx: usize) -> Option<&'static str> {
    BUILTINS.get(idx).map(|b| b.name)
}

/// 빌트인이 낸 에러의 종류가 아직 정해지지 않았으면(`error`) 빌트인 분야로 정한다 (Phase 48).
/// 인자 검사 헬퍼(req_args·str_arg 등)는 이미 `argument`를 붙이므로 그대로 둔다.
pub fn categorize_error(name: &str, e: RuntimeError) -> RuntimeError {
    use crate::runtime::kind;
    if e.kind != kind::ERROR {
        return e;
    }
    let k = if name.starts_with("file_") || name.starts_with("dir_") || name.starts_with("tcp_")
        || name.starts_with("proc_") || name.starts_with("env_")
        || matches!(name, "read_file" | "write_file" | "list_dir" | "is_dir" | "file_exists"
            | "http_get" | "http_post" | "http_request" | "serve_event" | "input" | "path_abs")
    {
        kind::IO
    } else if name.starts_with("json_") || name.starts_with("regex_") || name.starts_with("bytes_")
        || name.starts_with("csv_")
        || matches!(name, "int" | "float" | "chr" | "ord")
    {
        kind::VALUE
    } else if matches!(name, "assert" | "assert_eq") {
        kind::ASSERTION
    } else if name == "import" {
        kind::IMPORT
    } else if e.message.contains(" 필요, ") || e.message.contains("인자") {
        kind::ARGUMENT
    } else {
        kind::ERROR
    };
    e.with_kind(k)
}

/// 번호 → 인자 수 (`Some(None)` = 가변 인자, `None` = 없는 번호).
pub fn builtin_arity(idx: usize) -> Option<Option<usize>> {
    BUILTINS.get(idx).map(|b| b.arity)
}

/// 번호 → resolver가 등록하는 이름. 단일 테이블이므로 항상 `builtin_name`과 같다
/// (리팩터링 전 resolver/VM 목록이 따로 있던 시절의 계약 테스트 호환용).
pub fn builtin_resolver_name(idx: usize) -> Option<&'static str> {
    builtin_name(idx)
}
