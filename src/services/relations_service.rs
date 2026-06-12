pub use synapse_services::relations_service::*;

#[cfg(test)]
mod tests {
    use super::{RelationTarget, RelationsResponse};

    #[test]
    fn root_relations_service_reexport_keeps_total_field() {
        let response = RelationsResponse {
            chunk: vec![serde_json::json!({ "event_id": "$event:example.com" })],
            next_batch: None,
            prev_batch: None,
            total: Some(1),
        };

        let json = serde_json::to_value(&response).expect("serialize relations response");
        assert_eq!(json.get("total").and_then(serde_json::Value::as_i64), Some(1));
    }

    #[test]
    fn root_relations_service_reexport_keeps_relation_target_shape() {
        let target =
            RelationTarget { event_id: "$target:example.com".to_string(), rel_type: "m.reference".to_string() };

        assert_eq!(target.event_id, "$target:example.com");
        assert_eq!(target.rel_type, "m.reference");
    }
}
