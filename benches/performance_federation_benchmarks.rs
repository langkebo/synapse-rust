//! Federation Performance Benchmarks
//!
//! This module contains federation-specific performance benchmarks
//! that test real federation logic (state resolution, event auth
//! chain building). All benchmarks exercise actual code paths in
//! `synapse_rust::federation`.
//!
//! Pseudo benchmarks (signature black-box, key prewarm, cache
//! compress) have been removed per Step 8 audit — they measured
//! memory operations rather than federation behaviour.

use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::time::Duration;

fn benchmark_state_resolution(c: &mut Criterion) {
    use std::collections::HashMap;
    use synapse_rust::federation::event_auth::{EventAuthChain, EventData};

    let chain = EventAuthChain::new();

    c.bench_function("state_resolution_chain_10", |b| {
        let mut events = HashMap::new();
        for i in 0..10 {
            events.insert(
                format!("$event_{i}"),
                EventData {
                    event_id: format!("$event_{i}"),
                    room_id: "!room:test".to_string(),
                    event_type: format!("m.room.message{i}"),
                    auth_events: vec![],
                    prev_events: vec![],
                    state_key: Some(json!("user:test")),
                    content: Some(json!({"type": "m.text", "body": "test"})),
                },
            );
        }
        let auth_chain = ["$event_9"];
        b.iter(|| {
            let _ = chain.resolve_state_with_auth_chain(&events, &auth_chain);
        });
    });

    c.bench_function("state_resolution_chain_100", |b| {
        let mut events = HashMap::new();
        for i in 0..100 {
            events.insert(
                format!("$event_{i}"),
                EventData {
                    event_id: format!("$event_{i}"),
                    room_id: "!room:test".to_string(),
                    event_type: format!("m.room.message{}", i % 10),
                    auth_events: vec![],
                    prev_events: vec![],
                    state_key: Some(json!(format!("user:{}", i % 5))),
                    content: Some(json!({"type": "m.text", "body": format!("test{}", i)})),
                },
            );
        }
        let auth_chain = ["$event_99"];
        b.iter(|| {
            let _ = chain.resolve_state_with_auth_chain(&events, &auth_chain);
        });
    });
}

fn benchmark_event_auth_chain(c: &mut Criterion) {
    use std::collections::HashMap;
    use synapse_rust::federation::event_auth::{EventAuthChain, EventData};

    c.bench_function("auth_chain_build_10", |b| {
        let mut events = HashMap::new();
        events.insert(
            "$create".to_string(),
            EventData {
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
            EventData {
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

criterion_group!(
    name = federation_benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = benchmark_state_resolution,
             benchmark_event_auth_chain
);

criterion_main!(federation_benches);
