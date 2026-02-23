use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use synapse_rust::concurrency::{ConcurrencyController, ConcurrencyLimiter};
use tokio::sync::Semaphore;

fn bench_concurrency_controller(c: &mut Criterion) {
    let controller = ConcurrencyController::new(10, "test".to_string());

    let mut group = c.benchmark_group("concurrency_controller");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _controller = black_box(ConcurrencyController::new(10, "test".to_string()));
        })
    });

    group.bench_function("available_permits", |b| {
        b.iter(|| {
            let controller = black_box(&controller);
            controller.available_permits()
        })
    });

    group.bench_function("try_acquire_success", |b| {
        let controller = ConcurrencyController::new(10, "test".to_string());
        b.iter(|| {
            let controller = black_box(&controller);
            let _permit = controller.try_acquire();
        })
    });

    group.bench_function("try_acquire_failure", |b| {
        let controller = ConcurrencyController::new(1, "test".to_string());
        let _permit = controller.try_acquire();
        b.iter(|| {
            let controller = black_box(&controller);
            let _permit = controller.try_acquire();
        })
    });

    group.finish();
}

fn bench_concurrency_limiter(c: &mut Criterion) {
    let mut limiter = ConcurrencyLimiter::new();
    limiter.add_controller("controller_1".to_string(), 10);
    limiter.add_controller("controller_2".to_string(), 20);
    limiter.add_controller("controller_3".to_string(), 30);

    let mut group = c.benchmark_group("concurrency_limiter");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _limiter = black_box(ConcurrencyLimiter::new());
        })
    });

    group.bench_function("add_controller", |b| {
        b.iter(|| {
            let mut limiter = black_box(ConcurrencyLimiter::new());
            limiter.add_controller("test".to_string(), 10);
        })
    });

    group.bench_function("get_controller", |b| {
        b.iter(|| {
            let limiter = black_box(&limiter);
            limiter.get_controller("controller_1")
        })
    });

    group.finish();
}

fn bench_semaphore_operations(c: &mut Criterion) {
    let semaphore = Arc::new(Semaphore::new(10));

    let mut group = c.benchmark_group("semaphore");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _semaphore = black_box(Arc::new(Semaphore::new(10)));
        })
    });

    group.bench_function("available_permits", |b| {
        b.iter(|| {
            let semaphore = black_box(&semaphore);
            semaphore.available_permits()
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_concurrency_controller,
    bench_concurrency_limiter,
    bench_semaphore_operations
);
criterion_main!(benches);
