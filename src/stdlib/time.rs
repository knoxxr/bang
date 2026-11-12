//! 빌트인: 시간·난수·대기

use super::prelude::*;

/// 66: now_ms() → epoch millis
pub(crate) fn b_now_ms(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("now_ms", &args, 0, span)?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(VmValue::Int(ms))
}

/// 67: random() → [0,1) float
pub(crate) fn b_random(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("random", &args, 0, span)?;
    Ok(VmValue::Float(next_random()))
}

/// 68: random_int(lo, hi) → [lo, hi] 정수 (포함)
pub(crate) fn b_random_int(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("random_int", &args, 2, span)?;
    let lo = int_of("random_int", &args[0], span)?;
    let hi = int_of("random_int", &args[1], span)?;
    if hi < lo {
        return Err(RuntimeError::new("random_int: hi >= lo 이어야 함", span));
    }
    let range = (hi - lo + 1) as f64;
    let n = lo + (next_random() * range) as i64;
    Ok(VmValue::Int(n.min(hi)))
}

/// 74: format_time(ms) → "YYYY-MM-DD HH:MM:SS" (UTC)
pub(crate) fn b_format_time(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("format_time", &args, 1, span)?;
    let ms = int_of("format_time", &args[0], span)?;
    Ok(VmValue::Str(format_time_utc(ms)))
}

/// 112: sleep(ms) → nil  (현재 작업만 대기 — 탄력 풀이 다른 작업 처리)
pub(crate) fn b_sleep(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sleep", &args, 1, span)?;
    let ms = int_of("sleep", &args[0], span)?;
    if ms < 0 {
        return Err(RuntimeError::new(
            format!("sleep(): 음수 시간은 허용되지 않음: {ms}"),
            span,
        ));
    }
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    Ok(VmValue::Nil)
}

// ── 구현 헬퍼 (vm.rs에서 이동) ──────────────────────────────────────────────

// ── PRNG (xorshift64, 시간 시드) ────────────────────────────────────────────
fn rng_state() -> &'static Mutex<u64> {
    static S: OnceLock<Mutex<u64>> = OnceLock::new();
    S.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15)
            | 1;
        Mutex::new(seed)
    })
}

/// [0, 1) 범위의 난수.
pub(crate) fn next_random() -> f64 {
    let mut guard = match rng_state().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let mut x = *guard;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *guard = x;
    ((x >> 11) as f64) / ((1u64 << 53) as f64)
}

/// epoch 밀리초 → "YYYY-MM-DD HH:MM:SS" (UTC). 외부 의존성 없이 civil-date 변환.
pub(crate) fn format_time_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's days_from_civil 역변환 (civil_from_days)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}
