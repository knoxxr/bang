// Bang — 논블로킹 I/O 리액터 (단계 0: 골격)
//
// epoll/kqueue/IOCP를 크로스플랫폼으로 추상화한 `polling` 위에, fd readiness를
// 기다리는 최소 리액터를 둔다. 이 단계에서는 **추가만** 하며 VM/스케줄러는 건드리지
// 않는다. 이후 단계에서 tcp_accept/read의 WouldBlock → yield/resume 에 연결한다.
//
// 사용 흐름:
//   let r = Reactor::new()?
//   r.add_readable(&listener, KEY)?            // 관심 등록
//   let ready = r.wait(Some(timeout))?         // readiness 대기 → 준비된 key 목록
//   // ... accept/read 수행 ...
//   r.modify_readable(&listener, KEY)?         // oneshot 재무장
//
// polling은 oneshot 의미라, 이벤트가 한 번 발생하면 다시 받으려면 modify로 재무장한다.

use std::io;
use std::time::Duration;

use polling::{AsRawSource, AsSource, Event, Events, Poller};

pub struct Reactor {
    poller: Poller,
    events: Events,
}

impl Reactor {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            poller: Poller::new()?,
            events: Events::new(),
        })
    }

    /// 소스를 readable 관심으로 등록한다. key로 이벤트를 식별한다.
    ///
    /// # Safety
    /// 소스(fd/socket)는 deregister 하거나 리액터가 살아있는 동안 유효해야 한다.
    pub fn add_readable<S: AsRawSource>(&self, source: S, key: usize) -> io::Result<()> {
        // SAFETY: 호출자가 소스를 등록 해제 전까지 살려둔다(상위 계층 계약).
        unsafe { self.poller.add(source, Event::readable(key)) }
    }

    /// oneshot 이벤트 재무장 (다음 readiness를 다시 받기 위해).
    pub fn modify_readable<S: AsSource>(&self, source: S, key: usize) -> io::Result<()> {
        self.poller.modify(source, Event::readable(key))
    }

    /// 관심 등록 해제.
    pub fn deregister<S: AsSource>(&self, source: S) -> io::Result<()> {
        self.poller.delete(source)
    }

    /// readiness를 기다려 준비된 key 목록을 반환한다. timeout=None이면 무한 대기.
    /// 깨어났지만 이벤트가 없으면(타임아웃) 빈 벡터.
    pub fn wait(&mut self, timeout: Option<Duration>) -> io::Result<Vec<usize>> {
        self.events.clear();
        self.poller.wait(&mut self.events, timeout)?;
        Ok(self.events.iter().map(|e| e.key).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn reactor_creates() {
        assert!(Reactor::new().is_ok());
    }

    // 리스너를 등록하고, 다른 스레드에서 접속하면 readable readiness가 와야 한다.
    #[test]
    fn listener_becomes_readable_on_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();

        let mut reactor = Reactor::new().unwrap();
        const LKEY: usize = 1;
        reactor.add_readable(&listener, LKEY).unwrap();

        // 별도 스레드에서 접속
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).expect("connect");
            let _ = s.write_all(b"hi");
        });

        // readiness 대기 (넉넉한 타임아웃)
        let ready = reactor.wait(Some(Duration::from_secs(2))).unwrap();
        assert!(ready.contains(&LKEY), "리스너 readable readiness 기대: {ready:?}");

        // 논블로킹 accept 성공해야 함
        let accepted = listener.accept();
        assert!(accepted.is_ok(), "accept 성공 기대");

        client.join().unwrap();
        let _ = reactor.deregister(&listener);
    }

    // 등록한 소스가 없으면 짧은 타임아웃에 빈 결과로 깨어난다.
    #[test]
    fn wait_times_out_empty() {
        let mut reactor = Reactor::new().unwrap();
        let ready = reactor.wait(Some(Duration::from_millis(50))).unwrap();
        assert!(ready.is_empty());
    }
}
