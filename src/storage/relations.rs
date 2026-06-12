pub use synapse_storage::relations::*;

#[cfg(test)]
mod tests {
    use super::{AggregationResult, EventRelation, RelationQueryParams};

    #[test]
    fn root_relations_storage_reexport_keeps_relation_shape() {
        let relation = EventRelation {
            id: 1,
            room_id: "!test:example.com".to_string(),
            event_id: "$reaction1".to_string(),
            relates_to_event_id: "$original:example.com".to_string(),
            relation_type: "m.annotation".to_string(),
            sender: "@user:example.com".to_string(),
            origin_server_ts: 1234567890,
            content: serde_json::json!({"body": "👍"}),
            is_redacted: false,
            created_ts: 1234567890,
        };

        assert_eq!(relation.room_id, "!test:example.com");
        assert_eq!(relation.relation_type, "m.annotation");
        assert!(!relation.is_redacted);
    }

    #[test]
    fn root_relations_storage_reexport_keeps_query_params_and_aggregation() {
        let params = RelationQueryParams {
            room_id: "!test:example.com".to_string(),
            relates_to_event_id: "$original:example.com".to_string(),
            relation_type: Some("m.annotation".to_string()),
            limit: Some(50),
            from: None,
            direction: Some("f".to_string()),
        };
        let agg = AggregationResult {
            relation_type: "m.annotation".to_string(),
            key: Some("👍".to_string()),
            count: 5,
            sender: None,
        };

        assert_eq!(params.limit, Some(50));
        assert_eq!(agg.count, 5);
        assert_eq!(agg.key.as_deref(), Some("👍"));
    }
}
