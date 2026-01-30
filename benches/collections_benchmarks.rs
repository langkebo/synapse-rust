use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use synapse_rust::collections::{HashMapBuilder, HashSetBuilder, VecBuilder};

fn bench_vec_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec_builder");

    group.bench_function("with_capacity_10", |b| {
        b.iter(|| {
            let builder = black_box(VecBuilder::new(10));
            let _vec = builder.build();
        })
    });

    group.bench_function("with_capacity_100", |b| {
        b.iter(|| {
            let builder = black_box(VecBuilder::new(100));
            let _vec = builder.build();
        })
    });

    group.bench_function("with_capacity_1000", |b| {
        b.iter(|| {
            let builder = black_box(VecBuilder::new(1000));
            let _vec = builder.build();
        })
    });

    group.bench_function("from_iter_100", |b| {
        b.iter(|| {
            let builder = black_box(VecBuilder::new(100));
            let _vec = builder.from_iter(0..100);
        })
    });

    group.bench_function("vec_with_capacity", |b| {
        b.iter(|| {
            let _vec = black_box(synapse_rust::vec_with_capacity::<i32>(100));
        })
    });

    group.finish();
}

fn bench_hashmap_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap_builder");

    group.bench_function("with_capacity_10", |b| {
        b.iter(|| {
            let builder = black_box(HashMapBuilder::<String, i32>::new(10));
            let _map = builder.build();
        })
    });

    group.bench_function("with_capacity_100", |b| {
        b.iter(|| {
            let builder = black_box(HashMapBuilder::<String, i32>::new(100));
            let _map = builder.build();
        })
    });

    group.bench_function("with_capacity_1000", |b| {
        b.iter(|| {
            let builder = black_box(HashMapBuilder::<String, i32>::new(1000));
            let _map = builder.build();
        })
    });

    group.bench_function("from_iter_100", |b| {
        b.iter(|| {
            let builder = black_box(HashMapBuilder::<String, i32>::new(100));
            let _map = builder.from_iter((0..100).map(|i| (i.to_string(), i)));
        })
    });

    group.bench_function("hashmap_with_capacity", |b| {
        b.iter(|| {
            let _map = black_box(synapse_rust::hashmap_with_capacity::<String, i32>(100));
        })
    });

    group.finish();
}

fn bench_hashset_builder(c: &mut Criterion) {
    let group = c.benchmark_group("hashset_builder");

    group.bench_function("with_capacity_10", |b| {
        b.iter(|| {
            let builder = black_box(HashSetBuilder::<i32>::new(10));
            let _set = builder.build();
        })
    });

    group.bench_function("with_capacity_100", |b| {
        b.iter(|| {
            let builder = black_box(HashSetBuilder::<i32>::new(100));
            let _set = builder.build();
        })
    });

    group.bench_function("with_capacity_1000", |b| {
        b.iter(|| {
            let builder = black_box(HashSetBuilder::<i32>::new(1000));
            let _set = builder.build();
        })
    });

    group.bench_function("from_iter_100", |b| {
        b.iter(|| {
            let builder = black_box(HashSetBuilder::<i32>::new(100));
            let _set = builder.from_iter(0..100);
        })
    });

    group.bench_function("hashset_with_capacity", |b| {
        b.iter(|| {
            let _set = black_box(synapse_rust::hashset_with_capacity::<i32>(100));
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_vec_builder,
    bench_hashmap_builder,
    bench_hashset_builder
);
criterion_main!(benches);
