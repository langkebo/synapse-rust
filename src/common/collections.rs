use std::collections::{HashMap, HashSet};

pub struct VecBuilder<T> {
    capacity: usize,
}

impl<T> VecBuilder<T> {
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    pub fn build(&self) -> Vec<T> {
        Vec::with_capacity(self.capacity)
    }

    pub fn from_iter<I>(self, iter: I) -> Vec<T>
    where
        I: IntoIterator<Item = T>,
    {
        iter.into_iter().collect()
    }
}

pub struct HashMapBuilder<K, V> {
    capacity: usize,
}

impl<K, V> HashMapBuilder<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    pub fn build(&self) -> HashMap<K, V> {
        HashMap::with_capacity(self.capacity)
    }

    pub fn from_iter<I>(self, iter: I) -> HashMap<K, V>
    where
        K: Eq + std::hash::Hash,
        I: IntoIterator<Item = (K, V)>,
    {
        iter.into_iter().collect()
    }
}

pub struct HashSetBuilder<T> {
    capacity: usize,
}

impl<T> HashSetBuilder<T> {
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    pub fn build(&self) -> HashSet<T>
    where
        T: Eq + std::hash::Hash,
    {
        HashSet::with_capacity(self.capacity)
    }

    pub fn from_iter<I>(self, iter: I) -> HashSet<T>
    where
        T: Eq + std::hash::Hash,
        I: IntoIterator<Item = T>,
    {
        iter.into_iter().collect()
    }
}

pub fn vec_with_capacity<T>(capacity: usize) -> Vec<T> {
    Vec::with_capacity(capacity)
}

pub fn hashmap_with_capacity<K, V>(capacity: usize) -> HashMap<K, V> {
    HashMap::with_capacity(capacity)
}

pub fn hashset_with_capacity<T>(capacity: usize) -> HashSet<T>
where
    T: Eq + std::hash::Hash,
{
    HashSet::with_capacity(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_builder() {
        let builder = VecBuilder::new(10);
        let vec: Vec<i32> = builder.build();
        assert_eq!(vec.capacity(), 10);
        assert_eq!(vec.len(), 0);
    }

    #[test]
    fn test_vec_builder_from_iter() {
        let builder = VecBuilder::new(10);
        let vec: Vec<i32> = builder.from_iter(vec![1, 2, 3, 4, 5]);
        assert_eq!(vec.len(), 5);
        assert_eq!(vec, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_hashmap_builder() {
        let builder = HashMapBuilder::new(10);
        let map: HashMap<String, i32> = builder.build();
        assert_eq!(map.capacity(), 10);
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_hashmap_builder_from_iter() {
        let builder = HashMapBuilder::new(10);
        let map: HashMap<String, i32> = builder.from_iter(vec![
            ("a".to_string(), 1),
            ("b".to_string(), 2),
            ("c".to_string(), 3),
        ]);
        assert_eq!(map.len(), 3);
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("b"), Some(&2));
        assert_eq!(map.get("c"), Some(&3));
    }

    #[test]
    fn test_hashset_builder() {
        let builder = HashSetBuilder::new(10);
        let set: HashSet<i32> = builder.build();
        assert_eq!(set.capacity(), 10);
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_hashset_builder_from_iter() {
        let builder = HashSetBuilder::new(10);
        let set: HashSet<i32> = builder.from_iter(vec![1, 2, 3, 4, 5]);
        assert_eq!(set.len(), 5);
        assert!(set.contains(&1));
        assert!(set.contains(&5));
    }

    #[test]
    fn test_vec_with_capacity() {
        let vec: Vec<i32> = vec_with_capacity(20);
        assert_eq!(vec.capacity(), 20);
    }

    #[test]
    fn test_hashmap_with_capacity() {
        let map: HashMap<String, i32> = hashmap_with_capacity(20);
        assert_eq!(map.capacity(), 20);
    }

    #[test]
    fn test_hashset_with_capacity() {
        let set: HashSet<i32> = hashset_with_capacity(20);
        assert_eq!(set.capacity(), 20);
    }
}
