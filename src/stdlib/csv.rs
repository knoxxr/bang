//! 빌트인: CSV (Phase 52) — RFC 4180 (따옴표 필드, "" 이스케이프, 필드 안 줄바꿈, CRLF/LF)

use super::prelude::*;

/// CSV 텍스트 → 행 목록. 각 행은 문자열 리스트. 마지막 줄바꿈은 빈 행을 만들지 않는다.
pub(crate) fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;
    let mut field_started = false; // 빈 줄과 빈 필드 한 개짜리 행을 구별
    let mut line = 1usize;
    let mut quote_line = 0usize;
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => {
                in_quotes = true;
                field_started = true;
                quote_line = line;
            }
            ',' => {
                row.push(std::mem::take(&mut field));
                field_started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                line += 1;
                if field_started || !field.is_empty() || !row.is_empty() {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                field_started = false;
            }
            _ => {
                field.push(c);
                field_started = true;
            }
        }
    }
    if in_quotes {
        return Err(format!("{quote_line}번째 줄에서 연 따옴표가 닫히지 않음"));
    }
    if field_started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

/// 130: csv_parse(text) → [[str, ...], ...]
pub(crate) fn b_csv_parse(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("csv_parse", &args, 1, span)?;
    let text = str_arg("csv_parse", &args[0], span)?;
    let rows = parse_csv(&text).map_err(|m| RuntimeError::new(format!("csv_parse(): {m}"), span))?;
    Ok(VmValue::List(Arc::new(
        rows.into_iter()
            .map(|r| VmValue::List(Arc::new(r.into_iter().map(VmValue::Str).collect())))
            .collect(),
    )))
}

fn csv_field(v: &VmValue) -> String {
    let s = match v {
        VmValue::Str(s) => s.clone(),
        VmValue::Nil => String::new(),
        other => other.to_string(),
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// 131: csv_stringify(rows) → CSV 텍스트 (행마다 \n, 필요한 필드만 따옴표, nil은 빈 칸)
pub(crate) fn b_csv_stringify(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("csv_stringify", &args, 1, span)?;
    let rows = list_arg("csv_stringify", &args[0], span)?;
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        let row = deep_resolve(row.clone(), span)?;
        let VmValue::List(cells) = row else {
            return Err(RuntimeError::new(
                format!("csv_stringify(): {}번째 행이 List가 아님 ({})", i + 1, row.type_name()),
                span,
            )
            .with_kind(crate::runtime::kind::ARGUMENT));
        };
        let line: Vec<String> = cells.iter().map(csv_field).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    Ok(VmValue::Str(out))
}

#[cfg(test)]
mod tests {
    use super::parse_csv;

    #[test]
    fn parses_rfc4180() {
        let rows = parse_csv("name,note\r\n방,\"a, \"\"b\"\"\nc\"\n,\n").unwrap();
        assert_eq!(rows, vec![vec!["name", "note"], vec!["방", "a, \"b\"\nc"], vec!["", ""]]);
        assert_eq!(parse_csv("").unwrap(), Vec::<Vec<String>>::new());
        assert_eq!(parse_csv("a,b").unwrap(), vec![vec!["a", "b"]]);
        assert_eq!(parse_csv("a\n\nb\n").unwrap(), vec![vec!["a"], vec!["b"]]);
        assert!(parse_csv("a,\"open\n").is_err());
    }
}
