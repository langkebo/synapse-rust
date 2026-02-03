use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub timestamp: Instant,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Counter {
    name: String,
    value: Arc<std::sync::atomic::AtomicU64>,
    labels: HashMap<String, String>,
}

impl Counter {
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            labels: HashMap::new(),
        }
    }

    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        Self {
            name,
            value: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            labels,
        }
    }

    pub fn inc(&self) {
        self.value
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn inc_by(&self, delta: u64) {
        self.value
            .fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.value.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub struct Gauge {
    name: String,
    value: Arc<std::sync::atomic::AtomicI64>,
    labels: HashMap<String, String>,
}

impl Gauge {
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            labels: HashMap::new(),
        }
    }

    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        Self {
            name,
            value: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            labels,
        }
    }

    pub fn set(&self, value: f64) {
        self.value
            .store(value as i64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn inc(&self) {
        self.value
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn dec(&self) {
        self.value
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn add(&self, delta: f64) {
        self.value
            .fetch_add(delta as i64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn sub(&self, delta: f64) {
        self.value
            .fetch_sub(delta as i64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> f64 {
        self.value.load(std::sync::atomic::Ordering::Relaxed) as f64
    }
}

#[derive(Debug, Clone)]
pub struct Histogram {
    name: String,
    values: Arc<std::sync::Mutex<Vec<f64>>>,
    labels: HashMap<String, String>,
}

impl Histogram {
    pub fn new(name: String) -> Self {
        Self {
            name,
            values: Arc::new(std::sync::Mutex::new(Vec::new())),
            labels: HashMap::new(),
        }
    }

    pub fn with_labels(name: String, labels: HashMap<String, String>) -> Self {
        Self {
            name,
            values: Arc::new(std::sync::Mutex::new(Vec::new())),
            labels,
        }
    }

    pub fn observe(&self, value: f64) {
        let mut values = self.values.lock().unwrap();
        values.push(value);
    }

    pub fn get_values(&self) -> Vec<f64> {
        let values = self.values.lock().unwrap();
        values.clone()
    }

    pub fn get_count(&self) -> usize {
        let values = self.values.lock().unwrap();
        values.len()
    }

    pub fn get_sum(&self) -> f64 {
        let values = self.values.lock().unwrap();
        values.iter().sum()
    }

    pub fn get_avg(&self) -> f64 {
        let values = self.values.lock().unwrap();
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    pub fn get_percentile(&self, percentile: f64) -> f64 {
        let mut values = self.values.lock().unwrap().clone();
        if values.is_empty() {
            return 0.0;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let index = ((percentile / 100.0) * (values.len() - 1) as f64).floor() as usize;
        values[index.min(values.len() - 1)]
    }

    pub fn reset(&self) {
        let mut values = self.values.lock().unwrap();
        values.clear();
    }
}

pub struct MetricsCollector {
    counters: Arc<std::sync::Mutex<HashMap<String, Counter>>>,
    gauges: Arc<std::sync::Mutex<HashMap<String, Gauge>>>,
    histograms: Arc<std::sync::Mutex<HashMap<String, Histogram>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            counters: Arc::new(std::sync::Mutex::new(HashMap::new())),
            gauges: Arc::new(std::sync::Mutex::new(HashMap::new())),
            histograms: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn register_counter(&self, name: String) -> Counter {
        let counter = Counter::new(name.clone());
        let mut counters = self.counters.lock().unwrap();
        counters.insert(name, counter.clone());
        counter
    }

    pub fn register_counter_with_labels(
        &self,
        name: String,
        labels: HashMap<String, String>,
    ) -> Counter {
        let counter = Counter::with_labels(name.clone(), labels);
        let mut counters = self.counters.lock().unwrap();
        counters.insert(name, counter.clone());
        counter
    }

    pub fn register_gauge(&self, name: String) -> Gauge {
        let gauge = Gauge::new(name.clone());
        let mut gauges = self.gauges.lock().unwrap();
        gauges.insert(name, gauge.clone());
        gauge
    }

    pub fn register_gauge_with_labels(
        &self,
        name: String,
        labels: HashMap<String, String>,
    ) -> Gauge {
        let gauge = Gauge::with_labels(name.clone(), labels);
        let mut gauges = self.gauges.lock().unwrap();
        gauges.insert(name, gauge.clone());
        gauge
    }

    pub fn register_histogram(&self, name: String) -> Histogram {
        let histogram = Histogram::new(name.clone());
        let mut histograms = self.histograms.lock().unwrap();
        histograms.insert(name, histogram.clone());
        histogram
    }

    pub fn register_histogram_with_labels(
        &self,
        name: String,
        labels: HashMap<String, String>,
    ) -> Histogram {
        let histogram = Histogram::with_labels(name.clone(), labels);
        let mut histograms = self.histograms.lock().unwrap();
        histograms.insert(name, histogram.clone());
        histogram
    }

    pub fn get_counter(&self, name: &str) -> Option<Counter> {
        let counters = self.counters.lock().unwrap();
        counters.get(name).cloned()
    }

    pub fn get_gauge(&self, name: &str) -> Option<Gauge> {
        let gauges = self.gauges.lock().unwrap();
        gauges.get(name).cloned()
    }

    pub fn get_histogram(&self, name: &str) -> Option<Histogram> {
        let histograms = self.histograms.lock().unwrap();
        histograms.get(name).cloned()
    }

    pub fn collect_metrics(&self) -> Vec<Metric> {
        let mut metrics = Vec::new();

        let counters = self.counters.lock().unwrap();
        for counter in counters.values() {
            metrics.push(Metric {
                name: counter.name.clone(),
                value: counter.get() as f64,
                timestamp: Instant::now(),
                labels: counter.labels.clone(),
            });
        }

        let gauges = self.gauges.lock().unwrap();
        for gauge in gauges.values() {
            metrics.push(Metric {
                name: gauge.name.clone(),
                value: gauge.get(),
                timestamp: Instant::now(),
                labels: gauge.labels.clone(),
            });
        }

        let histograms = self.histograms.lock().unwrap();
        for histogram in histograms.values() {
            metrics.push(Metric {
                name: format!("{}_count", histogram.name),
                value: histogram.get_count() as f64,
                timestamp: Instant::now(),
                labels: histogram.labels.clone(),
            });
            metrics.push(Metric {
                name: format!("{}_sum", histogram.name),
                value: histogram.get_sum(),
                timestamp: Instant::now(),
                labels: histogram.labels.clone(),
            });
            metrics.push(Metric {
                name: format!("{}_avg", histogram.name),
                value: histogram.get_avg(),
                timestamp: Instant::now(),
                labels: histogram.labels.clone(),
            });
        }

        metrics
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new("test_counter".to_string());
        assert_eq!(counter.get(), 0);
        counter.inc();
        assert_eq!(counter.get(), 1);
        counter.inc_by(5);
        assert_eq!(counter.get(), 6);
        counter.reset();
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_counter_with_labels() {
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        let counter = Counter::with_labels("test_counter".to_string(), labels);
        counter.inc();
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("test_gauge".to_string());
        assert_eq!(gauge.get(), 0.0);
        gauge.set(42.0);
        assert_eq!(gauge.get(), 42.0);
        gauge.inc();
        assert_eq!(gauge.get(), 43.0);
        gauge.dec();
        assert_eq!(gauge.get(), 42.0);
        gauge.add(10.0);
        assert_eq!(gauge.get(), 52.0);
        gauge.sub(2.0);
        assert_eq!(gauge.get(), 50.0);
    }

    #[test]
    fn test_histogram() {
        let histogram = Histogram::new("test_histogram".to_string());
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(3.0);
        assert_eq!(histogram.get_count(), 3);
        assert_eq!(histogram.get_sum(), 6.0);
        assert_eq!(histogram.get_avg(), 2.0);
        assert_eq!(histogram.get_percentile(50.0), 2.0);
    }

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        let counter = collector.register_counter("test_counter".to_string());
        counter.inc();
        assert_eq!(collector.get_counter("test_counter").unwrap().get(), 1);

        let gauge = collector.register_gauge("test_gauge".to_string());
        gauge.set(42.0);
        assert_eq!(collector.get_gauge("test_gauge").unwrap().get(), 42.0);

        let histogram = collector.register_histogram("test_histogram".to_string());
        histogram.observe(1.0);
        assert_eq!(
            collector
                .get_histogram("test_histogram")
                .unwrap()
                .get_count(),
            1
        );

        let metrics = collector.collect_metrics();
        assert!(!metrics.is_empty());
    }
}
