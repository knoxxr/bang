//! 빌트인: 동시성 (채널·wait·parallel_map·select)

use super::prelude::*;

/// 7: channel([cap])
pub(crate) fn b_channel(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    _span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    let cap = args
        .first()
        .and_then(|v| match v {
            VmValue::Int(n) => Some(*n as usize),
            _ => None,
        })
        .unwrap_or(0);
    Ok(VmValue::Channel(Arc::new(BangChannel::new(cap))))
}

/// 8: send(ch, val)
pub(crate) fn b_send(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("send", &args, 2, span)?;
    match &args[0] {
        VmValue::Channel(ch) => {
            ch.send(to_runtime(&args[1], span)?)
                .map_err(|e| RuntimeError::new(e.message, span))?;
            Ok(VmValue::Nil)
        }
        other => Err(RuntimeError::new(
            format!("send(): Channel 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 9: recv(ch)
pub(crate) fn b_recv(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("recv", &args, 1, span)?;
    match &args[0] {
        VmValue::Channel(ch) => Ok(ch.recv().map(from_runtime).unwrap_or(VmValue::Nil)),
        other => Err(RuntimeError::new(
            format!("recv(): Channel 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 10: close(ch)
pub(crate) fn b_close(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("close", &args, 1, span)?;
    match &args[0] {
        VmValue::Channel(ch) => {
            ch.close();
            Ok(VmValue::Nil)
        }
        other => Err(RuntimeError::new(
            format!("close(): Channel 필요, {} 발견", other.type_name()),
            span,
        )),
    }
}

/// 11: parallel_map(list, fn) — sequential in Part A
pub(crate) fn b_parallel_map(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("parallel_map", &args, 2, span)?;
    let list = match &args[0] {
        VmValue::List(v) => (**v).clone(),
        other => {
            return Err(RuntimeError::new(
                format!("parallel_map(): List 필요, {} 발견", other.type_name()),
                span,
            ))
        }
    };
    let func = args[1].clone();
    let mut results = Vec::with_capacity(list.len());
    for item in list {
        let depth_before = vm.frames.len();
        vm.stack.push(func.clone());
        vm.stack.push(item);
        vm.do_call(1, span, stop_depth)?;
        if vm.frames.len() > depth_before {
            // User function: run until it returns
            vm.exec_until(depth_before)?;
        }
        // Return value is on top of stack
        results.push(vm.stack_pop());
    }
    Ok(VmValue::List(Arc::new(results)))
}

/// 12: wait(f) — Part A: value is already resolved
pub(crate) fn b_wait(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("wait", &args, 1, span)?;
    Ok(args[0].clone())
}

/// 103: select(channels) → [index, value] (먼저 준비된 채널), 모두 닫히면 nil
pub(crate) fn b_select(
    _vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    _stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("select", &args, 1, span)?;
    let list = list_arg("select", &args[0], span)?;
    // 모든 인자가 채널인지 확인
    let mut chans = Vec::with_capacity(list.len());
    for v in &list {
        match v {
            VmValue::Channel(c) => chans.push(c.clone()),
            other => {
                return Err(RuntimeError::new(
                    format!("select: 채널 리스트 필요, {} 발견", other.type_name()),
                    span,
                ))
            }
        }
    }
    if chans.is_empty() {
        return Err(RuntimeError::new("select: 빈 채널 리스트", span));
    }
    use crate::runtime::TryRecv;
    loop {
        let mut all_closed = true;
        for (i, ch) in chans.iter().enumerate() {
            match ch.try_recv() {
                TryRecv::Value(v) => {
                    return Ok(VmValue::List(Arc::new(vec![
                        VmValue::Int(i as i64),
                        from_runtime(v),
                    ])));
                }
                TryRecv::Empty => all_closed = false,
                TryRecv::Closed => {}
            }
        }
        if all_closed {
            return Ok(VmValue::Nil); // 모든 채널 닫힘
        }
        // 준비된 채널 없음 → 잠깐 양보 후 재시도 (폴링 select)
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
