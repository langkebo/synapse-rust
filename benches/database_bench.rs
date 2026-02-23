//! Database query benchmarks.
//!
//! Measures the performance of critical database operations.
//! Run with: cargo bench --bench database_bench

#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion, Throughput};

// Note: These benchmarks require a test database
// Set DATABASE_URL environment variable before running

/// Simple microbenchmark for string operations used in validation.
fn bench_validation_username(c: &mut Criterion) {
    use regex::Regex;

    let username_regex = Regex::new(r"^[a-z0-9._=\-]{1,255}$").unwrap();

    let mut group = c.benchmark_group("validation/username");
    group.throughput(Throughput::Elements(1));

    group.bench_function("valid_short", |b| {
        b.iter(|| {
            let username = black_box("alice");
            username_regex.is_match(username)
        })
    });

    group.bench_function("valid_long", |b| {
        b.iter(|| {
            let username = black_box("user.name-with_underscores-123");
            username_regex.is_match(username)
        })
    });

    group.bench_function("invalid", |b| {
        b.iter(|| {
            let username = black_box("Invalid User!");
            username_regex.is_match(username)
        })
    });

    group.finish();
}

/// Benchmark Matrix ID validation.
fn bench_validation_matrix_id(c: &mut Criterion) {
    use regex::Regex;

    let matrix_id_regex = Regex::new(r"^@[a-z0-9._=\-]+:[a-zA-Z0-9.-]+$").unwrap();

    let mut group = c.benchmark_group("validation/matrix_id");
    group.throughput(Throughput::Elements(1));

    group.bench_function("valid", |b| {
        b.iter(|| {
            let user_id = black_box("@alice:example.com");
            matrix_id_regex.is_match(user_id)
        })
    });

    group.bench_function("valid_complex", |b| {
        b.iter(|| {
            let user_id = black_box("@user_name-with.special:chars.server.org");
            matrix_id_regex.is_match(user_id)
        })
    });

    group.bench_function("invalid", |b| {
        b.iter(|| {
            let user_id = black_box("InvalidUser:server");
            matrix_id_regex.is_match(user_id)
        })
    });

    group.finish();
}

/// Benchmark password validation (complex operations).
fn bench_validation_password(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation/password");
    group.throughput(Throughput::Elements(1));

    group.bench_function("valid", |b| {
        b.iter(|| {
            let password = black_box("SecureP@ssw0rd123!");
            let has_upper = password.chars().any(|c| c.is_uppercase());
            let has_lower = password.chars().any(|c| c.is_lowercase());
            let has_digit = password.chars().any(|c| c.is_ascii_digit());
            let has_special = password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));
            has_upper && has_lower && has_digit && has_special
        })
    });

    group.bench_function("invalid_no_upper", |b| {
        b.iter(|| {
            let password = black_box("password123!");
            let has_upper = password.chars().any(|c| c.is_uppercase());
            has_upper
        })
    });

    group.finish();
}

/// Benchmark string operations common in user search.
fn bench_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("strings");

    group.bench_function("to_lowercase_contains", |b| {
        let error_msg = black_box("duplicate key value violates unique constraint");
        b.iter(|| {
            let error_msg_lower = error_msg.to_lowercase();
            error_msg_lower.contains("duplicate key") ||
                error_msg_lower.contains("unique constraint") ||
                error_msg_lower.contains("23505")
        })
    });

    group.bench_function("regex_match_multiple", |b| {
        use once_cell::sync::Lazy;
        use regex::Regex;

        static UNIQUE_VIOLATION: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)(duplicate key|unique constraint|23505|duplicatekeyvalue|duplicate_key|violates unique constraint)").unwrap()
        });

        let error_msg = black_box("duplicate key value violates unique constraint \"users_username_key\"");
        b.iter(|| {
            UNIQUE_VIOLATION.is_match(error_msg)
        })
    });

    group.finish();
}

/// Benchmark common data structure operations.
fn bench_data_structures(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_structures");

    // HashMap lookup vs Vec search
    let vec_data: Vec<String> = (0..1000).map(|i| format!("user_{}", i)).collect();
    let mut map_data = std::collections::HashMap::new();
    for item in &vec_data {
        map_data.insert(item.as_str(), true);
    }

    group.bench_function("vec_search_1000", |b| {
        let target = black_box("user_500");
        b.iter(|| {
            vec_data.iter().any(|x| x == target)
        })
    });

    group.bench_function("hashmap_lookup_1000", |b| {
        let target = black_box("user_500");
        b.iter(|| {
            map_data.contains_key(target)
        })
    });

    // HashSet membership check
    let set_data: std::collections::HashSet<&str> = vec_data.iter().map(|s| s.as_str()).collect();

    group.bench_function("hashset_contains_1000", |b| {
        let target = black_box("user_500");
        b.iter(|| {
            set_data.contains(target)
        })
    });

    group.finish();
}

/// Benchmark serialization/deserialization operations.
fn bench_serialization(c: &mut Criterion) {
    use serde_json::json;

    let mut group = c.benchmark_group("serialization");

    let user_data = json!({
        "user_id": "@alice:localhost",
        "username": "alice",
        "displayname": "Alice",
        "avatar_url": "mxc://localhost/media",
        "is_admin": false,
        "deactivated": false
    });

    group.bench_function("serialize_user", |b| {
        b.iter(|| {
            serde_json::to_string(black_box(&user_data))
        })
    });

    let serialized = user_data.to_string();
    group.bench_function("deserialize_user", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(black_box(&serialized))
        })
    });

    group.finish();
}

/// Benchmark collection operations for user search results.
fn bench_collection_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("collections");

    let users: Vec<String> = (0..100).map(|i| format!("@user{}:localhost", i)).collect();
    let _friend_ids: std::collections::HashSet<String> =
        (0..50).map(|i| format!("@user{}:localhost", i)).collect();
    let blocked_ids: std::collections::HashSet<String> =
        vec!["@user10:localhost".to_string(), "@user20:localhost".to_string()].into_iter().collect();

    group.bench_function("filter_with_hashset_100", |b| {
        b.iter(|| {
            users.iter().filter(|u| !blocked_ids.contains(u.as_str())).count()
        })
    });

    group.throughput(Throughput::Elements(10));
    group.bench_function("map_collect_100", |b| {
        b.iter(|| {
            users.to_vec()
        })
    });

    group.finish();
}

/// Benchmark timestamp operations.
fn bench_timestamp_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("timestamps");

    group.bench_function("chrono_now", |b| {
        b.iter(|| {
            chrono::Utc::now().timestamp()
        })
    });

    group.bench_function("chrono_now_millis", |b| {
        b.iter(|| {
            chrono::Utc::now().timestamp_millis()
        })
    });

    group.finish();
}

/// Benchmark format operations (used in API responses).
fn bench_format_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("format");

    group.bench_function("format_user_id", |b| {
        b.iter(|| {
            format!("@{}:localhost", black_box("alice"))
        })
    });

    group.bench_function("format_with_insert", |b| {
        b.iter(|| {
            "INSERT INTO users (user_id) VALUES ($1, $2)".to_string()
        })
    });

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(100).warm_up_time(std::time::Duration::from_secs(1));
    targets =
        bench_validation_username,
        bench_validation_matrix_id,
        bench_validation_password,
        bench_string_operations,
        bench_data_structures,
        bench_serialization,
        bench_collection_operations,
        bench_timestamp_operations,
        bench_format_operations
);

criterion_main!(benches);
