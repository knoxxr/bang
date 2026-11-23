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

/// parallel_map이 한 번에 만드는 작업 수 상한. 원소가 더 많으면 연속 구간으로 나눠
/// 작업 하나가 여러 원소를 순서대로 처리한다 (작업 생성 비용 억제).
const PAR_MAP_MAX_TASKS: usize = 256;

/// 11: parallel_map(list, fn) — 원소마다 fn을 병렬 실행하고 결과를 원래 순서로 모은다.
/// 각 호출은 spawn과 같은 격리(클로저·전역 깊은 복사)로 실행되며, 반환 전에 모든 작업을
/// 조인한다(구조적 동시성). 에러가 나면 전부 조인한 뒤 원래 순서상 첫 에러를 던진다.
/// 사용자 클로저가 아닌 함수(빌트인)는 순차 실행한다.
pub(crate) fn b_parallel_map(
    vm: &mut Vm,
    args: Vec<VmValue>,
    span: Span,
    stop_depth: usize,
) -> Result<VmValue, RuntimeError> {
    req_args("parallel_map", &args, 2, span)?;
    let list = match &args[0] {
        VmValue::List(v) => v.clone(),
        other => {
            return Err(RuntimeError::new(
                format!("parallel_map(): List 필요, {} 발견", other.type_name()),
                span,
            ))
        }
    };
    let func = args[1].clone();
    let closure = match &func {
        VmValue::Closure(c) => c.clone(),
        _ => {
            let mut results = Vec::with_capacity(list.len());
            for item in list.iter() {
                let depth_before = vm.frames.len();
                vm.stack.push(func.clone());
                vm.stack.push(item.clone());
                vm.do_call(1, span, stop_depth)?;
                if vm.frames.len() > depth_before {
                    vm.exec_until(depth_before)?;
                }
                results.push(vm.stack_pop());
            }
            return Ok(VmValue::List(Arc::new(results)));
        }
    };
    let n = list.len();
    let tasks = n.min(PAR_MAP_MAX_TASKS);
    let mut futures = Vec::with_capacity(tasks);
    for t in 0..tasks {
        let (lo, hi) = (t * n / tasks, (t + 1) * n / tasks);
        // 값 의미론: 원소마다 독립된 클로저 복사본 (spawn과 동일한 격리)
        #[allow(clippy::type_complexity)]
        let jobs: Vec<(Arc<VmClosure>, VmValue, Vec<StateCell>)> = list[lo..hi]
            .iter()
            .map(|item| {
                // 원소마다 독립 격리 (원소 안의 객체도 복사 — Phase 53)
                let mut iso = Isolator::new();
                let c = iso.closure(&closure);
                let it = iso.isolate(item.clone());
                iso.finish();
                let cells = iso.new_cells();
                (c, it, cells)
            })
            .collect();
        let output = vm.output.clone();
        let future = VmFuture::new();
        let done = future.clone();
        crate::scheduler::global().spawn_task(move || {
            let mut out = Vec::with_capacity(jobs.len());
            for (c, item, cells) in jobs {
                let r = Vm::run_spawned(output.clone(), c, vec![item], span);
                // 원소용으로 복사한 상태의 순환 정리 (Phase 60)
                crate::cycles::collect(cells);
                match r {
                    Ok(v) => out.push(v),
                    Err(e) => {
                        done.complete(Err(e));
                        return;
                    }
                }
            }
            done.complete(Ok(VmValue::List(Arc::new(out))));
        });
        futures.push(future);
    }
    let mut results = Vec::with_capacity(n);
    let mut first_err = None;
    for f in futures {
        match f.resolve() {
            Ok(VmValue::List(part)) => results.extend(part.iter().cloned()),
            Ok(_) => {}
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(VmValue::List(Arc::new(results))),
    }
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
