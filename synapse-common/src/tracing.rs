//! Request-id propagation tower layer.

use std::fmt;
use tracing::span::{Attributes, Id};
use tracing::Subscriber;
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::Layer;

/// Value type used to store request_id in span extensions for
/// cross-span propagation without relying on the field-value visitor pattern.
#[derive(Debug, Clone)]
/// Represents RequestId.
pub struct RequestId(pub String);

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A tracing-subscriber layer that propagates `RequestId` from parent spans
/// to child spans. This ensures the `request_id` appears in all log events
/// regardless of how deep the span nesting goes.
///
/// Usage: insert this layer *before* the fmt/otel layers so that child
/// spans inherit the request_id when the subscriber emits events.
///
/// Two ways a span can carry the request_id:
/// 1. Field on the span itself — set via `info_span!("name", request_id = %rid)`.
///    On root span creation we read this field and store it in extensions.
/// 2. Existing `RequestId` extension (set by other code paths).
///
/// Once in extensions, child spans pick it up via the parent-chain walk
/// in `on_new_span` (case 1 + 2 unified).
pub struct RequestIdPropagationLayer;

impl<S> Layer<S> for RequestIdPropagationLayer
where
    S: Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: LayerContext<'_, S>) {
        let span = match ctx.span(id) {
            Some(s) => s,
            None => return,
        };

        // Case 1: this span itself has a `request_id` field — extract it
        // and store in extensions. This is the "source" path used by the
        // request_id_middleware which creates `info_span!("http_request",
        // request_id = %rid)` but has no access to `extensions_mut()` from
        // the root crate.
        let mut visitor = RequestIdFieldVisitor::default();
        attrs.record(&mut visitor);
        if let Some(rid) = visitor.value {
            span.extensions_mut().insert(RequestId(rid));
            return;
        }

        // Case 2: walk up the parent chain to inherit RequestId from the
        // nearest ancestor that has one stored in extensions.
        let mut parent = ctx.lookup_current();
        while let Some(ref p) = parent {
            if let Some(rid) = p.extensions().get::<RequestId>() {
                span.extensions_mut().insert(rid.clone());
                return;
            }
            parent = p.parent();
        }
    }
}

#[derive(Default)]
struct RequestIdFieldVisitor {
    value: Option<String>,
}

impl tracing::field::Visit for RequestIdFieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "request_id" {
            self.value = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "request_id" {
            // Fall back to Debug formatting for non-str values (e.g. RequestId).
            self.value = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::info_span;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::registry::LookupSpan;

    /// Records, for every newly created span, the `RequestId` that
    /// `RequestIdPropagationLayer` left in its extensions. Registered *after*
    /// the layer under test so it observes the result of `on_new_span`.
    #[derive(Clone, Default)]
    struct CapturingLayer {
        seen: Arc<Mutex<Vec<Option<String>>>>,
    }

    impl<S> Layer<S> for CapturingLayer
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        fn on_new_span(&self, _attrs: &Attributes<'_>, id: &Id, ctx: LayerContext<'_, S>) {
            let rid = ctx.span(id).and_then(|span| span.extensions().get::<RequestId>().map(|r| r.0.clone()));
            self.seen.lock().expect("test mutex").push(rid);
        }
    }

    fn subscriber_with_capture() -> (CapturingLayer, impl Subscriber) {
        let capture = CapturingLayer::default();
        let subscriber = tracing_subscriber::registry().with(RequestIdPropagationLayer).with(capture.clone());
        (capture, subscriber)
    }

    #[test]
    fn test_request_id_display() {
        let rid = RequestId("req-abc123".to_string());
        assert_eq!(rid.to_string(), "req-abc123");
    }

    #[test]
    fn on_new_span_stores_request_id_field_then_child_spans_inherit_it() {
        let (capture, subscriber) = subscriber_with_capture();

        tracing::subscriber::with_default(subscriber, || {
            // Case 1: the root span carries the `request_id` field itself.
            let root = info_span!("http_request", request_id = "req-root");
            {
                let _root = root.enter();
                // Case 2: no field, so walk up to the entered parent.
                let child = info_span!("handler");
                let _child = child.enter();
                let grandchild = info_span!("storage.query");
                drop(grandchild);
            }
            // A span with neither the field nor a tagged ancestor stays untagged.
            let unrelated = info_span!("unrelated");
            let _unrelated = unrelated.enter();
            let unrelated_child = info_span!("unrelated.child");
            drop(unrelated_child);
        });

        let seen = capture.seen.lock().expect("test mutex").clone();
        assert_eq!(
            seen,
            vec![Some("req-root".to_string()), Some("req-root".to_string()), Some("req-root".to_string()), None, None,]
        );
    }

    #[test]
    fn on_new_span_falls_back_to_debug_formatting_for_non_str_values() {
        let (capture, subscriber) = subscriber_with_capture();

        tracing::subscriber::with_default(subscriber, || {
            let span = info_span!("http_request", request_id = 42);
            drop(span);
        });

        let seen = capture.seen.lock().expect("test mutex").clone();
        assert_eq!(seen, vec![Some("42".to_string())]);
    }
}
