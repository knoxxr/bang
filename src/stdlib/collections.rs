//! 빌트인: 리스트·맵·집합

use super::prelude::*;

/// 13: push(list, val)
pub(crate) fn b_push(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("push", &args, 2, span)?;
    match args[0].clone() {
        VmValue::List(mut v) => {
            Arc::make_mut(&mut v).push(args[1].clone());
            Ok(VmValue::List(v))
        }
        other => Err(RuntimeError::new(
            format!("push(): List 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 14: pop(list)
pub(crate) fn b_pop(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("pop", &args, 1, span)?;
    match args[0].clone() {
        VmValue::List(v) => Ok(v.last().cloned().unwrap_or(VmValue::Nil)),
        other => Err(RuntimeError::new(
            format!("pop(): List 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 15: keys(map)
pub(crate) fn b_keys(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("keys", &args, 1, span)?;
    match &args[0] {
        VmValue::Map(m) => {
            // 삽입 순서 (Phase 51 — 이전에는 정렬)
            let keys: Vec<VmValue> = m.keys().map(|k| VmValue::Str(k.clone())).collect();
            Ok(VmValue::List(Arc::new(keys)))
        }
        other => Err(RuntimeError::new(
            format!("keys(): Map 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 16: values(map)
pub(crate) fn b_values(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("values", &args, 1, span)?;
    match &args[0] {
        VmValue::Map(m) => {
            // 삽입 순서 (keys()와 같은 순서)
            Ok(VmValue::List(Arc::new(m.values().cloned().collect())))
        }
        other => Err(RuntimeError::new(
            format!("values(): Map 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 17: range
pub(crate) fn b_range(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    match args.len() {
        1 => {
            let n = as_int(&args[0], span)?;
            Ok(VmValue::List(Arc::new((0..n).map(VmValue::Int).collect())))
        }
        2 => {
            let s = as_int(&args[0], span)?;
            let e = as_int(&args[1], span)?;
            Ok(VmValue::List(Arc::new((s..e).map(VmValue::Int).collect())))
        }
        3 => {
            let s = as_int(&args[0], span)?;
            let e = as_int(&args[1], span)?;
            let step = as_int(&args[2], span)?;
            if step == 0 {
                return Err(RuntimeError::new("range: step는 0이 될 수 없음", span));
            }
            let mut v = Vec::new();
            let mut i = s;
            while (step > 0 && i < e) || (step < 0 && i > e) {
                v.push(VmValue::Int(i));
                i += step;
            }
            Ok(VmValue::List(Arc::new(v)))
        }
        _ => Err(RuntimeError::new("range: 인자 1~3개 필요", span)),
    }
}

/// 33: sort(list) → sorted copy (numbers or strings)
pub(crate) fn b_sort(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sort", &args, 1, span)?;
    let mut list = list_arg("sort", &args[0], span)?;
    let mut err: Option<RuntimeError> = None;
    list.sort_by(|a, b| {
        if err.is_some() {
            return std::cmp::Ordering::Equal;
        }
        match vm_cmp(a, b, span) {
            Ok(o) => o,
            Err(e) => {
                err = Some(e);
                std::cmp::Ordering::Equal
            }
        }
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(VmValue::List(Arc::new(list)))
}

/// 34: reverse(list) → reversed copy
pub(crate) fn b_reverse(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("reverse", &args, 1, span)?;
    let mut list = list_arg("reverse", &args[0], span)?;
    list.reverse();
    Ok(VmValue::List(Arc::new(list)))
}

/// 35: map(list, fn) → List
pub(crate) fn b_map(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("map", &args, 2, span)?;
    let list = list_arg("map", &args[0], span)?;
    let func = args[1].clone();
    let mut results = Vec::with_capacity(list.len());
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(item);
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        results.push(vm.stack_pop());
    }
    Ok(VmValue::List(Arc::new(results)))
}

/// 36: filter(list, fn) → List
pub(crate) fn b_filter(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("filter", &args, 2, span)?;
    let list = list_arg("filter", &args[0], span)?;
    let func = args[1].clone();
    let mut results = Vec::new();
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(item.clone());
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        if vm.stack_pop().is_truthy() {
            results.push(item);
        }
    }
    Ok(VmValue::List(Arc::new(results)))
}

/// 37: reduce(list, fn, init) → value
pub(crate) fn b_reduce(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("reduce", &args, 3, span)?;
    let list = list_arg("reduce", &args[0], span)?;
    let func = args[1].clone();
    let mut acc = args[2].clone();
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(acc);
        vm.stack.push(item);
        vm.do_call(2, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        acc = vm.stack_pop();
    }
    Ok(acc)
}

/// 38: any(list, fn) → Bool
pub(crate) fn b_any(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("any", &args, 2, span)?;
    let list = list_arg("any", &args[0], span)?;
    let func = args[1].clone();
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(item);
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        if vm.stack_pop().is_truthy() {
            return Ok(VmValue::Bool(true));
        }
    }
    Ok(VmValue::Bool(false))
}

/// 39: all(list, fn) → Bool
pub(crate) fn b_all(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("all", &args, 2, span)?;
    let list = list_arg("all", &args[0], span)?;
    let func = args[1].clone();
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(item);
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        if !vm.stack_pop().is_truthy() {
            return Ok(VmValue::Bool(false));
        }
    }
    Ok(VmValue::Bool(true))
}

/// 40: flat(list) → flattened one level
pub(crate) fn b_flat(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("flat", &args, 1, span)?;
    let list = list_arg("flat", &args[0], span)?;
    let mut out = Vec::new();
    for item in list {
        match item {
            VmValue::List(inner) => out.extend(inner.iter().cloned()),
            other => out.push(other),
        }
    }
    Ok(VmValue::List(Arc::new(out)))
}

/// 41: enumerate(list) → List of [i, val]
pub(crate) fn b_enumerate(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("enumerate", &args, 1, span)?;
    let list = list_arg("enumerate", &args[0], span)?;
    let out = list
        .into_iter()
        .enumerate()
        .map(|(i, v)| VmValue::List(Arc::new(vec![VmValue::Int(i as i64), v])))
        .collect();
    Ok(VmValue::List(Arc::new(out)))
}

/// 42: zip(list1, list2) → List of [a, b]
pub(crate) fn b_zip(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("zip", &args, 2, span)?;
    let l1 = list_arg("zip", &args[0], span)?;
    let l2 = list_arg("zip", &args[1], span)?;
    let out = l1
        .into_iter()
        .zip(l2)
        .map(|(a, b)| VmValue::List(Arc::new(vec![a, b])))
        .collect();
    Ok(VmValue::List(Arc::new(out)))
}

/// 43: sum(list) → number
pub(crate) fn b_sum(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sum", &args, 1, span)?;
    let list = list_arg("sum", &args[0], span)?;
    let mut total_i = 0i64;
    let mut total_f = 0.0f64;
    let mut has_float = false;
    for v in list {
        match v {
            VmValue::Int(n) => {
                total_i = total_i
                    .checked_add(n)
                    .ok_or_else(|| overflow_err("sum", span))?
            }
            VmValue::Float(n) => {
                total_f += n;
                has_float = true;
            }
            other => {
                return Err(RuntimeError::new(
                    format!("sum(): 숫자 리스트 필요, {} 발견", other.type_name()),
                    span,
                ))
            }
        }
    }
    if has_float {
        Ok(VmValue::Float(total_i as f64 + total_f))
    } else {
        Ok(VmValue::Int(total_i))
    }
}

/// 58: slice(seq, start, end) — list/str 부분 추출
pub(crate) fn b_slice(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("slice", &args, 3, span)?;
    let start = int_of("slice", &args[1], span)?;
    let end = int_of("slice", &args[2], span)?;
    match &args[0] {
        VmValue::List(items) => {
            let len = items.len() as i64;
            let s = start.clamp(0, len) as usize;
            let e = end.clamp(0, len) as usize;
            let sub = if s < e {
                items[s..e].to_vec()
            } else {
                Vec::new()
            };
            Ok(VmValue::List(Arc::new(sub)))
        }
        VmValue::Str(s0) => {
            let chars: Vec<char> = s0.chars().collect();
            let len = chars.len() as i64;
            let s = start.clamp(0, len) as usize;
            let e = end.clamp(0, len) as usize;
            let sub: String = if s < e {
                chars[s..e].iter().collect()
            } else {
                String::new()
            };
            Ok(VmValue::Str(sub))
        }
        VmValue::Bytes(b) => {
            let len = b.len() as i64;
            let s = start.clamp(0, len) as usize;
            let e = end.clamp(0, len) as usize;
            let sub = if s < e { b[s..e].to_vec() } else { Vec::new() };
            Ok(VmValue::Bytes(Arc::new(sub)))
        }
        other => Err(RuntimeError::new(
            format!("slice: List/Str/Bytes 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 59: has(map, key) → Bool
pub(crate) fn b_has(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("has", &args, 2, span)?;
    match &args[0] {
        VmValue::Map(m) => Ok(VmValue::Bool(m.contains_key(&args[1].to_string()))),
        other => Err(RuntimeError::new(
            format!("has: Map 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 60: get(map, key, default)
pub(crate) fn b_get(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("get", &args, 3, span)?;
    match &args[0] {
        VmValue::Map(m) => Ok(m
            .get(&args[1].to_string())
            .cloned()
            .unwrap_or_else(|| args[2].clone())),
        other => Err(RuntimeError::new(
            format!("get: Map 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 61: merge(map1, map2) → 새 map (map2 우선)
pub(crate) fn b_merge(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("merge", &args, 2, span)?;
    match (&args[0], &args[1]) {
        (VmValue::Map(a), VmValue::Map(b)) => {
            let mut m = (**a).clone();
            for (k, v) in b.iter() {
                m.insert(k.clone(), v.clone());
            }
            Ok(VmValue::Map(Arc::new(m)))
        }
        _ => Err(RuntimeError::new("merge: 두 인자 모두 Map이어야 함", span)),
    }
}

/// 63: index_of(list, x) → 첫 인덱스 또는 -1
pub(crate) fn b_index_of(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("index_of", &args, 2, span)?;
    let list = list_arg("index_of", &args[0], span)?;
    let idx = list
        .iter()
        .position(|e| vm_eq(e, &args[1]))
        .map(|i| i as i64)
        .unwrap_or(-1);
    Ok(VmValue::Int(idx))
}

/// 72: sort_by(list, keyfn) → 키 기준 정렬 사본
pub(crate) fn b_sort_by(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("sort_by", &args, 2, span)?;
    let list = list_arg("sort_by", &args[0], span)?;
    let func = args[1].clone();
    // 각 원소의 키를 계산 (고차 호출)
    let mut keyed: Vec<(VmValue, VmValue)> = Vec::with_capacity(list.len());
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(item.clone());
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            vm.exec_until(depth_before)?;
        }
        let key = vm.stack_pop();
        keyed.push((key, item));
    }
    let mut err: Option<RuntimeError> = None;
    keyed.sort_by(|a, b| {
        if err.is_some() {
            return std::cmp::Ordering::Equal;
        }
        match vm_cmp(&a.0, &b.0, span) {
            Ok(o) => o,
            Err(e) => {
                err = Some(e);
                std::cmp::Ordering::Equal
            }
        }
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(VmValue::List(Arc::new(
        keyed.into_iter().map(|(_, v)| v).collect(),
    )))
}

/// 73: unique(list) → 순서 유지 중복 제거
pub(crate) fn b_unique(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("unique", &args, 1, span)?;
    let list = list_arg("unique", &args[0], span)?;
    let mut out: Vec<VmValue> = Vec::new();
    for item in list {
        if !out.iter().any(|e| vm_eq(e, &item)) {
            out.push(item);
        }
    }
    Ok(VmValue::List(Arc::new(out)))
}

/// 93: union(a, b) → 중복 제거 합집합 (a 순서 후 b의 새 원소)
pub(crate) fn b_union(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("union", &args, 2, span)?;
    let a = list_arg("union", &args[0], span)?;
    let b = list_arg("union", &args[1], span)?;
    let mut out: Vec<VmValue> = Vec::new();
    for item in a.into_iter().chain(b) {
        if !out.iter().any(|e| vm_eq(e, &item)) {
            out.push(item);
        }
    }
    Ok(VmValue::List(Arc::new(out)))
}

/// 94: intersect(a, b) → a 중 b에도 있는 원소 (중복 제거)
pub(crate) fn b_intersect(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("intersect", &args, 2, span)?;
    let a = list_arg("intersect", &args[0], span)?;
    let b = list_arg("intersect", &args[1], span)?;
    let mut out: Vec<VmValue> = Vec::new();
    for item in a {
        if b.iter().any(|e| vm_eq(e, &item)) && !out.iter().any(|e| vm_eq(e, &item)) {
            out.push(item);
        }
    }
    Ok(VmValue::List(Arc::new(out)))
}

/// 95: difference(a, b) → a 중 b에 없는 원소 (중복 제거)
pub(crate) fn b_difference(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("difference", &args, 2, span)?;
    let a = list_arg("difference", &args[0], span)?;
    let b = list_arg("difference", &args[1], span)?;
    let mut out: Vec<VmValue> = Vec::new();
    for item in a {
        if !b.iter().any(|e| vm_eq(e, &item)) && !out.iter().any(|e| vm_eq(e, &item)) {
            out.push(item);
        }
    }
    Ok(VmValue::List(Arc::new(out)))
}
