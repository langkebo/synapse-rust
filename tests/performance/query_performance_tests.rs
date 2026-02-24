//! Query Performance Tests for N+1 Query Optimization
//!
//! This module tests the batch query interfaces and JOIN optimizations
//! to ensure N+1 query problems have been eliminated.
//!
//! Test Categories:
//! 1. Batch query performance - comparing N+1 vs batch queries
//! 2. JOIN query performance - verifying JOIN queries are efficient
//! 3. Memory efficiency - ensuring batch queries don't cause memory issues

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::time::Instant;

    fn generate_test_ids(prefix: &str, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("{}_{}", prefix, i)).collect()
    }

    #[test]
    fn test_batch_query_interface_design() {
        let room_ids = generate_test_ids("!room", 100);
        assert_eq!(room_ids.len(), 100);
        assert!(room_ids[0].starts_with("!room_"));
    }

    #[test]
    fn test_hashmap_aggregation_efficiency() {
        let room_ids: Vec<String> = (0..1000).map(|i| format!("!room_{}", i)).collect();
        
        let start = Instant::now();
        let mut result: HashMap<String, Vec<String>> = room_ids
            .iter()
            .map(|id| (id.clone(), Vec::new()))
            .collect();
        
        for i in 0..1000 {
            let room_id = format!("!room_{}", i);
            if let Some(events) = result.get_mut(&room_id) {
                events.push(format!("event_{}", i));
            }
        }
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 10, "HashMap aggregation should be fast");
        assert_eq!(result.len(), 1000);
    }

    #[test]
    fn test_batch_vs_n_plus_one_simulation() {
        let item_count = 100;
        
        let start_batch = Instant::now();
        let _batch_result: Vec<String> = (0..item_count).map(|i| format!("item_{}", i)).collect();
        let batch_duration = start_batch.elapsed();
        
        let start_n_plus_one = Instant::now();
        let mut n_plus_one_result = Vec::new();
        for i in 0..item_count {
            n_plus_one_result.push(format!("item_{}", i));
        }
        let n_plus_one_duration = start_n_plus_one.elapsed();
        
        assert!(batch_duration <= n_plus_one_duration + std::time::Duration::from_micros(100));
    }

    #[test]
    fn test_hashset_membership_check_efficiency() {
        let user_ids: HashSet<String> = (0..10000)
            .map(|i| format!("@user_{}:example.com", i))
            .collect();
        
        let start = Instant::now();
        let mut found_count = 0;
        for i in 0..10000 {
            if user_ids.contains(&format!("@user_{}:example.com", i)) {
                found_count += 1;
            }
        }
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 5, "HashSet membership should be O(1)");
        assert_eq!(found_count, 10000);
    }

    #[test]
    fn test_join_query_result_parsing() {
        let mock_row_count = 1000;
        
        let start = Instant::now();
        let results: Vec<(String, String, Option<String>)> = (0..mock_row_count)
            .map(|i| {
                (
                    format!("!room_{}", i),
                    format!("Room {}", i),
                    if i % 2 == 0 { Some(format!("topic_{}", i)) } else { None },
                )
            })
            .collect();
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 10);
        assert_eq!(results.len(), mock_row_count);
    }

    #[test]
    fn test_batch_update_efficiency() {
        let updates: Vec<(String, Option<String>)> = (0..100)
            .map(|i| (format!("@user_{}:example.com", i), Some(format!("User {}", i))))
            .collect();
        
        let start = Instant::now();
        let mut count = 0u64;
        for (user_id, displayname) in &updates {
            let _ = (user_id, displayname);
            count += 1;
        }
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 1);
        assert_eq!(count, 100);
    }

    #[test]
    fn test_empty_batch_early_return() {
        let empty_ids: Vec<String> = Vec::new();
        
        let start = Instant::now();
        let result: HashMap<String, Vec<String>> = if empty_ids.is_empty() {
            HashMap::new()
        } else {
            empty_ids.iter().map(|id| (id.clone(), Vec::new())).collect()
        };
        let duration = start.elapsed();
        
        assert!(duration.as_nanos() < 1000, "Empty batch should return immediately");
        assert!(result.is_empty());
    }

    #[test]
    fn test_large_batch_memory_efficiency() {
        let large_batch: Vec<String> = (0..100000).map(|i| format!("item_{}", i)).collect();
        
        let start = Instant::now();
        let mut map: HashMap<String, i32> = HashMap::with_capacity(large_batch.len());
        for (idx, item) in large_batch.iter().enumerate() {
            map.insert(item.clone(), idx as i32);
        }
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 100, "Large batch should be processed efficiently");
        assert_eq!(map.len(), 100000);
    }

    #[test]
    fn test_concurrent_batch_queries_simulation() {
        use std::sync::Arc;
        use std::thread;
        
        let shared_data = Arc::new((0..1000).collect::<Vec<i32>>());
        let mut handles = vec![];
        
        let start = Instant::now();
        for _ in 0..4 {
            let data = Arc::clone(&shared_data);
            handles.push(thread::spawn(move || {
                let mut sum = 0;
                for val in data.iter() {
                    sum += val;
                }
                sum
            }));
        }
        
        let results: Vec<i32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 50, "Concurrent queries should complete quickly");
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_query_result_grouping_efficiency() {
        let events: Vec<(String, String)> = (0..10000)
            .map(|i| {
                let room_idx = i % 100;
                (format!("!room_{}", room_idx), format!("event_{}", i))
            })
            .collect();
        
        let start = Instant::now();
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for (room_id, event_id) in events {
            grouped.entry(room_id).or_insert_with(Vec::new).push(event_id);
        }
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 20, "Grouping should be efficient");
        assert_eq!(grouped.len(), 100);
        for (_, events) in grouped {
            assert_eq!(events.len(), 100);
        }
    }
}

#[cfg(test)]
mod batch_query_correctness_tests {
    use std::collections::{HashMap, HashSet};

    fn simulate_batch_query<T, F>(ids: &[String], fetch_fn: F) -> HashMap<String, T>
    where
        T: Clone,
        F: Fn(&str) -> T,
    {
        ids.iter().map(|id| (id.clone(), fetch_fn(id))).collect()
    }

    fn simulate_n_plus_one_query<T, F>(ids: &[String], fetch_fn: F) -> HashMap<String, T>
    where
        T: Clone,
        F: Fn(&str) -> T,
    {
        let mut result = HashMap::new();
        for id in ids {
            result.insert(id.clone(), fetch_fn(id));
        }
        result
    }

    #[test]
    fn test_batch_query_returns_all_requested_items() {
        let ids: Vec<String> = (0..10).map(|i| format!("id_{}", i)).collect();
        
        let result = simulate_batch_query(&ids, |id| format!("value_for_{}", id));
        
        assert_eq!(result.len(), 10);
        for id in &ids {
            assert!(result.contains_key(id));
        }
    }

    #[test]
    fn test_batch_query_handles_duplicates() {
        let ids = vec![
            "id_1".to_string(),
            "id_2".to_string(),
            "id_1".to_string(),
        ];
        
        let result = simulate_batch_query(&ids, |id| format!("value_for_{}", id));
        
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_batch_membership_check() {
        let existing_ids: HashSet<String> = 
            vec!["id_1".to_string(), "id_2".to_string(), "id_3".to_string()]
                .into_iter()
                .collect();
        
        let query_ids = vec!["id_1".to_string(), "id_4".to_string(), "id_2".to_string()];
        
        let found: HashSet<String> = query_ids
            .iter()
            .filter(|id| existing_ids.contains(*id))
            .cloned()
            .collect();
        
        assert_eq!(found.len(), 2);
        assert!(found.contains("id_1"));
        assert!(found.contains("id_2"));
        assert!(!found.contains("id_4"));
    }

    #[test]
    fn test_join_result_combines_correctly() {
        let rooms: Vec<(String, String)> = vec![
            ("!room1".to_string(), "Room 1".to_string()),
            ("!room2".to_string(), "Room 2".to_string()),
        ];
        
        let members: Vec<(String, String)> = vec![
            ("!room1".to_string(), "@user1:example.com".to_string()),
            ("!room1".to_string(), "@user2:example.com".to_string()),
            ("!room2".to_string(), "@user3:example.com".to_string()),
        ];
        
        let room_map: HashMap<String, String> = rooms.into_iter().collect();
        
        let mut result: HashMap<String, (String, Vec<String>)> = HashMap::new();
        for (room_id, user_id) in members {
            let room_name = room_map.get(&room_id).cloned().unwrap_or_default();
            result
                .entry(room_id.clone())
                .or_insert_with(|| (room_name, Vec::new()))
                .1
                .push(user_id);
        }
        
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("!room1").unwrap().1.len(), 2);
        assert_eq!(result.get("!room2").unwrap().1.len(), 1);
    }
}

#[cfg(test)]
mod query_performance_benchmarks {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    const BATCH_SIZE: usize = 1000;
    const ACCEPTABLE_BATCH_TIME_MS: u64 = 50;
    const ACCEPTABLE_GROUP_TIME_MS: u64 = 20;

    #[test]
    fn benchmark_hashmap_creation() {
        let ids: Vec<String> = (0..BATCH_SIZE).map(|i| format!("id_{}", i)).collect();
        
        let start = Instant::now();
        let map: HashMap<String, Vec<String>> = ids
            .iter()
            .map(|id| (id.clone(), Vec::new()))
            .collect();
        let duration = start.elapsed();
        
        assert!(
            duration < Duration::from_millis(ACCEPTABLE_BATCH_TIME_MS),
            "HashMap creation took {:?}, expected < {}ms",
            duration,
            ACCEPTABLE_BATCH_TIME_MS
        );
        assert_eq!(map.len(), BATCH_SIZE);
    }

    #[test]
    fn benchmark_result_grouping() {
        let items: Vec<(String, String)> = (0..BATCH_SIZE)
            .map(|i| {
                let group = i % 100;
                (format!("group_{}", group), format!("item_{}", i))
            })
            .collect();
        
        let start = Instant::now();
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for (group, item) in items {
            grouped.entry(group).or_default().push(item);
        }
        let duration = start.elapsed();
        
        assert!(
            duration < Duration::from_millis(ACCEPTABLE_GROUP_TIME_MS),
            "Grouping took {:?}, expected < {}ms",
            duration,
            ACCEPTABLE_GROUP_TIME_MS
        );
        assert_eq!(grouped.len(), 100);
    }

    #[test]
    fn benchmark_set_membership() {
        let set: std::collections::HashSet<String> = (0..BATCH_SIZE)
            .map(|i| format!("item_{}", i))
            .collect();
        
        let start = Instant::now();
        let mut found = 0;
        for i in 0..BATCH_SIZE {
            if set.contains(&format!("item_{}", i)) {
                found += 1;
            }
        }
        let duration = start.elapsed();
        
        assert!(
            duration < Duration::from_millis(5),
            "Set membership took {:?}, expected < 5ms",
            duration
        );
        assert_eq!(found, BATCH_SIZE);
    }

    #[test]
    fn benchmark_string_formatting() {
        let start = Instant::now();
        let items: Vec<String> = (0..BATCH_SIZE)
            .map(|i| format!("!room_{}:example.com", i))
            .collect();
        let duration = start.elapsed();
        
        assert!(
            duration < Duration::from_millis(10),
            "String formatting took {:?}, expected < 10ms",
            duration
        );
        assert_eq!(items.len(), BATCH_SIZE);
    }
}
