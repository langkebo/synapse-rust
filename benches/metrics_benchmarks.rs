use criterion::{black_box, criterion_group, criterion_main, Criterion};
use synapse_rust::metrics::{Counter, Gauge, Histogram, MetricsCollector};

fn bench_counter_operations(c: &mut Criterion) {
    let counter = Counter::new("test_counter".to_string());

    let mut group = c.benchmark_group("counter");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _counter = black_box(Counter::new("test".to_string()));
        })
    });

    group.bench_function("inc", |b| {
        let counter = black_box(&counter);
        b.iter(|| counter.inc())
    });

    group.bench_function("inc_by", |b| {
        let counter = black_box(&counter);
        b.iter(|| counter.inc_by(5))
    });

    group.bench_function("get", |b| {
        let counter = black_box(&counter);
        b.iter(|| counter.get())
    });

    group.bench_function("reset", |b| {
        let counter = black_box(&counter);
        b.iter(|| counter.reset())
    });

    group.finish();
}

fn bench_gauge_operations(c: &mut Criterion) {
    let gauge = Gauge::new("test_gauge".to_string());

    let mut group = c.benchmark_group("gauge");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _gauge = black_box(Gauge::new("test".to_string()));
        })
    });

    group.bench_function("set", |b| {
        let gauge = black_box(&gauge);
        b.iter(|| gauge.set(42.0))
    });

    group.bench_function("inc", |b| {
        let gauge = black_box(&gauge);
        b.iter(|| gauge.inc())
    });

    group.bench_function("dec", |b| {
        let gauge = black_box(&gauge);
        b.iter(|| gauge.dec())
    });

    group.bench_function("add", |b| {
        let gauge = black_box(&gauge);
        b.iter(|| gauge.add(10.0))
    });

    group.bench_function("get", |b| {
        let gauge = black_box(&gauge);
        b.iter(|| gauge.get())
    });

    group.finish();
}

fn bench_histogram_operations(c: &mut Criterion) {
    let histogram = Histogram::new("test_histogram".to_string());

    let mut group = c.benchmark_group("histogram");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _histogram = black_box(Histogram::new("test".to_string()));
        })
    });

    group.bench_function("observe", |b| {
        let histogram = black_box(&histogram);
        b.iter(|| histogram.observe(42.0))
    });

    group.bench_function("get_count", |b| {
        let histogram = black_box(&histogram);
        b.iter(|| histogram.get_count())
    });

    group.bench_function("get_sum", |b| {
        let histogram = black_box(&histogram);
        b.iter(|| histogram.get_sum())
    });

    group.bench_function("get_avg", |b| {
        let histogram = black_box(&histogram);
        b.iter(|| histogram.get_avg())
    });

    group.bench_function("get_percentile_50", |b| {
        let histogram = black_box(&histogram);
        b.iter(|| histogram.get_percentile(50.0))
    });

    group.finish();
}

fn bench_metrics_collector(c: &mut Criterion) {
    let collector = MetricsCollector::new();

    let mut group = c.benchmark_group("metrics_collector");

    group.bench_function("new", |b| {
        b.iter(|| {
            let _collector = black_box(MetricsCollector::new());
        })
    });

    group.bench_function("register_counter", |b| {
        let collector = black_box(&collector);
        b.iter(|| collector.register_counter("test".to_string()))
    });

    group.bench_function("register_gauge", |b| {
        let collector = black_box(&collector);
        b.iter(|| collector.register_gauge("test".to_string()))
    });

    group.bench_function("register_histogram", |b| {
        let collector = black_box(&collector);
        b.iter(|| collector.register_histogram("test".to_string()))
    });

    group.bench_function("collect_metrics_empty", |b| {
        let collector = black_box(&collector);
        b.iter(|| collector.collect_metrics())
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_counter_operations,
    bench_gauge_operations,
    bench_histogram_operations,
    bench_metrics_collector
);
criterion_main!(benches);
