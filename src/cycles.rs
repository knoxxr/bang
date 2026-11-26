//! 순환 참조 수집 (Phase 60)
//!
//! bang의 값은 참조 카운팅(Arc)으로 관리된다. 리스트·맵은 값이라 서로를 가리킬 수 없지만,
//! **클로저는 변수(상태 벡터)를 붙잡으므로** "상태 벡터 → (맵/리스트 →) 클로저 → 같은 상태 벡터"
//! 순환이 생길 수 있다. 참조 카운팅만으로는 이런 순환이 해제되지 않는다 (SPEC 15장).
//!
//! 이 모듈은 후보 상태 벡터에서 닿는 부분 그래프만 보고 "외부에서 들어오는 참조가 있는가"를 센다
//! (trial deletion, Bacon–Rajan 방식의 국소 판정):
//!
//! 1. 후보에서 출발해 상태 벡터·클로저·upvalue·(클로저를 품은) 리스트/맵을 노드로 모은다.
//!    클로저가 없는 리스트/맵은 순환의 일부가 될 수 없으므로 건너뛴다. Future·채널 등은 불투명(간선 없음).
//! 2. 노드마다 "그래프 안에서 들어오는 참조 수"(internal)를 센다. Arc 강한 참조 수에서 우리가 쥔 1개와
//!    internal을 빼면 외부 참조 수다.
//! 3. 외부 참조가 있는 노드에서 닿는 모든 노드는 살아 있다. 나머지는 순환 안에서만 서로를 가리키는
//!    쓰레기이므로, 쓰레기 상태 벡터를 비워 순환을 끊는다(그러면 Arc가 연쇄적으로 해제된다).
//!
//! 안전성: 노드는 이 작업(스레드)의 것이다 — 작업 경계에서 Isolator가 클로저·상태를 복사하므로 다른
//! 작업이 같은 상태 벡터를 쥐지 않는다. 다른 스레드가 쥔 참조가 있다면 그것은 외부 참조로 세어져
//! "살아 있음"이 되므로(안전한 방향) 잘못 해제하지 않는다. 쓰레기 노드에는 바깥에서 닿는 길이 없으므로
//! 판정 뒤에 새 참조가 생길 수도 없다.

use std::collections::HashMap;
use std::sync::Arc;

use crate::omap::OrderedMap;
use crate::vm::{StateCell, Upvalue, VmClosure, VmValue};

enum Node {
    Cell(StateCell),
    Closure(Arc<VmClosure>),
    Upval(Arc<Upvalue>),
    List(Arc<Vec<VmValue>>),
    Map(Arc<OrderedMap<VmValue>>),
}

impl Node {
    fn key(&self) -> usize {
        match self {
            Node::Cell(a) => Arc::as_ptr(a) as *const u8 as usize,
            Node::Closure(a) => Arc::as_ptr(a) as *const u8 as usize,
            Node::Upval(a) => Arc::as_ptr(a) as *const u8 as usize,
            Node::List(a) => Arc::as_ptr(a) as *const u8 as usize,
            Node::Map(a) => Arc::as_ptr(a) as *const u8 as usize,
        }
    }

    fn strong(&self) -> usize {
        match self {
            Node::Cell(a) => Arc::strong_count(a),
            Node::Closure(a) => Arc::strong_count(a),
            Node::Upval(a) => Arc::strong_count(a),
            Node::List(a) => Arc::strong_count(a),
            Node::Map(a) => Arc::strong_count(a),
        }
    }
}

struct Info {
    node: Node,
    internal: usize,
    edges: Vec<usize>,
    live: bool,
}

/// 값 안에 클로저가 있는가 (없으면 순환의 일부가 될 수 없다).
fn holds_closure(v: &VmValue) -> bool {
    crate::vm::contains_closure(v)
}

fn value_children(v: &VmValue, out: &mut Vec<Node>) {
    match v {
        VmValue::Closure(c) => out.push(Node::Closure(c.clone())),
        VmValue::List(l) if holds_closure(v) => out.push(Node::List(l.clone())),
        VmValue::Map(m) if holds_closure(v) => out.push(Node::Map(m.clone())),
        _ => {}
    }
}

fn children(node: &Node, out: &mut Vec<Node>) {
    match node {
        Node::Cell(c) => {
            if let Ok(g) = c.lock() {
                for v in g.iter() {
                    value_children(v, out);
                }
            }
        }
        Node::Closure(c) => {
            for uv in &c.upvalues {
                out.push(Node::Upval(uv.clone()));
            }
            out.push(Node::Cell(c.globals.clone()));
        }
        Node::Upval(u) => out.push(Node::Cell(u.locals.clone())),
        Node::List(l) => {
            for v in l.iter() {
                value_children(v, out);
            }
        }
        Node::Map(m) => {
            for v in m.values() {
                value_children(v, out);
            }
        }
    }
}

/// 수집 결과.
pub(crate) struct Collected {
    /// 살아 있는 것으로 판정된 후보 (다음 세대에서 다시 볼 것)
    pub survivors: Vec<StateCell>,
}

static FREED_TOTAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 지금까지 순환을 끊어 비운 상태 벡터 총수 (테스트·진단용).
pub fn freed_cells_total() -> usize {
    FREED_TOTAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// 후보 상태 벡터들에서 닿는 부분에서, 바깥에서 닿지 않는 순환을 끊는다.
/// 호출자는 후보를 넘긴 뒤 자기 쪽 복사본을 쥐고 있지 않아야 한다(쥐고 있으면 살아 있다고 판정될 뿐 안전하다).
pub(crate) fn collect(candidates: Vec<StateCell>) -> Collected {
    let mut infos: HashMap<usize, Info> = HashMap::new();
    let mut queue: Vec<usize> = Vec::new();
    let mut candidate_keys: Vec<usize> = Vec::new();
    for c in candidates {
        let n = Node::Cell(c);
        let k = n.key();
        if let std::collections::hash_map::Entry::Vacant(e) = infos.entry(k) {
            e.insert(Info { node: n, internal: 0, edges: Vec::new(), live: false });
            queue.push(k);
            candidate_keys.push(k);
        }
    }
    if infos.is_empty() {
        return Collected { survivors: Vec::new() };
    }

    // 1. 부분 그래프 수집 + 내부 참조 수
    let mut kids: Vec<Node> = Vec::new();
    while let Some(k) = queue.pop() {
        kids.clear();
        if let Some(info) = infos.get(&k) {
            children(&info.node, &mut kids);
        }
        let mut edges = Vec::with_capacity(kids.len());
        for kid in kids.drain(..) {
            let kk = kid.key();
            edges.push(kk);
            match infos.get_mut(&kk) {
                Some(i) => i.internal += 1, // kid(임시 복사본)는 여기서 버려진다
                None => {
                    infos.insert(kk, Info { node: kid, internal: 1, edges: Vec::new(), live: false });
                    queue.push(kk);
                }
            }
        }
        if let Some(info) = infos.get_mut(&k) {
            info.edges = edges;
        }
    }

    // 2. 외부 참조가 있는 노드 = 살아 있음의 뿌리 (우리가 쥔 복사본 1개를 뺀다)
    let mut work: Vec<usize> = infos
        .iter()
        .filter(|(_, i)| i.node.strong().saturating_sub(1) != i.internal || i.node.strong() == 0)
        .map(|(k, _)| *k)
        .collect();
    for k in &work {
        if let Some(i) = infos.get_mut(k) {
            i.live = true;
        }
    }
    // 3. 살아 있는 노드에서 닿는 모든 노드도 살아 있다
    while let Some(k) = work.pop() {
        let edges = infos.get(&k).map(|i| i.edges.clone()).unwrap_or_default();
        for e in edges {
            if let Some(i) = infos.get_mut(&e) {
                if !i.live {
                    i.live = true;
                    work.push(e);
                }
            }
        }
    }

    // 4. 쓰레기 상태 벡터를 비워 순환을 끊는다 (꺼낸 값은 잠금을 모두 푼 뒤에 버린다)
    let mut trash: Vec<Vec<VmValue>> = Vec::new();
    for i in infos.values() {
        if let (false, Node::Cell(c)) = (i.live, &i.node) {
            if let Ok(mut g) = c.lock() {
                trash.push(std::mem::take(&mut *g));
            }
        }
    }
    FREED_TOTAL.fetch_add(trash.len(), std::sync::atomic::Ordering::Relaxed);
    let mut survivors = Vec::new();
    for k in candidate_keys {
        if let Some(Info { node: Node::Cell(c), live: true, .. }) = infos.remove(&k) {
            survivors.push(c);
        }
    }
    drop(infos);
    drop(trash);
    Collected { survivors }
}
