use std::ops::{Deref, DerefMut, RangeBounds};
use std::slice::Iter;
use std::vec::Drain;

pub struct SimpleTreeMap<K: Ord, V>(Vec<(K, V)>);

impl<K: Ord, V> SimpleTreeMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, search_key: &K) -> Option<(usize, &(K, V))> {
        match self.0.binary_search_by(|(k, _)| k.cmp(search_key)) {
            Ok(index) => self.0.get(index).map(|kv| (index, kv)),
            Err(_) => None,
        }
    }

    pub fn get_prev(&self, search_key: &K) -> Option<(usize, &(K, V))> {
        let ret = match self.0.binary_search_by(|(k, _)| k.cmp(search_key)) {
            Ok(index) => (index, self.0.get(index)),
            Err(index) => {
                if index != 0 {
                    (index - 1, self.0.get(index - 1))
                } else {
                    (0, None)
                }
            }
        };
        ret.1.map(|kv| (ret.0, kv))
    }

    pub fn get_next(&self, search_key: &K) -> Option<(usize, &(K, V))> {
        let ret = match self.0.binary_search_by(|(k, _)| k.cmp(search_key)) {
            Ok(index) => (index, self.0.get(index)),
            Err(index) => (index, self.0.get(index)),
        };
        ret.1.map(|kv| (ret.0, kv))
    }

    pub fn insert(&mut self, key: K, value: V) -> usize {
        match self.0.binary_search_by(|(k, _)| k.cmp(&key)) {
            Ok(index) => {
                self.0[index].1 = value;
                index
            }
            Err(index) => {
                self.0.insert(index, (key, value));
                index
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> Iter<'_, (K, V)> {
        self.0.iter()
    }

    pub fn remove_at(&mut self, index: usize) -> (K, V) {
        self.0.remove(index)
    }

    pub fn remove(&mut self, key: &K) -> Option<(K, V)> {
        if let Some((index, _)) = self.get(key) {
            Some(self.remove_at(index))
        } else {
            None
        }
    }

    pub fn drain(&mut self, range: impl RangeBounds<usize>) -> Drain<'_, (K, V)> {
        self.0.drain(range)
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl<'a, K: Ord, V> IntoIterator for &'a SimpleTreeMap<K, V> {
    type Item = &'a (K, V);
    type IntoIter = Iter<'a, (K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<K: Ord, V> Default for SimpleTreeMap<K, V> {
    fn default() -> Self {
        SimpleTreeMap(Vec::default())
    }
}

impl<K: Ord, V> Deref for SimpleTreeMap<K, V> {
    type Target = [(K, V)];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<K: Ord, V> DerefMut for SimpleTreeMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}
