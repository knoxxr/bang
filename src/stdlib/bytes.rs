//! 빌트인: 바이트 값 타입

use super::prelude::*;

/// 117: bytes_from_str(s) → Bytes (UTF-8 인코딩)
pub(crate) fn b_bytes_from_str(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("bytes_from_str", &args, 1, span)?;
    let s = str_arg("bytes_from_str", &args[0], span)?;
    Ok(VmValue::Bytes(Arc::new(s.into_bytes())))
}

/// 118: bytes_to_str(b) → Str (유효한 UTF-8이 아니면 에러)
pub(crate) fn b_bytes_to_str(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("bytes_to_str", &args, 1, span)?;
    let b = bytes_arg("bytes_to_str", &args[0], span)?;
    let s = std::str::from_utf8(b).map_err(|e| {
        RuntimeError::new(format!("bytes_to_str(): 유효한 UTF-8이 아님 ({e})"), span)
    })?;
    Ok(VmValue::Str(s.to_string()))
}

/// 119: bytes_from_hex(s) → Bytes ("48656c6c6f", 대소문자 무관)
pub(crate) fn b_bytes_from_hex(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("bytes_from_hex", &args, 1, span)?;
    let s = str_arg("bytes_from_hex", &args[0], span)?;
    Ok(VmValue::Bytes(Arc::new(hex_decode(&s, span)?)))
}

/// 120: bytes_to_hex(b) → Str (소문자)
pub(crate) fn b_bytes_to_hex(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("bytes_to_hex", &args, 1, span)?;
    let b = bytes_arg("bytes_to_hex", &args[0], span)?;
    let mut out = String::with_capacity(b.len() * 2);
    for x in b {
        out.push_str(&format!("{x:02x}"));
    }
    Ok(VmValue::Str(out))
}

// ── 구현 헬퍼 (vm.rs에서 이동) ──────────────────────────────────────────────

pub(crate) fn hex_decode(s: &str, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let digits = s.as_bytes();
    if digits.len() % 2 == 1 {
        // (is_multiple_of는 Rust 1.87+ — GUIDE의 1.82 지원 유지)
        return Err(RuntimeError::new(
            format!(
                "bytes_from_hex(): 16진수 자릿수가 홀수 ({}자)",
                digits.len()
            ),
            span,
        ));
    }
    let nib = |c: u8, pos: usize| -> Result<u8, RuntimeError> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(RuntimeError::new(
                format!("bytes_from_hex(): {pos}번째 문자가 16진수가 아님"),
                span,
            )),
        }
    };
    digits
        .chunks(2)
        .enumerate()
        .map(|(i, p)| Ok(nib(p[0], i * 2)? << 4 | nib(p[1], i * 2 + 1)?))
        .collect()
}
