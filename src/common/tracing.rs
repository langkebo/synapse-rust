use opentelemetry::trace::TraceContextExt;
use opentelemetry::{global, Context};
use tracing::{error, info, info_span, warn, Span};

pub struct DistributedTracer {
    service_name: String,
}

impl DistributedTracer {
    pub fn new(service_name: String) -> Self {
        Self { service_name }
    }

    pub fn init_tracer(&self) -> Result<(), Box<dyn std::error::Error>> {
        global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
        Ok(())
    }

    pub fn create_span(&self, name: &'static str) -> Span {
        info_span!("{}", name)
    }

    pub fn with_span<F, R>(&self, name: &'static str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let span = self.create_span(name);
        let _enter = span.enter();
        f()
    }

    pub fn get_trace_id(&self) -> Option<String> {
        let cx = Context::current();
        let span = cx.span();
        let span_context = span.span_context();
        if span_context.is_valid() {
            Some(span_context.trace_id().to_string())
        } else {
            None
        }
    }

    pub fn get_span_id(&self) -> Option<String> {
        let cx = Context::current();
        let span = cx.span();
        let span_context = span.span_context();
        if span_context.is_valid() {
            Some(span_context.span_id().to_string())
        } else {
            None
        }
    }
}

impl Default for DistributedTracer {
    fn default() -> Self {
        Self::new("synapse-rust".to_string())
    }
}

#[macro_export]
macro_rules! trace_span {
    ($name:expr, $block:block) => {{
        let tracer = DistributedTracer::default();
        tracer.with_span($name, || $block)
    }};
}

#[macro_export]
macro_rules! trace_async {
    ($name:expr, $future:expr) => {
        async {
            let tracer = DistributedTracer::default();
            let span = tracer.create_span($name);
            let _enter = span.enter();
            $future.await
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distributed_tracer_creation() {
        let tracer = DistributedTracer::new("test-service".to_string());
        assert_eq!(tracer.service_name, "test-service");
    }

    #[test]
    fn test_distributed_tracer_default() {
        let tracer = DistributedTracer::default();
        assert_eq!(tracer.service_name, "synapse-rust");
    }

    #[test]
    fn test_distributed_tracer_create_span() {
        let tracer = DistributedTracer::default();
        let span = tracer.create_span("test_span");
        assert_eq!(span.metadata().name(), "test_span");
    }

    #[test]
    fn test_distributed_tracer_with_span() {
        let tracer = DistributedTracer::default();
        let result = tracer.with_span("test_span", || 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_trace_span_macro() {
        let result = trace_span!("test_span", { 42 });
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_trace_async_macro() {
        let result = trace_async!("test_span", async { 42 }).await;
        assert_eq!(result, 42);
    }
}
