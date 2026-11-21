//! 삽입 순서를 지키는 맵 (Phase 51) — bang 맵 값(`VmValue::Map`)과 채널 전송용 `Value::Map`의 저장소.
//!
//! Python dict처럼 키를 처음 넣은 순서대로 순회·표시한다. 이미 있는 키에 다시 넣으면 값만 바뀌고
//! 위치는 그대로다. 구현은 엔트리 벡터 + 키→위치 색인 (외부 의존성 없음 — 헌법 5조).

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
    index: HashMap<String, usize>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self { entries: Vec::new(), index: HashMap::new() }
    }
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Self { entries: Vec::with_capacity(n), index: HashMap::with_capacity(n) }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        match self.index.get(key) {
            Some(&i) => Some(&mut self.entries[i].1),
            None => None,
        }
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// 넣기. 이미 있는 키면 값을 바꾸고(위치 유지) 이전 값을 돌려준다.
    pub fn insert(&mut self, key: String, value: V) -> Option<V> {
        if let Some(&i) = self.index.get(&key) {
            return Some(std::mem::replace(&mut self.entries[i].1, value));
        }
        self.index.insert(key.clone(), self.entries.len());
        self.entries.push((key, value));
        None
    }

    /// 지우기 (뒤 엔트리를 한 칸씩 당긴다 — O(n), 순서 유지).
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let i = self.index.remove(key)?;
        let (_, v) = self.entries.remove(i);
        for pos in self.index.values_mut() {
            if *pos > i {
                *pos -= 1;
            }
        }
        Some(v)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut V)> {
        self.entries.iter_mut().map(|(k, v)| (&*k, v))
    }
}

impl<V> FromIterator<(String, V)> for OrderedMap<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(iter: I) -> Self {
        let mut m = OrderedMap::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<V> IntoIterator for OrderedMap<V> {
    type Item = (String, V);
    type IntoIter = std::vec::IntoIter<(String, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a OrderedMap<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = std::iter::Map<std::slice::Iter<'a, (String, V)>, fn(&'a (String, V)) -> (&'a String, &'a V)>;
    fn into_iter(self) -> Self::IntoIter {
        fn split<K, V>(e: &(K, V)) -> (&K, &V) {
            (&e.0, &e.1)
        }
        self.entries.iter().map(split)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_insertion_order_and_updates_in_place() {
        let mut m = OrderedMap::new();
        m.insert("b".to_string(), 1);
        m.insert("a".to_string(), 2);
        m.insert("c".to_string(), 3);
        assert_eq!(m.insert("a".to_string(), 20), Some(2));
        assert_eq!(m.keys().cloned().collect::<Vec<_>>(), ["b", "a", "c"]);
        assert_eq!(m.get("a"), Some(&20));
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn remove_keeps_order_and_index() {
        let mut m: OrderedMap<i32> = [("x", 1), ("y", 2), ("z", 3)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(m.remove("x"), Some(1));
        assert_eq!(m.get("z"), Some(&3));
        assert_eq!(m.keys().cloned().collect::<Vec<_>>(), ["y", "z"]);
        m.insert("x".to_string(), 9);
        assert_eq!(m.keys().cloned().collect::<Vec<_>>(), ["y", "z", "x"]);
        assert_eq!(m.remove("nope"), None);
    }
}
