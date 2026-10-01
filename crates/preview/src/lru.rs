//! A cost-bounded least-recently-used map.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

pub struct Lru<K, V> {
    map: HashMap<K, (V, usize, u64)>,
    /// Use tick → key, oldest first.
    order: BTreeMap<u64, K>,
    tick: u64,
    cost: usize,
    budget: usize,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    /// An LRU holding at most `budget` cost units (e.g. bytes). The most recent entry is always
    /// kept, even if it alone exceeds the budget.
    pub fn new(budget: usize) -> Self {
        Lru { map: HashMap::new(), order: BTreeMap::new(), tick: 0, cost: 0, budget }
    }

    fn touch(&mut self, k: &K) {
        self.tick += 1;
        let t = self.tick;
        if let Some(e) = self.map.get_mut(k) {
            self.order.remove(&e.2);
            e.2 = t;
            self.order.insert(t, k.clone());
        }
    }

    pub fn get(&mut self, k: &K) -> Option<&V> {
        if !self.map.contains_key(k) {
            return None;
        }
        self.touch(k);
        self.map.get(k).map(|e| &e.0)
    }

    pub fn peek(&self, k: &K) -> Option<&V> {
        self.map.get(k).map(|e| &e.0)
    }

    pub fn contains(&self, k: &K) -> bool {
        self.map.contains_key(k)
    }

    pub fn insert(&mut self, k: K, v: V, cost: usize) {
        self.remove(&k);
        self.tick += 1;
        self.order.insert(self.tick, k.clone());
        self.map.insert(k, (v, cost, self.tick));
        self.cost += cost;
        while self.cost > self.budget && self.map.len() > 1 {
            let Some((_, old)) = self.order.pop_first() else { break };
            if let Some((_, c, _)) = self.map.remove(&old) {
                self.cost -= c;
            }
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        let (v, c, t) = self.map.remove(k)?;
        self.order.remove(&t);
        self.cost -= c;
        Some(v)
    }

    pub fn retain(&mut self, mut f: impl FnMut(&K) -> bool) {
        let drop: Vec<K> = self.map.keys().filter(|k| !f(k)).cloned().collect();
        for k in drop {
            self.remove(&k);
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.cost = 0;
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn cost(&self) -> usize {
        self.cost
    }
    pub fn budget(&self) -> usize {
        self.budget
    }
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
    }
}
