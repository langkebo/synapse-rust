//! Pre-sized HashMap/HashSet/Vec builders (HashMapBuilder/HashSetBuilder/VecBuilder).

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

/// Represents VecBuilder.
pub struct VecBuilder<T> {
    capacity: usize,
    _phantom: PhantomData<T>,
}

impl<T> VecBuilder<T> {
    /// Constructs a new instance.
    pub fn new(capacity: usize) -> Self {
        Self { capacity, _phantom: PhantomData }
    }

    /// Builds the configured value.
    pub fn build(&self) -> Vec<T> {
        Vec::with_capacity(self.capacity)
    }

    /// Constructs from iter.
    pub fn from_iter<I>(self, iter: I) -> Vec<T>
    where
        I: IntoIterator<Item = T>,
    {
        iter.into_iter().collect()
    }
}

/// Represents HashMapBuilder.
pub struct HashMapBuilder<K, V> {
    capacity: usize,
    _phantom_key: PhantomData<K>,
    _phantom_value: PhantomData<V>,
}

impl<K, V> HashMapBuilder<K, V> {
    /// Constructs a new instance.
    pub fn new(capacity: usize) -> Self {
        Self { capacity, _phantom_key: PhantomData, _phantom_value: PhantomData }
    }

    /// Builds the configured value.
    pub fn build(&self) -> HashMap<K, V> {
        HashMap::with_capacity(self.capacity)
    }

    /// Constructs from iter.
    pub fn from_iter<I>(self, iter: I) -> HashMap<K, V>
    where
        K: Eq + std::hash::Hash,
        I: IntoIterator<Item = (K, V)>,
    {
        iter.into_iter().collect()
    }
}

/// Represents HashSetBuilder.
pub struct HashSetBuilder<T> {
    capacity: usize,
    _phantom: PhantomData<T>,
}

impl<T> HashSetBuilder<T> {
    /// Constructs a new instance.
    pub fn new(capacity: usize) -> Self {
        Self { capacity, _phantom: PhantomData }
    }

    /// Builds the configured value.
    pub fn build(&self) -> HashSet<T>
    where
        T: Eq + std::hash::Hash,
    {
        HashSet::with_capacity(self.capacity)
    }

    /// Constructs from iter.
    pub fn from_iter<I>(self, iter: I) -> HashSet<T>
    where
        T: Eq + std::hash::Hash,
        I: IntoIterator<Item = T>,
    {
        iter.into_iter().collect()
    }
}

/// Vecs the with.
pub fn vec_with_capacity<T>(capacity: usize) -> Vec<T> {
    Vec::with_capacity(capacity)
}

/// Hashmaps the with.
pub fn hashmap_with_capacity<K, V>(capacity: usize) -> HashMap<K, V> {
    HashMap::with_capacity(capacity)
}

/// Hashsets the with.
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
        assert!(map.capacity() >= 10);
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_hashmap_builder_from_iter() {
        let builder = HashMapBuilder::new(10);
        let map: HashMap<String, i32> =
            builder.from_iter(vec![("a".to_string(), 1), ("b".to_string(), 2), ("c".to_string(), 3)]);
        assert_eq!(map.len(), 3);
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("b"), Some(&2));
        assert_eq!(map.get("c"), Some(&3));
    }

    #[test]
    fn test_hashset_builder() {
        let builder = HashSetBuilder::new(10);
        let set: HashSet<i32> = builder.build();
        assert!(set.capacity() >= 10);
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
        assert!(map.capacity() >= 20);
    }

    #[test]
    fn test_hashset_with_capacity() {
        let set: HashSet<i32> = hashset_with_capacity(20);
        assert!(set.capacity() >= 20);
    }
}
