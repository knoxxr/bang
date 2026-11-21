//! `bang fmt` — 줄 단위 들여쓰기 정규화 포매터 (Phase 49)
//!
//! 의도적으로 보수적이다: 줄을 합치거나 나누지 않고, 줄 안의 공백도 건드리지 않는다.
//! 하는 일은 네 가지뿐이다.
//!   1. 들여쓰기를 괄호 깊이 × 4칸으로 맞춘다 (줄 맨 앞의 닫는 괄호만큼 한 단계 내어 씀).
//!   2. 줄 끝 공백을 지운다.
//!   3. 연속 빈 줄을 하나로 줄이고, 파일 앞뒤 빈 줄을 지운다.
//!   4. 줄바꿈을 LF로 통일하고 파일 끝에 줄바꿈 하나를 둔다.
//!
//! 여러 줄 문자열(`"""..."""`)과 블록 주석(`/* ... */`)의 안쪽 줄은 한 글자도 바꾸지 않는다.
//! 포맷 전후의 토큰열이 다르면(포매터 버그) 결과를 쓰지 않도록 [`format_source`]가 검증한다.

use crate::lexer::Lexer;
use crate::lexer::token::TokenKind;

const INDENT: &str = "    ";

/// 줄을 훑는 동안 유지하는 상태.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Code,
    /// `"""` 문자열 안
    Triple,
    /// `/* */` 주석 안
    Block,
}

/// 한 줄을 훑은 결과.
struct LineScan {
    /// 줄 맨 앞의 닫는 괄호 수
    leading_closers: usize,
    /// 괄호 사건열 (true=열림) — 문자열·주석 안의 괄호는 제외
    events: Vec<bool>,
    /// 줄 끝 상태
    end: Mode,
    /// 마지막 코드 토큰이 이항 연산자/대입(다음 줄로 식이 이어짐)인가
    continues: bool,
}

/// 줄을 잇는 마지막 토큰인가: 이항 연산자·대입·`and`/`or` (콤마는 제외 — 괄호 안 나열)
fn is_continuation(last: &str) -> bool {
    matches!(last, "and" | "or")
        || last.chars().last().is_some_and(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>'))
}

/// 한 줄을 훑는다. `start`는 줄 시작 시점의 상태.
fn scan_line(line: &str, start: Mode) -> LineScan {
    let chars: Vec<char> = line.chars().collect();
    let mut mode = start;
    let mut events = Vec::new();
    let mut leading_closers = 0usize;
    let mut at_line_start = start == Mode::Code;
    // 마지막 코드 토큰(연산자 문자 또는 단어). 문자열·괄호는 연속 아님으로 기록.
    let mut last = String::new();
    let mut in_word = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');
        match mode {
            Mode::Triple => {
                if c == '\\' {
                    i += 2;
                    continue;
                }
                if c == '"' && next == '"' && chars.get(i + 2) == Some(&'"') {
                    mode = Mode::Code;
                    i += 3;
                    continue;
                }
                i += 1;
            }
            Mode::Block => {
                if c == '*' && next == '/' {
                    mode = Mode::Code;
                    i += 2;
                    continue;
                }
                i += 1;
            }
            Mode::Code => {
                if c == ' ' || c == '\t' {
                    in_word = false;
                    i += 1;
                    continue;
                }
                let is_closer = matches!(c, '}' | ')' | ']');
                if at_line_start && is_closer {
                    leading_closers += 1;
                } else {
                    at_line_start = false;
                }
                match c {
                    '/' if next == '/' => break, // 줄 주석: 나머지 무시
                    '/' if next == '*' => {
                        mode = Mode::Block;
                        i += 2;
                        continue;
                    }
                    '"' => {
                        if next == '"' && chars.get(i + 2) == Some(&'"') {
                            mode = Mode::Triple;
                            i += 3;
                            continue;
                        }
                        // 한 줄 문자열: 닫는 따옴표까지 건너뜀
                        last = "\"".into();
                        in_word = false;
                        i += 1;
                        while i < chars.len() {
                            match chars[i] {
                                '\\' => i += 2,
                                '"' => {
                                    i += 1;
                                    break;
                                }
                                _ => i += 1,
                            }
                        }
                        continue;
                    }
                    '{' | '(' | '[' => events.push(true),
                    '}' | ')' | ']' => events.push(false),
                    _ => {}
                }
                if c.is_alphanumeric() || c == '_' {
                    // 단어 이어 붙이기 (and / or 판정용)
                    if !in_word {
                        last.clear();
                    }
                    in_word = true;
                    last.push(c);
                } else {
                    in_word = false;
                    last = c.to_string();
                }
                i += 1;
            }
        }
    }
    let continues = mode == Mode::Code && is_continuation(&last);
    LineScan { leading_closers, events, end: mode, continues }
}

/// 들여쓰기만 정규화한 결과 (토큰 검증 없음).
///
/// 각 여는 괄호는 "그 괄호가 있는 줄의 들여쓰기 수준"을 스택에 기록한다. 다음 줄은
/// 가장 안쪽 미닫힘 괄호를 연 줄보다 한 단계 깊다 — 그래서 `f(fn(x) {`처럼 한 줄에서
/// 괄호를 여럿 열어도 본문은 한 단계만 들어간다. 줄 맨 앞의 닫는 괄호는 그 괄호를 연
/// 줄의 수준으로 내어 쓴다.
pub fn reindent(source: &str) -> String {
    let src = source.replace("\r\n", "\n");
    let mut out: Vec<String> = Vec::new();
    let mut openers: Vec<usize> = Vec::new();
    let mut mode = Mode::Code;
    let mut blank_run = 0;
    // 앞 줄이 연산자로 끝나 식이 이어지는 중인가, 그 식(문장)이 시작된 줄의 수준
    let mut cont = false;
    let mut stmt_level = 0usize;

    let content_level = |openers: &Vec<usize>| openers.last().map(|l| l + 1).unwrap_or(0);

    for raw in src.split('\n') {
        let start_mode = mode;
        let LineScan { leading_closers: closers, events, end: end_mode, continues } = scan_line(raw, start_mode);
        mode = end_mode;

        let level;
        if start_mode != Mode::Code {
            // 여러 줄 문자열/블록 주석의 안쪽 줄: 그대로 (문자열 내용 보존)
            out.push(raw.to_string());
            blank_run = 0;
            level = content_level(&openers);
        } else {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                blank_run += 1;
                if blank_run == 1 && !out.is_empty() {
                    out.push(String::new());
                }
                continue;
            }
            blank_run = 0;
            // 맨 앞 닫는 괄호가 닫는 괄호들 중 가장 바깥 것을 연 줄의 수준
            level = if closers > 0 && closers <= openers.len() {
                openers[openers.len() - closers]
            } else if cont {
                stmt_level + 1 // 이어지는 줄: 문장 첫 줄보다 한 단계
            } else {
                content_level(&openers)
            };
            if !cont {
                stmt_level = level;
            }
            cont = continues;
            // 줄 끝 공백 제거 — 단, 이 줄에서 여러 줄 문자열이 시작되면 꼬리 공백도 내용이므로 보존
            let body = if end_mode == Mode::Triple { raw.trim_start() } else { trimmed };
            out.push(format!("{}{}", INDENT.repeat(level), body));
        }
        for open in events {
            if open {
                openers.push(level);
            } else {
                openers.pop();
            }
        }
    }

    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let mut result = out.join("\n");
    result.push('\n');
    result
}

/// 소스 → 토큰 종류열 (비교용). 렉싱 실패면 None.
fn token_kinds(source: &str) -> Option<Vec<TokenKind>> {
    Lexer::new(source).tokenize().ok().map(|ts| ts.into_iter().map(|t| t.kind).collect())
}

/// 포맷 결과. 소스가 렉싱되지 않거나 포맷이 토큰열을 바꾸면 에러(쓰지 말 것).
pub fn format_source(source: &str) -> Result<String, String> {
    let before = token_kinds(source).ok_or("렉싱 오류가 있는 파일은 포맷하지 않습니다 (bang check로 확인)")?;
    let formatted = reindent(source);
    let after = token_kinds(&formatted).ok_or("내부 오류: 포맷 결과를 렉싱할 수 없음 — 파일을 바꾸지 않았습니다")?;
    if before != after {
        return Err("내부 오류: 포맷이 토큰열을 바꿈 — 파일을 바꾸지 않았습니다 (버그 제보 부탁)".into());
    }
    Ok(formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindents_blocks_and_closers() {
        let src = "fn f(x) {\nif x {\n  return 1\n      } else {\nreturn 2\n}\n}\n";
        let want = "fn f(x) {\n    if x {\n        return 1\n    } else {\n        return 2\n    }\n}\n";
        assert_eq!(format_source(src).unwrap(), want);
    }

    #[test]
    fn brackets_inside_strings_and_comments_ignored() {
        let src = "let s = \"{[(\" // {{{\nlet t = 1 /* { */\nprint(s)\n";
        assert_eq!(format_source(src).unwrap(), src);
    }

    #[test]
    fn multiline_constructs_preserved() {
        let src = "fn f() {\nlet s = \"\"\"\n   keep {\n  me   \n\"\"\"\n/*\n   comment {\n*/\nreturn s\n}\n";
        let want = "fn f() {\n    let s = \"\"\"\n   keep {\n  me   \n\"\"\"\n    /*\n   comment {\n*/\n    return s\n}\n";
        assert_eq!(format_source(src).unwrap(), want);
    }

    #[test]
    fn blank_lines_trailing_space_crlf() {
        let src = "\n\nlet a = 1   \r\n\r\n\r\n\r\nlet b = 2\n\n\n";
        assert_eq!(format_source(src).unwrap(), "let a = 1\n\nlet b = 2\n");
    }

    #[test]
    fn multiline_call_args_and_maps() {
        let src = "let m = {\n\"a\": [\n1,\n2\n],\n}\nf(fn(x) {\nreturn x\n})\n";
        let want = "let m = {\n    \"a\": [\n        1,\n        2\n    ],\n}\nf(fn(x) {\n    return x\n})\n";
        assert_eq!(format_source(src).unwrap(), want);
    }

    #[test]
    fn continuation_lines_get_hanging_indent() {
        let src = "fn f() {\nlet s = \"a\" +\n\"b\" +\n\"c\"\nlet ok = x and\ny\nreturn s\n}\n";
        let want = "fn f() {\n    let s = \"a\" +\n        \"b\" +\n        \"c\"\n    let ok = x and\n        y\n    return s\n}\n";
        assert_eq!(format_source(src).unwrap(), want);
        // 맵 항목 끝의 콤마는 연속으로 보지 않는다
        let m = "let m = {\n    \"a\": 1,\n    \"b\": 2\n}\n";
        assert_eq!(format_source(m).unwrap(), m);
    }

    #[test]
    fn idempotent_and_rejects_lex_errors() {
        let src = "fn f() {\n        let x = [1,\n  2]\n    return x\n}\n";
        let once = format_source(src).unwrap();
        assert_eq!(format_source(&once).unwrap(), once);
        assert!(format_source("let s = \"unterminated\n").is_err());
    }
}
