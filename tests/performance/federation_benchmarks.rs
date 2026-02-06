//! Federation Performance Benchmarks
//!
//! This module contains federation-specific performance benchmarks
//! to ensure federation features meet quality gate standards.

use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;

#[cfg(test)]
mod federation_benchmarks {
    use super::*;

    fn benchmark_federation_signature_verification(c: &mut Criterion) {
        use std::collections::HashMap;
        use synapse_rust::common::crypto::{generate_ed25519_keypair, sign_data};

        let (public_key, private_key) = generate_ed25519_keypair();
        let test_data = b"test_federation_event";

        c.bench_function("federation_signature_single", |b| {
            b.iter(|| {
                let signature = sign_data(test_data, &private_key, &public_key);
                criterion::black_box(signature);
            });
        });

        c.bench_function("federation_signature_verify", |b| {
            let signature = sign_data(test_data, &private_key, &public_key);
            let mut signatures = HashMap::new();
            signatures.insert("matrix.org".to_string(), HashMap::from([
                ("ed25519:testkey".to_string(), signature)
            ]));

            b.iter(|| {
                criterion::black_box(&signatures);
            });
        });
    }

    fn benchmark_federation_key_prewarming(c: &mut Criterion) {
        use std::sync::Arc;
        use tokio::runtime::Runtime;
        use synapse_rust::web::routes::AppState;
        use synapse_rust::cache::CacheManager;

        let rt = Runtime::new().expect("Failed to create runtime");
        let cache_manager = Arc::new(CacheManager::new(synapse_rust::cache::CacheConfig::default()));

        c.bench_function("federation_key_prewarm_10", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let origins: Vec<&str> = (0..10).map(|i| format!("server{}.test", i).as_str()).collect();
                    criterion::black_box(&origins);
                });
            });
        });
    }

    fn benchmark_state_resolution(c: &mut Criterion) {
        use std::collections::HashMap;
        use serde_json::json;
        use synapse_rust::federation::event_auth::EventAuthChain;

        let chain = EventAuthChain::new();

        c.bench_function("state_resolution_chain_10", |b| {
            let mut events = HashMap::new();
            for i in 0..10 {
                events.insert(
                    format!("$event_{}", i),
                    synapse_rust::federation::event_auth::EventData {
                        event_id: format!("$event_{}", i),
                        room_id: "!room:test".to_string(),
                        event_type: format!("m.room.message{}", i),
                        auth_events: vec![],
                        prev_events: vec![],
                        state_key: Some(json!("user:test")),
                        content: Some(json!({"type": "m.text", "body": "test"})),
                    }
                );
            }
            b.iter(|| {
                let _ = chain.resolve_state_with_auth_chain(&events, &["$event_9"]);
            });
        });

        c.bench_function("state_resolution_chain_100", |b| {
            let mut events = HashMap::new();
            for i in 0..100 {
                events.insert(
                    format!("$event_{}", i),
                    synapse_rust::federation::event_auth::EventData {
                        event_id: format!("$event_{}", i),
                        room_id: "!room:test".to_string(),
                        event_type: format!("m.room.message{}", i % 10),
                        auth_events: vec![],
                        prev_events: vec![],
                        state_key: Some(json!(format!("user:{}", i % 5))),
                        content: Some(json!({"type": "m.text", "body": format!("test{}", i)})),
                    }
                );
            }
            b.iter(|| {
                let _ = chain.resolve_state_with_auth_chain(&events, &["$event_99"]);
            });
        });
    }

    fn benchmark_event_auth_chain(c: &mut Criterion) {
        use std::collections::HashMap;
        use synapse_rust::federation::event_auth::EventAuthChain;

        c.bench_function("auth_chain_build_10", |b| {
            let mut events = HashMap::new();
            events.insert(
                "$create".to_string(),
                synapse_rust::federation::event_auth::EventData {
                    event_id: "$create".to_string(),
                    room_id: "!room:test".to_string(),
                    event_type: "m.room.create".to_string(),
                    auth_events: vec![],
                    prev_events: vec![],
                    state_key: None,
                    content: None,
                },
            );
            events.insert(
                "$member".to_string(),
                synapse_rust::federation::event_auth::EventData {
                    event_id: "$member".to_string(),
                    room_id: "!room:test".to_string(),
                    event_type: "m.room.member".to_string(),
                    auth_events: vec!["$create".to_string()],
                    prev_events: vec!["$create".to_string()],
                    state_key: Some(json!("@user:test")),
                    content: None,
                },
            );

            b.iter(|| {
                let chain = EventAuthChain::new();
                let _ = chain.build_auth_chain_from_events(&events, "$member");
            });
        });
    }

    fn benchmark_cache_compression(c: &mut Criterion) {
        use synapse_rust::cache::compression::{compress, decompress};

        let small_data = b"Hello, World!";
        let medium_data = (0..1000).map(|i| (i % 256) as u8).collect::<Vec<_>>();
        let large_data = (0..10000).map(|i| (i % 256) as u8).collect::<Vec<_>>();

        c.bench_function("cache_compress_small", |b| {
            b.iter(|| {
                let _ = compress(small_data);
            });
        });

        c.bench_function("cache_compress_medium", |b| {
            b.iter(|| {
                let _ = compress(&medium_data);
            });
        });

        c.bench_function("cache_compress_large", |b| {
            b.iter(|| {
                let _ = compress(&large_data);
            });
        });

        c.bench_function("cache_decompress", |b| {
            let compressed = compress(&large_data);
            b.iter(|| {
                let _ = decompress(&compressed);
            });
        });
    }

    criterion_group!(
        name = federation_benches;
        config = Criterion::default()
            .sample_size(20)
            .measurement_time(Duration::from_secs(30))
            .warm_up_time(Duration::from_secs(5));
        targets = benchmark_federation_signature_verification,
                 benchmark_federation_key_prewarming,
                 benchmark_state_resolution,
                 benchmark_event_auth_chain,
                 benchmark_cache_compression
    );

    criterion_main!(federation_benches);
}
