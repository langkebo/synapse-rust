use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::regex_cache::RegexCache;

fn bench_cache_operations(c: &mut Criterion) {
    let cache = CacheManager::new(CacheConfig::default());
    let key = "test_key";
    let value = "test_value".to_string();

    let group = c.benchmark_group("cache_operations");

    group.bench_with_input(
        BenchmarkId::new("set", "write"),
        &(&key, &value),
        |b, (key, value)| {
            b.iter(|| {
                let cache = black_box(&cache);
                let key = black_box(key);
                let value = black_box(value);
                cache.set(key, value, 60)
            })
        },
    );

    group.bench_with_input(BenchmarkId::new("get", "hit"), &key, |b, key| {
        b.iter(|| {
            let cache = black_box(&cache);
            let key = black_box(key);
            cache.get::<String>(key)
        })
    });

    group.bench_with_input(
        BenchmarkId::new("get", "miss"),
        &"nonexistent_key",
        |b, key| {
            b.iter(|| {
                let cache = black_box(&cache);
                let key = black_box(key);
                cache.get::<String>(key)
            })
        },
    );

    group.bench_with_input(BenchmarkId::new("delete", "existing"), &key, |b, key| {
        b.iter(|| {
            let cache = black_box(&cache);
            let key = black_box(key);
            cache.delete(key)
        })
    });

    group.finish();
}

fn bench_regex_cache(c: &mut Criterion) {
    let cache = RegexCache::new();
    let pattern = r"\d{3}-\d{3}-\d{4}";
    let text = "123-456-7890";

    let group = c.benchmark_group("regex_cache");

    group.bench_with_input(
        BenchmarkId::new("compile", "first_time"),
        pattern,
        |b, pattern| {
            b.iter(|| {
                let cache = black_box(&cache);
                let pattern = black_box(pattern);
                cache.get_or_create(pattern)
            })
        },
    );

    group.bench_with_input(BenchmarkId::new("is_match", "cached"), &text, |b, text| {
        let _ = cache.get_or_create(pattern);
        b.iter(|| {
            let cache = black_box(&cache);
            let text = black_box(text);
            cache.is_match(pattern, text)
        })
    });

    group.finish();
}

fn bench_regex_cache_patterns(c: &mut Criterion) {
    let cache = RegexCache::new();
    let patterns = vec![
        r"\d+",
        r"[a-z]+",
        r"\w+",
        r"^test.*",
        r".*end$",
        r"\d{4}-\d{2}-\d{2}",
        r"[A-Z][a-z]+",
        r"(\w+)@(\w+)\.(\w+)",
        r"\b\w+\b",
        r"(?:http|https)://[^\s]+",
    ];

    let group = c.benchmark_group("regex_cache_patterns");

    group.bench_function("compile_multiple", |b| {
        b.iter(|| {
            let cache = black_box(&cache);
            for pattern in &patterns {
                let _ = cache.get_or_create(pattern);
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cache_operations,
    bench_regex_cache,
    bench_regex_cache_patterns
);
criterion_main!(benches);
