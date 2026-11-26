//! 빌트인: 수학

use super::prelude::*;

/// 44: abs(x)
pub(crate) fn b_abs(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("abs", &args, 1, span)?;
    match &args[0] {
        VmValue::Int(n) => n
            .checked_abs()
            .map(VmValue::Int)
            .ok_or_else(|| overflow_err("abs", span)),
        VmValue::Float(n) => Ok(VmValue::Float(n.abs())),
        other => Err(RuntimeError::new(
            format!("abs(): 숫자 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 45: sqrt(x)
pub(crate) fn b_sqrt(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sqrt", &args, 1, span)?;
    let n = as_float(&args[0], span)?;
    Ok(VmValue::Float(n.sqrt()))
}

/// 46: floor(x)
pub(crate) fn b_floor(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("floor", &args, 1, span)?;
    let n = as_float(&args[0], span)?;
    Ok(VmValue::Int(n.floor() as i64))
}

/// 47: ceil(x)
pub(crate) fn b_ceil(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("ceil", &args, 1, span)?;
    let n = as_float(&args[0], span)?;
    Ok(VmValue::Int(n.ceil() as i64))
}

/// 48: round(x)
pub(crate) fn b_round(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("round", &args, 1, span)?;
    let n = as_float(&args[0], span)?;
    Ok(VmValue::Int(n.round() as i64))
}

/// 49: pow(base, exp)
pub(crate) fn b_pow(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("pow", &args, 2, span)?;
    let base = as_float(&args[0], span)?;
    let exp = as_float(&args[1], span)?;
    let result = base.powf(exp);
    if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
        if let (VmValue::Int(_), VmValue::Int(e)) = (&args[0], &args[1]) {
            if *e >= 0 {
                return Ok(VmValue::Int(result as i64));
            }
        }
    }
    Ok(VmValue::Float(result))
}

/// 50: min(a, b) or min(list)
pub(crate) fn b_min(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    match args.len() {
        1 => {
            let list = list_arg("min", &args[0], span)?;
            if list.is_empty() {
                return Err(RuntimeError::new("min(): 빈 리스트", span));
            }
            let mut m = list[0].clone();
            for v in &list[1..] {
                if vm_cmp(v, &m, span)? == std::cmp::Ordering::Less {
                    m = v.clone();
                }
            }
            Ok(m)
        }
        2 => {
            let a = args[0].clone();
            let b = args[1].clone();
            if vm_cmp(&a, &b, span)? == std::cmp::Ordering::Less {
                Ok(a)
            } else {
                Ok(b)
            }
        }
        _ => Err(RuntimeError::new(
            "min(): 인자 1개(리스트) 또는 2개 필요",
            span,
        )),
    }
}

/// 51: max(a, b) or max(list)
pub(crate) fn b_max(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    match args.len() {
        1 => {
            let list = list_arg("max", &args[0], span)?;
            if list.is_empty() {
                return Err(RuntimeError::new("max(): 빈 리스트", span));
            }
            let mut m = list[0].clone();
            for v in &list[1..] {
                if vm_cmp(v, &m, span)? == std::cmp::Ordering::Greater {
                    m = v.clone();
                }
            }
            Ok(m)
        }
        2 => {
            let a = args[0].clone();
            let b = args[1].clone();
            if vm_cmp(&a, &b, span)? == std::cmp::Ordering::Greater {
                Ok(a)
            } else {
                Ok(b)
            }
        }
        _ => Err(RuntimeError::new(
            "max(): 인자 1개(리스트) 또는 2개 필요",
            span,
        )),
    }
}

/// 82: gcd(a, b)
pub(crate) fn b_gcd(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("gcd", &args, 2, span)?;
    // unsigned_abs: i64::MIN.abs() 오버플로 회피. 결과가 i64 범위를 넘으면 에러.
    let mut a = int_of("gcd", &args[0], span)?.unsigned_abs();
    let mut b = int_of("gcd", &args[1], span)?.unsigned_abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    i64::try_from(a)
        .map(VmValue::Int)
        .map_err(|_| overflow_err("gcd", span))
}

/// 83: clamp(x, lo, hi) → 원본 타입 유지
pub(crate) fn b_clamp(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("clamp", &args, 3, span)?;
    if vm_cmp(&args[0], &args[1], span)? == Ordering::Less {
        Ok(args[1].clone())
    } else if vm_cmp(&args[0], &args[2], span)? == Ordering::Greater {
        Ok(args[2].clone())
    } else {
        Ok(args[0].clone())
    }
}

/// 84: sign(x) → -1 / 0 / 1
pub(crate) fn b_sign(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sign", &args, 1, span)?;
    let x = num_of("sign", &args[0], span)?;
    Ok(VmValue::Int(if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }))
}

/// 85: sin
pub(crate) fn b_sin(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sin", &args, 1, span)?;
    Ok(VmValue::Float(num_of("sin", &args[0], span)?.sin()))
}

/// 86: cos
pub(crate) fn b_cos(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("cos", &args, 1, span)?;
    Ok(VmValue::Float(num_of("cos", &args[0], span)?.cos()))
}

/// 87: tan
pub(crate) fn b_tan(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("tan", &args, 1, span)?;
    Ok(VmValue::Float(num_of("tan", &args[0], span)?.tan()))
}

/// 88: log
pub(crate) fn b_log(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("log", &args, 1, span)?;
    Ok(VmValue::Float(num_of("log", &args[0], span)?.ln()))
}

/// 89: log10
pub(crate) fn b_log10(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("log10", &args, 1, span)?;
    Ok(VmValue::Float(num_of("log10", &args[0], span)?.log10()))
}

/// 90: exp
pub(crate) fn b_exp(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("exp", &args, 1, span)?;
    Ok(VmValue::Float(num_of("exp", &args[0], span)?.exp()))
}

/// 91: pi
pub(crate) fn b_pi(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("pi", &args, 0, span)?;
    Ok(VmValue::Float(std::f64::consts::PI))
}

/// 92: e
pub(crate) fn b_e(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("e", &args, 0, span)?;
    Ok(VmValue::Float(std::f64::consts::E))
}

/// 134: idiv(a, b) → 내림 정수 나눗셈 (Phase 57). `/`는 항상 float이므로 정수 몫은 이것으로.
/// int끼리는 int (음수는 아래로 내림: idiv(-7, 2) = -4, Python의 //와 같음), float이 섞이면 floor한 float.
pub(crate) fn b_idiv(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    use crate::runtime::kind;
    req_args("idiv", &args, 2, span)?;
    let zero = || RuntimeError::new("idiv(): 0으로 나눌 수 없음", span).with_kind(kind::ZERO_DIVISION);
    match (&args[0], &args[1]) {
        (VmValue::Int(a), VmValue::Int(b)) => {
            if *b == 0 {
                return Err(zero());
            }
            let q = a.checked_div(*b).ok_or_else(|| overflow_err("idiv", span))?;
            // 나머지가 있고 부호가 다르면 한 칸 아래로 (truncation → floor)
            let q = if a % b != 0 && ((*a < 0) != (*b < 0)) { q - 1 } else { q };
            Ok(VmValue::Int(q))
        }
        (a, b) => {
            let x = num_of("idiv", a, span)?;
            let y = num_of("idiv", b, span)?;
            if y == 0.0 {
                return Err(zero());
            }
            Ok(VmValue::Float((x / y).floor()))
        }
    }
}
