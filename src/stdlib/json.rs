//! 빌트인: JSON

use super::prelude::*;

/// 64: json_parse(str) → value
pub(crate) fn b_json_parse(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("json_parse", &args, 1, span)?;
    let s = str_arg("json_parse", &args[0], span)?;
    json_parse(&s, span)
}

/// 65: json_stringify(value) → str
pub(crate) fn b_json_stringify(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("json_stringify", &args, 1, span)?;
    let mut out = String::new();
    json_stringify(&args[0], &mut out, span)?;
    Ok(VmValue::Str(out))
}

// ── 구현 헬퍼 (vm.rs에서 이동) ──────────────────────────────────────────────

// ── JSON ────────────────────────────────────────────────────────────────────

/// JSON 텍스트 → VmValue. object→Map, array→List, number→Int/Float, null→Nil.
pub(crate) fn json_parse(s: &str, span: Span) -> Result<VmValue, RuntimeError> {
    let chars: Vec<char> = s.chars().collect();
    let mut p = JsonParser {
        chars: &chars,
        pos: 0,
        span,
        depth: 0,
    };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(RuntimeError::new("json_parse: 끝에 잉여 문자", span));
    }
    Ok(v)
}

/// JSON 중첩 깊이 상한 (재귀 파서의 스택 오버플로 방지).
const JSON_MAX_DEPTH: usize = 512;

struct JsonParser<'a> {
    chars: &'a [char],
    pos: usize,
    span: Span,
    depth: usize,
}

impl JsonParser<'_> {
    fn err(&self, msg: &str) -> RuntimeError {
        RuntimeError::new(format!("json_parse: {msg}"), self.span)
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn parse_value(&mut self) -> Result<VmValue, RuntimeError> {
        self.skip_ws();
        // 깊이 가드: 깊은 중첩([[[.../{{{...)에서 스택 오버플로 대신 잡히는 에러
        if self.depth > JSON_MAX_DEPTH {
            return Err(self.err("중첩이 너무 깊음"));
        }
        match self.peek() {
            Some('{') => {
                self.depth += 1;
                let r = self.parse_object();
                self.depth -= 1;
                r
            }
            Some('[') => {
                self.depth += 1;
                let r = self.parse_array();
                self.depth -= 1;
                r
            }
            Some('"') => Ok(VmValue::Str(self.parse_string()?)),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            _ => Err(self.err("예상하지 못한 토큰")),
        }
    }
    fn expect(&mut self, lit: &str) -> Result<(), RuntimeError> {
        for ec in lit.chars() {
            if self.peek() == Some(ec) {
                self.pos += 1;
            } else {
                return Err(self.err("리터럴 불일치"));
            }
        }
        Ok(())
    }
    fn parse_bool(&mut self) -> Result<VmValue, RuntimeError> {
        if self.peek() == Some('t') {
            self.expect("true")?;
            Ok(VmValue::Bool(true))
        } else {
            self.expect("false")?;
            Ok(VmValue::Bool(false))
        }
    }
    fn parse_null(&mut self) -> Result<VmValue, RuntimeError> {
        self.expect("null")?;
        Ok(VmValue::Nil)
    }
    fn parse_string(&mut self) -> Result<String, RuntimeError> {
        self.pos += 1; // opening "
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("종료되지 않은 문자열")),
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some('"') => out.push('"'),
                        Some('\\') => out.push('\\'),
                        Some('/') => out.push('/'),
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('r') => out.push('\r'),
                        Some('b') => out.push('\u{0008}'),
                        Some('f') => out.push('\u{000C}'),
                        Some('u') => {
                            let mut code = 0u32;
                            for _ in 0..4 {
                                self.pos += 1;
                                let d = self
                                    .peek()
                                    .and_then(|c| c.to_digit(16))
                                    .ok_or_else(|| self.err("잘못된 \\u 이스케이프"))?;
                                code = code * 16 + d;
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.err("잘못된 이스케이프")),
                    }
                    self.pos += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
        Ok(out)
    }
    fn parse_number(&mut self) -> Result<VmValue, RuntimeError> {
        let start = self.pos;
        let mut is_float = false;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            match c {
                '0'..='9' => self.pos += 1,
                '.' | 'e' | 'E' | '+' | '-' => {
                    is_float = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        if is_float {
            text.parse::<f64>()
                .map(VmValue::Float)
                .map_err(|_| self.err("잘못된 숫자"))
        } else {
            match text.parse::<i64>() {
                Ok(n) => Ok(VmValue::Int(n)),
                Err(_) => text
                    .parse::<f64>()
                    .map(VmValue::Float)
                    .map_err(|_| self.err("잘못된 숫자")),
            }
        }
    }
    fn parse_array(&mut self) -> Result<VmValue, RuntimeError> {
        self.pos += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(VmValue::List(Arc::new(items)));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("배열에 ',' 또는 ']' 기대")),
            }
        }
        Ok(VmValue::List(Arc::new(items)))
    }
    fn parse_object(&mut self) -> Result<VmValue, RuntimeError> {
        self.pos += 1; // {
        let mut map = crate::omap::OrderedMap::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(VmValue::Map(Arc::new(map)));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some('"') {
                return Err(self.err("객체 키는 문자열"));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(':') {
                return Err(self.err("키 뒤에 ':' 기대"));
            }
            self.pos += 1;
            let val = self.parse_value()?;
            map.insert(encode_str_key(&key), val);
            self.skip_ws();
            match self.peek() {
                Some(',') => {
                    self.pos += 1;
                }
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("객체에 ',' 또는 '}' 기대")),
            }
        }
        Ok(VmValue::Map(Arc::new(map)))
    }
}

/// VmValue → JSON 텍스트. 함수/채널/Future는 직렬화 불가(에러).
pub(crate) fn json_stringify(
    v: &VmValue,
    out: &mut String,
    span: Span,
) -> Result<(), RuntimeError> {
    match v {
        VmValue::Int(n) => {
            out.push_str(&n.to_string());
        }
        VmValue::Float(n) => {
            out.push_str(&fmt_float(*n));
        }
        VmValue::Bool(b) => {
            out.push_str(if *b { "true" } else { "false" });
        }
        VmValue::Nil => {
            out.push_str("null");
        }
        VmValue::Str(s) => json_escape(s, out),
        VmValue::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_stringify(item, out, span)?;
            }
            out.push(']');
        }
        VmValue::Map(m) => {
            out.push('{');
            // 삽입 순서 그대로 (Phase 51 — 이전에는 키 정렬). json_parse → stringify가 순서를 보존한다.
            for (i, (k, val)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_escape(&key_display(k), out);
                out.push(':');
                json_stringify(val, out, span)?;
            }
            out.push('}');
        }
        other => {
            return Err(RuntimeError::new(
                format!(
                    "json_stringify: {} 타입은 직렬화할 수 없음",
                    other.type_name()
                ),
                span,
            ))
        }
    }
    Ok(())
}

fn json_escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
