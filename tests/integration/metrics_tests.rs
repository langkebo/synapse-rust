#[cfg(test)]
mod metrics_integration_tests {
    use synapse_rust::metrics::{Counter, Gauge, Histogram, MetricsCollector};

    #[test]
    fn test_metrics_collector_workflow() {
        let collector = MetricsCollector::new();

        let counter = collector.register_counter("http_requests".to_string());
        let gauge = collector.register_gauge("active_connections".to_string());
        let histogram = collector.register_histogram("request_duration".to_string());

        for _ in 0..100 {
            counter.inc();
        }

        gauge.set(50.0);

        for i in 0..100 {
            histogram.observe(i as f64);
        }

        let counter_value = counter.get();
        assert_eq!(counter_value, 100);

        let gauge_value = gauge.get();
        assert_eq!(gauge_value, 50.0);

        let histogram_count = histogram.get_count();
        assert_eq!(histogram_count, 100);

        let histogram_sum = histogram.get_sum();
        assert_eq!(histogram_sum, 4950.0);

        let metrics = collector.collect_metrics();
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_counter_with_labels() {
        let mut labels = std::collections::HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        labels.insert("endpoint".to_string(), "/api/users".to_string());

        let counter = Counter::with_labels("http_requests".to_string(), labels);

        counter.inc();
        counter.inc_by(5);

        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge_operations() {
        let gauge = Gauge::new("memory_usage".to_string());

        gauge.set(100.0);
        assert_eq!(gauge.get(), 100.0);

        gauge.inc();
        assert_eq!(gauge.get(), 101.0);

        gauge.dec();
        assert_eq!(gauge.get(), 100.0);

        gauge.add(50.0);
        assert_eq!(gauge.get(), 150.0);

        gauge.sub(30.0);
        assert_eq!(gauge.get(), 120.0);
    }

    #[test]
    fn test_histogram_percentiles() {
        let histogram = Histogram::new("latency".to_string());

        for i in 1..=100 {
            histogram.observe(i as f64);
        }

        assert_eq!(histogram.get_percentile(50.0), 50.0);
        assert_eq!(histogram.get_percentile(90.0), 90.0);
        assert_eq!(histogram.get_percentile(99.0), 99.0);
    }

    #[test]
    fn test_metrics_collector_multiple_registers() {
        let collector = MetricsCollector::new();

        for i in 0..10 {
            collector.register_counter(format!("counter_{}", i));
            collector.register_gauge(format!("gauge_{}", i));
            collector.register_histogram(format!("histogram_{}", i));
        }

        let metrics = collector.collect_metrics();
        // 10 counters + 10 gauges + 10 histograms * 3 (count, sum, avg) = 50
        assert_eq!(metrics.len(), 50);
    }
}
