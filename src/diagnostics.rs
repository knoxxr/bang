//! 진단 (Phase 65) — 사람이 읽는 출력 + 기계가 읽는 JSON 출력 + 다른 언어 습관 힌트.
//!
//! AI 에이전트가 bang 코드를 고치려면 에러를 "읽을" 수 있어야 한다. `bang check --json`·`bang run --json`은
//! 진단 하나를 JSON 한 줄로 내보낸다(JSON Lines). 사람용 출력과 JSON 모두 같은 [`Diagnostic`]에서 만든다.
//!
//! 힌트: LLM과 사람이 가장 자주 틀리는 곳은 Python·JavaScript와 헷갈리는 지점이다(`elif`, `None`,
//! `xs.append(v)`, `&&` ...). 에러 위치는 습관 단어 자체가 아니라 그 다음 토큰을 가리키는 일이 많으므로,
//! 에러가 난 **줄**(문자열·주석은 지움)을 보고 판단한다. 런타임 에러는 오판을 피하려고 메시지에 그 이름이
//! 있을 때만 힌트를 붙인다.

use crate::lexer::token::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// 진단 하나.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// "lex" | "parse" | "resolve" | "type" | "compile" | "runtime"
    pub stage: &'static str,
    /// 런타임 에러의 종류 (SPEC 8.2 — `e.kind`와 같은 값). 그 밖의 단계는 None.
    pub kind: Option<&'static str>,
    /// 위치 접두어 없는 메시지
    pub message: String,
    /// 사람용 출력의 첫 줄 (기존 형식 그대로 — "[3:7] 파스 오류: ...")
    pub display: String,
    pub file: Option<String>,
    pub span: Span,
    pub hint: Option<&'static str>,
}

impl Diagnostic {
    /// 진단을 만들고, 오류면 해당 소스 줄을 보고 힌트를 붙인다.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        severity: Severity,
        stage: &'static str,
        kind: Option<&'static str>,
        message: String,
        display: String,
        file: Option<String>,
        span: Span,
        source: &str,
    ) -> Self {
        let hint = if severity == Severity::Error {
            let line = source.lines().nth(span.line.saturating_sub(1)).unwrap_or("");
            hint_for(stage, &message, if span.line == 0 { "" } else { line })
        } else {
            None
        };
        Self { severity, stage, kind, message, display, file, span, hint }
    }

    /// 사람용: 메시지 + 소스 줄 + ^ 표시 (+ 힌트).
    pub fn human(&self, source: &str) -> String {
        let mut out = with_context(&self.display, self.span, source);
        if let Some(h) = self.hint {
            out.push_str("\n  = 힌트: ");
            out.push_str(h);
        }
        out
    }

    /// 기계용: JSON 한 줄 (필드 순서 고정).
    pub fn json(&self) -> String {
        let mut o = String::from("{");
        o.push_str(&format!(
            "\"severity\":{},\"stage\":{},\"kind\":{},\"message\":{},\"file\":{},\"line\":{},\"col\":{},\"hint\":{}",
            json_str(match self.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            }),
            json_str(self.stage),
            self.kind.map(json_str).unwrap_or_else(|| "null".into()),
            json_str(&self.message),
            self.file.as_deref().map(json_str).unwrap_or_else(|| "null".into()),
            self.span.line,
            self.span.col,
            self.hint.map(json_str).unwrap_or_else(|| "null".into()),
        ));
        o.push('}');
        o
    }
}

/// 기존 사람용 형식: 메시지 / 줄 번호 | 소스 / ^ 표시.
pub fn with_context(message: &str, span: Span, source: &str) -> String {
    if span.line == 0 {
        return message.to_string();
    }
    let line_text = source.lines().nth(span.line.saturating_sub(1)).unwrap_or("");
    let pointer_offset = span.col.saturating_sub(1);
    format!("{message}\n  |\n{:>3} | {line_text}\n  | {:>pointer_offset$}^", span.line, "")
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// 문자열 리터럴 안쪽을 공백으로 지우고 `//` 주석을 잘라 낸 줄 (힌트 판단용).
fn code_only(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    chars.next();
                    out.push(' ');
                    out.push(' ');
                }
                '"' => {
                    in_str = false;
                    out.push('"');
                }
                _ => out.push(' '),
            }
        } else if c == '"' {
            in_str = true;
            out.push('"');
        } else if c == '/' && chars.peek() == Some(&'/') {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

fn words(code: &str) -> Vec<&str> {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect()
}

/// 다른 언어 습관에 대한 힌트. 첫 번째로 맞는 규칙 하나만.
pub fn hint_for(stage: &str, message: &str, line: &str) -> Option<&'static str> {
    if stage == "runtime" {
        // 런타임은 메시지에 그 이름이 나올 때만 (같은 줄의 다른 원인 에러에 엉뚱한 힌트를 달지 않게)
        if message.contains("'append'") {
            return Some("리스트에 추가는 `xs += [v]` (또는 `xs = xs.push(v)`) — 리스트는 값이라 제자리에서 바뀌지 않습니다");
        }
        if message.contains("'length'") || message.contains("'size'") || message.contains("'count'") {
            return Some("길이는 `len(x)`");
        }
        if message.contains("'remove'") {
            return Some("리스트에서 빼려면 슬라이스로 새 리스트를 만드세요: `xs = xs[:-1]`");
        }
        return None;
    }
    let code = code_only(line);
    let trimmed = code.trim();
    let ws = words(&code);
    let has = |w: &str| ws.contains(&w);
    let starts = |w: &str| ws.first() == Some(&w);

    if has("elif") {
        return Some("`elif` 대신 `else if`를 씁니다: `} else if 조건 {`");
    }
    if starts("def") {
        return Some("함수 정의는 `fn 이름(인자) { ... }` — 본문은 `{ }`로 감싸고 `return`으로 값을 돌려줍니다");
    }
    if starts("function") {
        return Some("함수 정의는 `fn 이름(인자) { ... }`");
    }
    if starts("var") || starts("const") {
        return Some("변수 선언은 `let 이름 = 값`");
    }
    if starts("class") {
        return Some("클래스는 없습니다 — 상태를 붙잡은 클로저들을 맵에 담아 객체를 만듭니다 (`return {\"get\": fn() { return n }}`)");
    }
    if has("lambda") {
        return Some("익명 함수는 `fn(x) { return x * 2 }`");
    }
    if has("except") {
        return Some("`except` 대신 `catch e { ... }` — `try { ... } catch e { ... }`");
    }
    if starts("raise") {
        return Some("`raise` 대신 `throw 값` (아무 값이나 던질 수 있음)");
    }
    if starts("from") || (starts("import") && !code.contains("import(")) {
        return Some("모듈은 함수로 불러옵니다: `let m = import(\"이름\")` — json_parse·regex_find 같은 내장 함수는 import 없이 씁니다");
    }
    if starts("pass") {
        return Some("`pass`는 없습니다 — 빈 블록은 `{ }`");
    }
    if has("None") || has("null") || has("undefined") {
        return Some("값이 없음은 `nil`");
    }
    if has("True") || has("False") {
        return Some("불리언은 소문자 `true` / `false`");
    }
    if ws.iter().skip(1).any(|w| *w == "is") {
        return Some("`is` 대신 `==` / `!=` (예: `x == nil`)");
    }
    if code.contains("&&") || code.contains("||") {
        return Some("`&&` `||` 대신 `and` `or`");
    }
    if code.contains("++") || code.contains("--") {
        return Some("`x++` 대신 `x += 1`");
    }
    if code.contains("**") {
        return Some("거듭제곱은 `pow(a, b)`");
    }
    if code.contains(';') {
        return Some("세미콜론은 쓰지 않습니다 — 줄바꿈이 문장의 끝입니다");
    }
    if code.char_indices().any(|(i, c)| c == '!' && !code[i + 1..].starts_with('=')) {
        return Some("`!x` 대신 `not x`");
    }
    let block_kw = ["if", "else", "for", "while", "try", "fn"];
    if trimmed.ends_with(':') && ws.first().is_some_and(|w| block_kw.contains(w)) {
        return Some("블록은 콜론·들여쓰기가 아니라 `{ }`로 감쌉니다: `if 조건 { ... }`");
    }
    if ws.iter().skip(1).any(|w| *w == "if") && has("else") && !code.contains("else if") && !code.contains('{') {
        return Some("조건식은 `if 조건 { 값1 } else { 값2 }` (Python의 `a if c else b` 순서가 아님)");
    }
    if code.contains(".append(") {
        return Some("리스트에 추가는 `xs += [v]`");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(line: &str) -> Option<&'static str> {
        hint_for("parse", "", line)
    }

    #[test]
    fn python_and_js_habits() {
        assert!(h("} elif x == 2 {").unwrap().contains("else if"));
        assert!(h("def f(a):").unwrap().contains("fn 이름"));
        assert!(h("let y = None").unwrap().contains("nil"));
        assert!(h("let t = True").unwrap().contains("true"));
        assert!(h("if x is nil {").unwrap().contains("=="));
        assert!(h("if a && b {").unwrap().contains("and"));
        assert!(h("if !x {").unwrap().contains("not x"));
        assert!(h("x++").unwrap().contains("+= 1"));
        assert!(h("let z = 1;").unwrap().contains("세미콜론"));
        assert!(h("import json").unwrap().contains("import(\""));
        assert!(h("print(2 ** 3)").unwrap().contains("pow"));
        assert!(h("for i in range(3):").unwrap().contains("{ }"));
        assert!(h("print(a if c else b)").unwrap().contains("조건식"));
        assert!(h("try {} except e {}").unwrap().contains("catch"));
        assert!(h("var q = 1").unwrap().contains("let"));
        assert!(hint_for("runtime", "필드 접근: List 에 필드 'append' 없음", "xs.append(2)").unwrap().contains("+= [v]"));
    }

    #[test]
    fn no_false_positives_from_strings_comments_or_normal_code() {
        assert_eq!(h("print(\"None; && ** elif\")"), None);
        assert_eq!(h("let x = 1   // elif None; x++"), None);
        assert_eq!(h("if x != 1 { print(x) }"), None);
        assert_eq!(h("let v = if c { 1 } else { 2 }"), None);
        assert_eq!(h("let m = import(\"lib/a.bang\")"), None);
        // 런타임 에러는 메시지가 맞을 때만
        assert_eq!(hint_for("runtime", "0으로 나눌 수 없음", "print(xs.length / 0)"), None);
    }

    #[test]
    fn json_line_escapes() {
        let d = Diagnostic::new(
            Severity::Error,
            "parse",
            None,
            "따옴표 \" 와\n줄바꿈".into(),
            "[1:1] 파스 오류: x".into(),
            Some("a\\b.bang".into()),
            Span::new(1, 5),
            "let y = None",
        );
        assert_eq!(
            d.json(),
            "{\"severity\":\"error\",\"stage\":\"parse\",\"kind\":null,\"message\":\"따옴표 \\\" 와\\n줄바꿈\",\"file\":\"a\\\\b.bang\",\"line\":1,\"col\":5,\"hint\":\"값이 없음은 `nil`\"}"
        );
    }
}
