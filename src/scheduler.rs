// Bang — Phase 9 Part A: M:N 스케줄러
//
// OS 스레드 1개당 태스크 1개(Phase 5B)를 교체.
// 고정 크기 스레드 풀(M 워커)이 N 태스크를 처리한다.
// 의미론은 동일: VmFuture.complete() / .resolve() 패턴 그대로 유지.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;

type Task = Box<dyn FnOnce() + Send + 'static>;

/// 임시(overflow) 워커가 유휴 상태로 이 시간을 넘기면 종료한다.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

struct Inner {
    queue:  VecDeque<Task>,
    closed: bool,
    idle:   usize, // 현재 태스크를 기다리는(유휴) 워커 수
    total:  usize, // 살아있는 워커 수
    base:   usize, // 항상 유지하는 기본 워커 수
    cap:    usize, // 최대 워커 수 (블로킹 작업 폭증 대비 상한)
}

pub struct Scheduler {
    inner: Arc<(Mutex<Inner>, Condvar)>,
}

/// 워커 루프. base 초과로 생성된 임시 워커는 유휴 타임아웃 시 스스로 종료한다.
fn worker_loop(inner: Arc<(Mutex<Inner>, Condvar)>) {
    let (lock, cvar) = &*inner;
    loop {
        let task: Option<Task> = {
            let mut guard = lock.lock().unwrap();
            loop {
                if let Some(t) = guard.queue.pop_front() {
                    break Some(t);
                }
                if guard.closed {
                    guard.total -= 1;
                    break None;
                }
                guard.idle += 1;
                let (g, res) = cvar.wait_timeout(guard, IDLE_TIMEOUT).unwrap();
                guard = g;
                guard.idle -= 1;
                // 임시 워커(base 초과)는 유휴 타임아웃 시 종료
                if res.timed_out() && guard.queue.is_empty()
                    && !guard.closed && guard.total > guard.base {
                    guard.total -= 1;
                    break None;
                }
            }
        };
        match task {
            Some(f) => f(),   // 락 해제 상태에서 실행
            None    => break, // 종료
        }
    }
}

impl Scheduler {
    pub fn new(num_threads: usize) -> Self {
        let base = num_threads.max(1);
        let inner: Arc<(Mutex<Inner>, Condvar)> = Arc::new((
            Mutex::new(Inner {
                queue: VecDeque::new(),
                closed: false,
                idle: 0,
                total: base,
                base,
                cap: base.max(512), // 블로킹 I/O 동시성 상한
            }),
            Condvar::new(),
        ));

        for _ in 0..base {
            let inner = inner.clone();
            thread::spawn(move || worker_loop(inner));
        }

        Self { inner }
    }

    /// 태스크를 풀에 제출한다.
    /// 풀 채널이 닫힌 경우(정상 운용에서는 발생하지 않음)
    /// 새 OS 스레드로 폴백한다.
    pub fn spawn_task<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let (lock, cvar) = &*self.inner;
        let mut guard = lock.lock().unwrap();
        if guard.closed {
            // 폴백: 새 OS 스레드 생성 (정상 경로에서는 불필요)
            drop(guard);
            thread::spawn(f);
            return;
        }
        guard.queue.push_back(Box::new(f));
        // 유휴 워커가 없고 상한 미만이면 임시 워커를 띄운다
        // (블로킹 I/O로 모든 워커가 막혀도 새 연결을 처리할 수 있게 함).
        if guard.idle == 0 && guard.total < guard.cap {
            guard.total += 1;
            drop(guard);
            let inner = self.inner.clone();
            thread::spawn(move || worker_loop(inner));
        } else {
            drop(guard);
            cvar.notify_one();
        }
    }

    /// 현재 큐에 쌓인 태스크 수 (테스트·진단용)
    pub fn queued(&self) -> usize {
        self.inner.0.lock().unwrap().queue.len()
    }
}

// Drop 이 호출되면(정적 변수에서는 프로세스 종료 시) 풀을 정상 종료한다.
impl Drop for Scheduler {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.inner;
        lock.lock().unwrap().closed = true;
        cvar.notify_all();
    }
}

// ============================================================================
// 전역 스케줄러 (lazy 초기화)
// ============================================================================

static GLOBAL: OnceLock<Scheduler> = OnceLock::new();

/// 전역 M:N 스케줄러를 반환한다.
/// 처음 호출 시 `available_parallelism()` 크기의 스레드 풀을 생성한다.
pub fn global() -> &'static Scheduler {
    GLOBAL.get_or_init(|| {
        let n = thread::available_parallelism()
            .map(|p| p.get().max(2))
            .unwrap_or(4);
        Scheduler::new(n)
    })
}

// ============================================================================
// 단위 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_scheduler_runs_tasks() {
        let sched = Scheduler::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::channel();

        for _ in 0..10 {
            let c = counter.clone();
            let tx = tx.clone();
            sched.spawn_task(move || {
                c.fetch_add(1, Ordering::SeqCst);
                tx.send(()).ok();
            });
        }
        for _ in 0..10 { rx.recv().unwrap(); }
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_global_scheduler() {
        let (tx, rx) = std::sync::mpsc::channel();
        global().spawn_task(move || { tx.send(42u32).ok(); });
        assert_eq!(rx.recv().unwrap(), 42);
    }
}
