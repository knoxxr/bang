//! 빌트인: 프로세스·환경

use super::prelude::*;

/// 109: proc_run(argv) → {code, stdout, stderr}  (셸 미경유)
pub(crate) fn b_proc_run(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("proc_run", &args, 1, span)?;
    proc_run(&args[0], span)
}

/// 110: env_get(name) → Str 또는 nil
pub(crate) fn b_env_get(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("env_get", &args, 1, span)?;
    let name = str_arg("env_get", &args[0], span)?;
    match std::env::var(&name) {
        Ok(v) => Ok(VmValue::Str(v)),
        Err(std::env::VarError::NotPresent) => Ok(VmValue::Nil),
        Err(std::env::VarError::NotUnicode(_)) => Err(RuntimeError::new(
            format!("env_get('{name}'): 값이 유효한 유니코드가 아님"),
            span,
        )),
    }
}

/// 111: env_cwd() → Str
pub(crate) fn b_env_cwd(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("env_cwd", &args, 0, span)?;
    let dir =
        std::env::current_dir().map_err(|e| RuntimeError::new(format!("env_cwd(): {e}"), span))?;
    Ok(VmValue::Str(dir.to_string_lossy().into_owned()))
}

// ── 구현 헬퍼 (vm.rs에서 이동) ──────────────────────────────────────────────

/// proc_run(argv) — 셸을 거치지 않고 argv[0]을 직접 실행. stdin은 연결하지 않는다.
/// 0이 아닌 종료 코드는 에러가 아니라 code로 반환. 시그널 종료 시 code = nil.
/// stdout/stderr의 비-UTF-8 바이트는 U+FFFD로 대체 (바이트 타입 도입 전 한계, SPEC 명시).
pub(crate) fn proc_run(argv: &VmValue, span: Span) -> Result<VmValue, RuntimeError> {
    let items = list_arg("proc_run", argv, span)?;
    let mut parts = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        match it {
            VmValue::Str(s) => parts.push(s.clone()),
            other => {
                return Err(RuntimeError::new(
                    format!(
                        "proc_run(): argv[{i}]는 문자열이어야 함, {} 발견",
                        other.type_name()
                    ),
                    span,
                ))
            }
        }
    }
    let Some((prog, rest)) = parts.split_first() else {
        return Err(RuntimeError::new(
            "proc_run(): argv가 비어 있음 (실행할 명령 필요)",
            span,
        ));
    };
    let out = std::process::Command::new(prog)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| RuntimeError::new(format!("proc_run('{prog}'): {e}"), span))?;
    let code = match out.status.code() {
        Some(c) => VmValue::Int(c as i64),
        None => VmValue::Nil,
    };
    let mut m = crate::omap::OrderedMap::new();
    m.insert("code".to_string(), code);
    m.insert(
        "stdout".to_string(),
        VmValue::Str(String::from_utf8_lossy(&out.stdout).into_owned()),
    );
    m.insert(
        "stderr".to_string(),
        VmValue::Str(String::from_utf8_lossy(&out.stderr).into_owned()),
    );
    Ok(VmValue::Map(Arc::new(m)))
}
