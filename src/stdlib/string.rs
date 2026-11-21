//! 빌트인: 문자열

use super::prelude::*;

/// 20: split(s, sep)
pub(crate) fn b_split(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("split", &args, 2, span)?;
    let s = str_arg("split", &args[0], span)?;
    let sep = str_arg("split", &args[1], span)?;
    Ok(VmValue::List(Arc::new(
        s.split(sep.as_str())
            .map(|p| VmValue::Str(p.to_string()))
            .collect(),
    )))
}

/// 21: join(list, sep)
pub(crate) fn b_join(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("join", &args, 2, span)?;
    let list = list_arg("join", &args[0], span)?;
    let sep = str_arg("join", &args[1], span)?;
    let parts: Vec<String> = list.iter().map(|v| format!("{v}")).collect();
    Ok(VmValue::Str(parts.join(&sep)))
}

/// 22: trim(s)
pub(crate) fn b_trim(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("trim", &args, 1, span)?;
    Ok(VmValue::Str(
        str_arg("trim", &args[0], span)?.trim().to_string(),
    ))
}

/// 23: trim_start(s)
pub(crate) fn b_trim_start(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("trim_start", &args, 1, span)?;
    Ok(VmValue::Str(
        str_arg("trim_start", &args[0], span)?
            .trim_start()
            .to_string(),
    ))
}

/// 24: trim_end(s)
pub(crate) fn b_trim_end(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("trim_end", &args, 1, span)?;
    Ok(VmValue::Str(
        str_arg("trim_end", &args[0], span)?.trim_end().to_string(),
    ))
}

/// 25: replace(s, from, to)
pub(crate) fn b_replace(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("replace", &args, 3, span)?;
    let s = str_arg("replace", &args[0], span)?;
    let from = str_arg("replace", &args[1], span)?;
    let to = str_arg("replace", &args[2], span)?;
    Ok(VmValue::Str(s.replace(from.as_str(), &to)))
}

/// 26: contains(s, sub)
pub(crate) fn b_contains(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("contains", &args, 2, span)?;
    let s = str_arg("contains", &args[0], span)?;
    let sub = str_arg("contains", &args[1], span)?;
    Ok(VmValue::Bool(s.contains(sub.as_str())))
}

/// 27: starts_with(s, prefix)
pub(crate) fn b_starts_with(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("starts_with", &args, 2, span)?;
    let s = str_arg("starts_with", &args[0], span)?;
    let pre = str_arg("starts_with", &args[1], span)?;
    Ok(VmValue::Bool(s.starts_with(pre.as_str())))
}

/// 28: ends_with(s, suffix)
pub(crate) fn b_ends_with(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("ends_with", &args, 2, span)?;
    let s = str_arg("ends_with", &args[0], span)?;
    let suf = str_arg("ends_with", &args[1], span)?;
    Ok(VmValue::Bool(s.ends_with(suf.as_str())))
}

/// 29: upper(s)
pub(crate) fn b_upper(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("upper", &args, 1, span)?;
    Ok(VmValue::Str(
        str_arg("upper", &args[0], span)?.to_uppercase(),
    ))
}

/// 30: lower(s)
pub(crate) fn b_lower(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("lower", &args, 1, span)?;
    Ok(VmValue::Str(
        str_arg("lower", &args[0], span)?.to_lowercase(),
    ))
}

/// 31: find(s, sub) → Int (-1 if not found)
pub(crate) fn b_find(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("find", &args, 2, span)?;
    let s = str_arg("find", &args[0], span)?;
    let sub = str_arg("find", &args[1], span)?;
    let idx = s
        .find(sub.as_str())
        .map(|b| s[..b].chars().count() as i64)
        .unwrap_or(-1);
    Ok(VmValue::Int(idx))
}

/// 32: chars(s) → List of single-char strings
pub(crate) fn b_chars(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("chars", &args, 1, span)?;
    let s = str_arg("chars", &args[0], span)?;
    Ok(VmValue::List(Arc::new(
        s.chars().map(|c| VmValue::Str(c.to_string())).collect(),
    )))
}

/// 62: repeat(str, n)
pub(crate) fn b_repeat(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("repeat", &args, 2, span)?;
    let s = str_arg("repeat", &args[0], span)?;
    let n = int_of("repeat", &args[1], span)?;
    if n < 0 {
        return Err(RuntimeError::new("repeat: n은 음수가 될 수 없음", span));
    }
    Ok(VmValue::Str(s.repeat(n as usize)))
}

/// 75: ord(str) → 첫 문자의 코드포인트
pub(crate) fn b_ord(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("ord", &args, 1, span)?;
    let s = str_arg("ord", &args[0], span)?;
    match s.chars().next() {
        Some(c) => Ok(VmValue::Int(c as i64)),
        None => Err(RuntimeError::new("ord: 빈 문자열", span)),
    }
}

/// 76: chr(int) → 한 문자 문자열
pub(crate) fn b_chr(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("chr", &args, 1, span)?;
    let n = int_of("chr", &args[0], span)?;
    match u32::try_from(n).ok().and_then(char::from_u32) {
        Some(c) => Ok(VmValue::Str(c.to_string())),
        None => Err(RuntimeError::new(
            format!("chr: 잘못된 코드포인트 {n}"),
            span,
        )),
    }
}

// ============================================================================
// 형식 지정 (Phase 50) — f-문자열 `{x:spec}`과 format()이 공유
// ============================================================================

/// 형식 지정자 `[[채움]정렬][0][너비][,][.정밀도][종류]`.
/// 정렬: `<` 왼쪽, `>` 오른쪽, `^` 가운데. 종류: `f` 고정소수점, `d` 정수, `x` 16진, `b` 2진, `%` 백분율.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormatSpec {
    pub fill: char,
    pub align: Option<char>,
    pub zero: bool,
    pub width: usize,
    pub grouping: bool,
    pub precision: Option<usize>,
    pub ty: Option<char>,
}

pub fn parse_format_spec(spec: &str) -> Result<FormatSpec, String> {
    let c: Vec<char> = spec.chars().collect();
    let mut i = 0;
    let mut fs = FormatSpec { fill: ' ', ..Default::default() };
    let is_align = |ch: char| matches!(ch, '<' | '>' | '^');
    if c.len() >= 2 && is_align(c[1]) {
        fs.fill = c[0];
        fs.align = Some(c[1]);
        i = 2;
    } else if !c.is_empty() && is_align(c[0]) {
        fs.align = Some(c[0]);
        i = 1;
    }
    if c.get(i) == Some(&'0') {
        fs.zero = true;
        i += 1;
    }
    let mut w = String::new();
    while let Some(d) = c.get(i).filter(|d| d.is_ascii_digit()) {
        w.push(*d);
        i += 1;
    }
    if !w.is_empty() {
        fs.width = w.parse().map_err(|_| "너비가 너무 큼".to_string())?;
        if fs.width > 10_000 {
            return Err("너비가 너무 큼 (최대 10000)".into());
        }
    }
    if c.get(i) == Some(&',') {
        fs.grouping = true;
        i += 1;
    }
    if c.get(i) == Some(&'.') {
        i += 1;
        let mut p = String::new();
        while let Some(d) = c.get(i).filter(|d| d.is_ascii_digit()) {
            p.push(*d);
            i += 1;
        }
        if p.is_empty() {
            return Err("'.' 뒤에 정밀도 숫자가 필요함".into());
        }
        let p: usize = p.parse().map_err(|_| "정밀도가 너무 큼".to_string())?;
        if p > 100 {
            return Err("정밀도가 너무 큼 (최대 100)".into());
        }
        fs.precision = Some(p);
    }
    if let Some(&t) = c.get(i) {
        if !matches!(t, 'f' | 'd' | 'x' | 'b' | '%') {
            return Err(format!("알 수 없는 종류 '{t}' (f d x b % 중 하나)"));
        }
        fs.ty = Some(t);
        i += 1;
    }
    if i != c.len() {
        return Err("해석할 수 없는 형식".into());
    }
    Ok(fs)
}

/// 정수 부분에 천 단위 콤마.
fn group_thousands(digits: &str) -> String {
    let (sign, rest) = match digits.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", digits),
    };
    let (int_part, frac) = match rest.find('.') {
        Some(p) => (&rest[..p], &rest[p..]),
        None => (rest, ""),
    };
    let mut out = String::new();
    for (n, ch) in int_part.chars().enumerate() {
        if n > 0 && (int_part.len() - n) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{sign}{out}{frac}")
}

/// 값 하나를 형식 지정자로 문자열로 만든다. 실패 메시지는 호출자가 에러로 감싼다.
pub fn apply_format(v: &VmValue, spec: &FormatSpec) -> Result<String, String> {
    let is_num = matches!(v, VmValue::Int(_) | VmValue::Float(_));
    let as_f = |v: &VmValue| match v {
        VmValue::Int(n) => Some(*n as f64),
        VmValue::Float(x) => Some(*x),
        _ => None,
    };
    let mut body = match spec.ty {
        Some('f') | Some('%') => {
            let x = as_f(v).ok_or_else(|| format!("'{}' 형식은 숫자 필요, {} 발견", spec.ty.unwrap_or('f'), v.type_name()))?;
            let p = spec.precision.unwrap_or(6);
            if spec.ty == Some('%') { format!("{:.*}%", p, x * 100.0) } else { format!("{x:.p$}") }
        }
        Some('d') => match v {
            VmValue::Int(n) => n.to_string(),
            other => return Err(format!("'d' 형식은 Int 필요, {} 발견", other.type_name())),
        },
        Some('x') | Some('b') => match v {
            VmValue::Int(n) if *n >= 0 => if spec.ty == Some('x') { format!("{n:x}") } else { format!("{n:b}") },
            VmValue::Int(n) => if spec.ty == Some('x') { format!("-{:x}", n.unsigned_abs()) } else { format!("-{:b}", n.unsigned_abs()) },
            other => return Err(format!("'{}' 형식은 Int 필요, {} 발견", spec.ty.unwrap_or('x'), other.type_name())),
        },
        _ => match (v, spec.precision) {
            // 종류 없이 정밀도만: 숫자는 고정소수점, 문자열은 앞에서 자름
            (VmValue::Int(_) | VmValue::Float(_), Some(p)) => format!("{:.p$}", as_f(v).unwrap_or(0.0)),
            (VmValue::Str(s), Some(p)) => s.chars().take(p).collect(),
            _ => format!("{v}"),
        },
    };
    if spec.grouping {
        if !is_num {
            return Err(format!("',' 형식은 숫자 필요, {} 발견", v.type_name()));
        }
        let pct = body.ends_with('%');
        let core = body.trim_end_matches('%').to_string();
        body = group_thousands(&core) + if pct { "%" } else { "" };
    }
    let len = body.chars().count();
    if len >= spec.width {
        return Ok(body);
    }
    let pad = spec.width - len;
    if spec.zero && spec.align.is_none() && is_num {
        // 부호 뒤에 0 채움: -0042
        let (sign, rest) = match body.strip_prefix('-') {
            Some(r) => ("-", r.to_string()),
            None => ("", body.clone()),
        };
        return Ok(format!("{sign}{}{rest}", "0".repeat(pad)));
    }
    let fill = if spec.zero && spec.align.is_none() { '0' } else { spec.fill };
    let align = spec.align.unwrap_or(if is_num { '>' } else { '<' });
    let f = |n: usize| fill.to_string().repeat(n);
    Ok(match align {
        '>' => format!("{}{body}", f(pad)),
        '^' => format!("{}{body}{}", f(pad / 2), f(pad - pad / 2)),
        _ => format!("{body}{}", f(pad)),
    })
}

/// 124: format(template, args...) — `{}` / `{:형식}` 자리에 인자를 차례로 넣는다. `{{` `}}`는 중괄호 글자.
pub(crate) fn b_format(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    use crate::runtime::kind;
    let Some(first) = args.first() else {
        return Err(RuntimeError::new("format(): 형식 문자열 필요", span).with_kind(kind::ARGUMENT));
    };
    let tpl = str_arg("format", first, span)?;
    let vals = &args[1..];
    let c: Vec<char> = tpl.chars().collect();
    let mut out = String::new();
    let mut next = 0usize;
    let mut i = 0;
    let verr = |m: String| RuntimeError::new(format!("format(): {m}"), span).with_kind(kind::VALUE);
    while i < c.len() {
        match c[i] {
            '{' if c.get(i + 1) == Some(&'{') => { out.push('{'); i += 2; }
            '}' if c.get(i + 1) == Some(&'}') => { out.push('}'); i += 2; }
            '}' => return Err(verr("짝 없는 '}' — 글자 중괄호는 '}}'".into())),
            '{' => {
                let close = c[i..].iter().position(|&x| x == '}').map(|p| i + p)
                    .ok_or_else(|| verr("'{' 가 닫히지 않음".into()))?;
                let inner: String = c[i + 1..close].iter().collect();
                let spec_src = match inner.strip_prefix(':') {
                    Some(sp) => sp.to_string(),
                    None if inner.is_empty() => String::new(),
                    None => return Err(verr(format!("자리 표시는 {{}} 또는 {{:형식}} 만 가능, '{{{inner}}}' 발견"))),
                };
                let spec = parse_format_spec(&spec_src).map_err(|m| verr(format!("형식 '{spec_src}': {m}")))?;
                let v = vals.get(next).ok_or_else(|| {
                    RuntimeError::new(format!("format(): 자리 표시가 인자({}개)보다 많음", vals.len()), span)
                        .with_kind(kind::ARGUMENT)
                })?;
                let v = deep_resolve(v.clone(), span)?;
                out.push_str(&apply_format(&v, &spec).map_err(verr)?);
                next += 1;
                i = close + 1;
            }
            ch => { out.push(ch); i += 1; }
        }
    }
    if next != vals.len() {
        return Err(RuntimeError::new(
            format!("format(): 인자 {}개 중 {}개만 사용됨 (자리 표시 수와 맞춰야 함)", vals.len(), next), span)
            .with_kind(kind::ARGUMENT));
    }
    Ok(VmValue::Str(out))
}
