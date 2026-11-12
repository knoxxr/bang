//! 빌트인: 정규식 (엔진은 crate::regex)

use super::prelude::*;

/// 77: regex_match(s, pat) → Bool
pub(crate) fn b_regex_match(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("regex_match", &args, 2, span)?;
    let s = str_arg("regex_match", &args[0], span)?;
    let pat = str_arg("regex_match", &args[1], span)?;
    let re = compile_regex(&pat, span)?;
    Ok(VmValue::Bool(re.is_match(&s.chars().collect::<Vec<_>>())))
}

/// 78: regex_find(s, pat) → 첫 매치 문자열 또는 nil
pub(crate) fn b_regex_find(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("regex_find", &args, 2, span)?;
    let s = str_arg("regex_find", &args[0], span)?;
    let pat = str_arg("regex_find", &args[1], span)?;
    let re = compile_regex(&pat, span)?;
    let chars: Vec<char> = s.chars().collect();
    match re.search(&chars) {
        Some((a, b)) => Ok(VmValue::Str(chars[a..b].iter().collect())),
        None => Ok(VmValue::Nil),
    }
}

/// 79: regex_find_all(s, pat) → List of 매치 문자열
pub(crate) fn b_regex_find_all(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("regex_find_all", &args, 2, span)?;
    let s = str_arg("regex_find_all", &args[0], span)?;
    let pat = str_arg("regex_find_all", &args[1], span)?;
    let re = compile_regex(&pat, span)?;
    let chars: Vec<char> = s.chars().collect();
    let matches: Vec<VmValue> = re
        .find_all(&chars)
        .into_iter()
        .map(|(a, b)| VmValue::Str(chars[a..b].iter().collect()))
        .collect();
    Ok(VmValue::List(Arc::new(matches)))
}

/// 80: regex_replace(s, pat, repl) → 치환된 문자열
pub(crate) fn b_regex_replace(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("regex_replace", &args, 3, span)?;
    let s = str_arg("regex_replace", &args[0], span)?;
    let pat = str_arg("regex_replace", &args[1], span)?;
    let repl = str_arg("regex_replace", &args[2], span)?;
    let re = compile_regex(&pat, span)?;
    let chars: Vec<char> = s.chars().collect();
    Ok(VmValue::Str(re.replace_all(&chars, &repl)))
}

/// 81: regex_groups(s, pat) → [전체매치, g1, g2, ...] 또는 nil
pub(crate) fn b_regex_groups(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("regex_groups", &args, 2, span)?;
    let s = str_arg("regex_groups", &args[0], span)?;
    let pat = str_arg("regex_groups", &args[1], span)?;
    let re = compile_regex(&pat, span)?;
    let chars: Vec<char> = s.chars().collect();
    match re.captures(&chars) {
        Some(groups) => {
            let items: Vec<VmValue> = groups
                .into_iter()
                .map(|g| match g {
                    Some((a, b)) => VmValue::Str(chars[a..b].iter().collect()),
                    None => VmValue::Nil,
                })
                .collect();
            Ok(VmValue::List(Arc::new(items)))
        }
        None => Ok(VmValue::Nil),
    }
}

// ── 구현 헬퍼 (vm.rs에서 이동) ──────────────────────────────────────────────

/// 정규식 패턴 컴파일 (에러는 try/catch로 잡히는 런타임 에러).
pub(crate) fn compile_regex(pat: &str, span: Span) -> Result<crate::regex::Regex, RuntimeError> {
    crate::regex::compile(pat).map_err(|e| RuntimeError::new(format!("정규식 오류: {e}"), span))
}
