//! 빌트인: 기본·변환·I/O·모듈

use super::prelude::*;

/// 0: print — Future 깊이 해소
pub(crate) fn b_print(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    let mut parts = Vec::with_capacity(args.len());
    for v in args {
        let resolved = deep_resolve(v, span)?;
        parts.push(format!("{resolved}"));
    }
    let line = parts.join(" ");
    vm.output
        .write_line(line)
        .map_err(|m| RuntimeError::new(format!("print(): {m}"), span).with_kind(crate::runtime::kind::IO))?;
    Ok(VmValue::Nil)
}

/// 1: str(x)
pub(crate) fn b_str(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("str", &args, 1, span)?;
    Ok(VmValue::Str(format!("{}", args[0])))
}

/// 2: int(x)
pub(crate) fn b_int(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("int", &args, 1, span)?;
    match &args[0] {
        VmValue::Int(n) => Ok(VmValue::Int(*n)),
        VmValue::Float(n) => Ok(VmValue::Int(*n as i64)),
        VmValue::Bool(b) => Ok(VmValue::Int(if *b { 1 } else { 0 })),
        VmValue::Str(s) => s
            .trim()
            .parse::<i64>()
            .map(VmValue::Int)
            .map_err(|_| RuntimeError::new(format!("int 변환 실패: '{s}'"), span)),
        other => Err(RuntimeError::new(
            format!("int(): {} 변환 불가", other.type_name()),
            span,
        )),
    }
}

/// 3: float(x)
pub(crate) fn b_float(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("float", &args, 1, span)?;
    match &args[0] {
        VmValue::Float(n) => Ok(VmValue::Float(*n)),
        VmValue::Int(n) => Ok(VmValue::Float(*n as f64)),
        VmValue::Str(s) => s
            .trim()
            .parse::<f64>()
            .map(VmValue::Float)
            .map_err(|_| RuntimeError::new(format!("float 변환 실패: '{s}'"), span)),
        other => Err(RuntimeError::new(
            format!("float(): {} 변환 불가", other.type_name()),
            span,
        )),
    }
}

/// 4: bool(x)
pub(crate) fn b_bool(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("bool", &args, 1, span)?;
    Ok(VmValue::Bool(args[0].is_truthy()))
}

/// 5: len(x)
pub(crate) fn b_len(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("len", &args, 1, span)?;
    match &args[0] {
        VmValue::List(v) => Ok(VmValue::Int(v.len() as i64)),
        VmValue::Str(s) => Ok(VmValue::Int(s.chars().count() as i64)),
        VmValue::Map(m) => Ok(VmValue::Int(m.len() as i64)),
        VmValue::Bytes(b) => Ok(VmValue::Int(b.len() as i64)),
        other => Err(RuntimeError::new(
            format!("len(): {} 지원 안 됨", other.type_name()),
            span,
        )),
    }
}

/// 6: type(x)
pub(crate) fn b_type(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("type", &args, 1, span)?;
    Ok(VmValue::Str(args[0].type_name().to_string()))
}

/// 18: assert
/// 123: assert_eq(actual, expected) — 같지 않으면 두 값을 보여 주는 에러 (Phase 49, `bang test`용).
pub(crate) fn b_assert_eq(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("assert_eq", &args, 2, span)?;
    let actual = deep_resolve(args[0].clone(), span)?;
    let expected = deep_resolve(args[1].clone(), span)?;
    if vm_eq(&actual, &expected) {
        return Ok(VmValue::Nil);
    }
    // 문자열은 따옴표로 감싸 1과 "1"을 구별해 보여 준다
    let show = |v: &VmValue| match v {
        VmValue::Str(s) => format!("{s:?}"),
        other => format!("{other}"),
    };
    Err(RuntimeError::new(
        format!("assert_eq 실패: 기대 {}, 실제 {}", show(&expected), show(&actual)),
        span,
    ))
}

pub(crate) fn b_assert(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if args.is_empty() {
        return Err(RuntimeError::new("assert: 인자 필요", span));
    }
    if !args[0].is_truthy() {
        let msg = args.get(1).map(|v| format!("{v}")).unwrap_or_default();
        return Err(RuntimeError::new(
            format!(
                "assertion 실패{}",
                if msg.is_empty() {
                    String::new()
                } else {
                    format!(": {msg}")
                }
            ),
            span,
        ));
    }
    Ok(VmValue::Nil)
}

/// 19: exit
pub(crate) fn b_exit(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    // 정수가 아닌 종료 코드는 에러 (Phase 61: 이전에는 조용히 0으로 종료 — 실패가 성공으로 보였다)
    let code = match args.first() {
        None => 0,
        Some(v) => int_of("exit", v, span)? as i32,
    };
    std::process::exit(code);
}

/// 54: input(prompt?) → Str
pub(crate) fn b_input(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    if args.len() > 1 {
        return Err(RuntimeError::new("input(): 인자 0 또는 1개 필요", span));
    }
    if let Some(prompt) = args.first() {
        print!("{prompt}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| RuntimeError::new(format!("input(): {e}"), span))?;
    Ok(VmValue::Str(
        line.trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string(),
    ))
}

/// 55: print_err(...) → nil
pub(crate) fn b_print_err(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    let mut parts = Vec::with_capacity(args.len());
    for v in args {
        let resolved = deep_resolve(v, span)?;
        parts.push(format!("{resolved}"));
    }
    eprintln!("{}", parts.join(" "));
    Ok(VmValue::Nil)
}

/// 56: args() → List of CLI args
pub(crate) fn b_args(
    _vm: &mut Vm,
    _args: Vec<VmValue>,
    _span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    let cli_args: Vec<VmValue> = std::env::args().map(VmValue::Str).collect();
    Ok(VmValue::List(Arc::new(cli_args)))
}

/// 57: import(spec) → Map of module exports (캐시된 싱글톤)
pub(crate) fn b_import(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("import", &args, 1, span)?;
    let spec = str_arg("import", &args[0], span)?;
    // 이름 해석: 바레 이름은 bang_modules/ 및 BANG_PATH에서 검색,
    // .bang/경로 구분자 포함이면 직접 경로.
    let path = resolve_module(&spec).ok_or_else(|| {
        RuntimeError::new(
            format!("import(): 모듈을 찾을 수 없음: '{spec}' (bang_modules/ 또는 BANG_PATH 확인)"),
            span,
        )
    })?;
    let path = path.to_string_lossy().into_owned();
    // 캐시 키: 정규화된 절대경로 (실패 시 해석된 경로)
    let key = std::fs::canonicalize(&path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.clone());
    if let Some(cached) = module_cache().lock().unwrap().get(&key).cloned() {
        return Ok(cached); // 이미 로드됨 → 같은 모듈 인스턴스 반환
    }
    // 순환 import 감지: 로딩 중인 모듈을 다시 import하면 무한 재귀 대신 에러
    if !module_loading().lock().unwrap().insert(key.clone()) {
        return Err(RuntimeError::new(
            format!("import(): 순환 import 감지: '{path}'"),
            span,
        ));
    }
    let result = vm.import_module(&path, &key, span);
    module_loading().lock().unwrap().remove(&key);
    result
}
