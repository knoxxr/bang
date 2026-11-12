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
