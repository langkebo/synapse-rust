#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::services::relations_service::{
    AggregationItem, AggregationResponse, RelationsResponse, RelationsService,
    SendAnnotationRequest, SendReferenceRequest, SendReplacementRequest,
};
use synapse_rust::storage::relations::RelationsStorage;
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<sqlx::PgPool>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping relations service tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE event_relations (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            relates_to_event_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            sender TEXT NOT NULL,
            origin_server_ts BIGINT NOT NULL,
            content JSONB NOT NULL DEFAULT '{}',
            is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create event_relations table");

    sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_event_relations_unique
        ON event_relations(event_id, relation_type, sender)
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create event_relations unique index");

    sqlx::query(
        r#"
        CREATE INDEX idx_event_relations_room_relates
        ON event_relations(room_id, relates_to_event_id, relation_type)
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create event_relations room_relates index");

    sqlx::query(
        r#"
        CREATE INDEX idx_event_relations_room_event
        ON event_relations(room_id, event_id)
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create event_relations room_event index");

    Some(pool)
}

fn create_service(pool: &Arc<sqlx::PgPool>) -> RelationsService {
    let storage = Arc::new(RelationsStorage::new(pool));
    RelationsService::new(storage, "localhost".to_string())
}

#[test]
fn test_send_annotation() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 1000,
        };

        let result = service.send_annotation(request).await.unwrap();
        assert_eq!(result.room_id, room_id);
        assert_eq!(result.relates_to_event_id, relates_to);
        assert_eq!(result.relation_type, "m.annotation");
        assert_eq!(result.sender, sender);
        assert!(!result.is_redacted);
        assert!(result.event_id.starts_with('$'));
    });
}

#[test]
fn test_send_annotation_content_includes_relates_to() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "❤️".to_string(),
            origin_server_ts: 2000,
        };

        let result = service.send_annotation(request).await.unwrap();
        let content = result.content.as_object().unwrap();
        assert_eq!(content["body"], "❤️");
        let relates = content["m.relates_to"].as_object().unwrap();
        assert_eq!(relates["rel_type"], "m.annotation");
        assert_eq!(relates["event_id"], relates_to);
    });
}

#[test]
fn test_send_reference() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendReferenceRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            content: serde_json::json!({"msgtype": "m.text", "body": "see this"}),
            origin_server_ts: 3000,
            relation_type: None,
        };

        let result = service.send_reference(request).await.unwrap();
        assert_eq!(result.room_id, room_id);
        assert_eq!(result.relates_to_event_id, relates_to);
        assert_eq!(result.relation_type, "m.reference");
        assert_eq!(result.sender, sender);
        assert!(result.event_id.starts_with('$'));
    });
}

#[test]
fn test_send_reference_with_custom_relation_type() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendReferenceRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            content: serde_json::json!({"body": "thread reply"}),
            origin_server_ts: 4000,
            relation_type: Some("m.thread".to_string()),
        };

        let result = service.send_reference(request).await.unwrap();
        assert_eq!(result.relation_type, "m.thread");
        let content = result.content.as_object().unwrap();
        let relates = content["m.relates_to"].as_object().unwrap();
        assert_eq!(relates["rel_type"], "m.thread");
    });
}

#[test]
fn test_send_reference_non_object_content_gets_replaced() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendReferenceRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            content: serde_json::json!("not an object"),
            origin_server_ts: 5000,
            relation_type: None,
        };

        let result = service.send_reference(request).await.unwrap();
        let content = result.content.as_object().unwrap();
        assert!(content.contains_key("m.relates_to"));
    });
}

#[test]
fn test_send_replacement() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendReplacementRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            new_content: serde_json::json!({"msgtype": "m.text", "body": "edited message"}),
            origin_server_ts: 6000,
        };

        let result = service.send_replacement(request).await.unwrap();
        assert_eq!(result.room_id, room_id);
        assert_eq!(result.relates_to_event_id, relates_to);
        assert_eq!(result.relation_type, "m.replace");
        assert_eq!(result.sender, sender);
        let content = result.content.as_object().unwrap();
        assert!(content.contains_key("m.new_content"));
        let relates = content["m.relates_to"].as_object().unwrap();
        assert_eq!(relates["rel_type"], "m.replace");
    });
}

#[test]
fn test_send_replacement_updates_existing() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request1 = SendReplacementRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            new_content: serde_json::json!({"body": "first edit"}),
            origin_server_ts: 7000,
        };
        let first = service.send_replacement(request1).await.unwrap();
        let first_event_id = first.event_id.clone();

        let request2 = SendReplacementRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            new_content: serde_json::json!({"body": "second edit"}),
            origin_server_ts: 8000,
        };
        let second = service.send_replacement(request2).await.unwrap();

        assert_eq!(second.event_id, first_event_id);
    });
}

#[test]
fn test_send_replacement_different_senders_independent() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender_a = format!("@userA_{suffix}:localhost");
        let sender_b = format!("@userB_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request_a = SendReplacementRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender_a.clone(),
            new_content: serde_json::json!({"body": "edit from A"}),
            origin_server_ts: 9000,
        };
        let result_a = service.send_replacement(request_a).await.unwrap();

        let request_b = SendReplacementRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender_b.clone(),
            new_content: serde_json::json!({"body": "edit from B"}),
            origin_server_ts: 10000,
        };
        let result_b = service.send_replacement(request_b).await.unwrap();

        assert_ne!(result_a.event_id, result_b.event_id);
    });
}

#[test]
fn test_get_relations_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let response = service
            .get_relations(&room_id, &relates_to, None, None, None, None)
            .await
            .unwrap();

        assert!(response.chunk.is_empty());
        assert_eq!(response.total, Some(0));
        assert!(response.next_batch.is_none());
        assert!(response.prev_batch.is_none());
    });
}

#[test]
fn test_get_relations_with_data() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 11000,
        };
        service.send_annotation(request).await.unwrap();

        let response = service
            .get_relations(&room_id, &relates_to, None, None, None, None)
            .await
            .unwrap();

        assert_eq!(response.chunk.len(), 1);
        assert_eq!(response.total, Some(1));
        let item = &response.chunk[0];
        assert_eq!(item["type"], "m.relates_to");
        assert_eq!(item["sender"], sender);
    });
}

#[test]
fn test_get_relations_filtered_by_type() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let annotation_req = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 12000,
        };
        service.send_annotation(annotation_req).await.unwrap();

        let reference_req = SendReferenceRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            content: serde_json::json!({"body": "ref"}),
            origin_server_ts: 13000,
            relation_type: None,
        };
        service.send_reference(reference_req).await.unwrap();

        let response = service
            .get_relations(&room_id, &relates_to, Some("m.annotation"), None, None, None)
            .await
            .unwrap();

        assert_eq!(response.chunk.len(), 1);
        assert_eq!(response.total, Some(1));
    });
}

#[test]
fn test_get_relations_with_limit() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        for i in 0..5 {
            let sender = format!("@user_{i}_{suffix}:localhost");
            let request = SendAnnotationRequest {
                room_id: room_id.clone(),
                relates_to_event_id: relates_to.clone(),
                sender,
                key: format!("emoji_{i}"),
                origin_server_ts: 14000 + i as i64,
            };
            service.send_annotation(request).await.unwrap();
        }

        let response = service
            .get_relations(&room_id, &relates_to, None, Some(3), None, None)
            .await
            .unwrap();

        assert_eq!(response.chunk.len(), 3);
        assert_eq!(response.total, Some(5));
    });
}

#[test]
fn test_get_aggregations_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let response = service
            .get_aggregations(&room_id, &relates_to)
            .await
            .unwrap();

        assert!(response.chunk.is_empty());
    });
}

#[test]
fn test_get_aggregations_with_annotations() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        for i in 0..3 {
            let sender = format!("@sender_{i}_{suffix}:localhost");
            let request = SendAnnotationRequest {
                room_id: room_id.clone(),
                relates_to_event_id: relates_to.clone(),
                sender,
                key: "👍".to_string(),
                origin_server_ts: 15000 + i as i64,
            };
            service.send_annotation(request).await.unwrap();
        }

        let sender_extra = format!("@extra_{suffix}:localhost");
        let extra_req = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender_extra,
            key: "❤️".to_string(),
            origin_server_ts: 16000,
        };
        service.send_annotation(extra_req).await.unwrap();

        let response = service
            .get_aggregations(&room_id, &relates_to)
            .await
            .unwrap();

        assert_eq!(response.chunk.len(), 2);
        let thumbs_up = response
            .chunk
            .iter()
            .find(|item| item.key.as_deref() == Some("👍"))
            .unwrap();
        assert_eq!(thumbs_up.count, 3);
        assert_eq!(thumbs_up.event_type, "m.annotation");

        let heart = response
            .chunk
            .iter()
            .find(|item| item.key.as_deref() == Some("❤️"))
            .unwrap();
        assert_eq!(heart.count, 1);
    });
}

#[test]
fn test_redact_relation_own_sender() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 17000,
        };
        let annotation = service.send_annotation(request).await.unwrap();

        let result = service
            .redact_relation(&room_id, &annotation.event_id, &sender)
            .await;
        assert!(result.is_ok());

        let storage = RelationsStorage::new(&pool);
        let found = storage
            .get_relation(&room_id, &annotation.event_id)
            .await
            .unwrap();
        assert!(found.is_none());
    });
}

#[test]
fn test_redact_relation_different_sender_forbidden() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@owner_{suffix}:localhost");
        let other_sender = format!("@other_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 18000,
        };
        let annotation = service.send_annotation(request).await.unwrap();

        let result = service
            .redact_relation(&room_id, &annotation.event_id, &other_sender)
            .await;
        assert!(result.is_err());
    });
}

#[test]
fn test_redact_relation_nonexistent() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");

        let result = service
            .redact_relation(&room_id, "$nonexistent:localhost", &sender)
            .await;
        assert!(result.is_ok());
    });
}

#[test]
fn test_annotation_exists_true() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 19000,
        };
        service.send_annotation(request).await.unwrap();

        let exists = service
            .annotation_exists(&room_id, &relates_to, &sender, "👍")
            .await
            .unwrap();
        assert!(exists);
    });
}

#[test]
fn test_annotation_exists_false_different_sender() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let other_sender = format!("@other_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 20000,
        };
        service.send_annotation(request).await.unwrap();

        let exists = service
            .annotation_exists(&room_id, &relates_to, &other_sender, "👍")
            .await
            .unwrap();
        assert!(!exists);
    });
}

#[test]
fn test_annotation_exists_false_no_annotation() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let exists = service
            .annotation_exists(&room_id, &relates_to, &sender, "👍")
            .await
            .unwrap();
        assert!(!exists);
    });
}

#[test]
fn test_redacted_relation_excluded_from_get_relations() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 21000,
        };
        let annotation = service.send_annotation(request).await.unwrap();

        service
            .redact_relation(&room_id, &annotation.event_id, &sender)
            .await
            .unwrap();

        let response = service
            .get_relations(&room_id, &relates_to, None, None, None, None)
            .await
            .unwrap();

        assert!(response.chunk.is_empty());
        assert_eq!(response.total, Some(0));
    });
}

#[test]
fn test_redacted_annotation_excluded_from_exists() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 22000,
        };
        service.send_annotation(request).await.unwrap();

        service
            .redact_relation(
                &room_id,
                &format!("$annotation_{suffix}:localhost"),
                &sender,
            )
            .await
            .unwrap();

        let storage = RelationsStorage::new(&pool);
        let relations = storage
            .get_relations(synapse_rust::storage::relations::RelationQueryParams {
                room_id: room_id.clone(),
                relates_to_event_id: relates_to.clone(),
                relation_type: Some("m.annotation".to_string()),
                limit: None,
                from: None,
                direction: None,
            })
            .await
            .unwrap();
        assert!(relations.is_empty());
    });
}

#[test]
fn test_get_aggregations_excludes_redacted() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let request = SendAnnotationRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            key: "👍".to_string(),
            origin_server_ts: 23000,
        };
        let annotation = service.send_annotation(request).await.unwrap();

        service
            .redact_relation(&room_id, &annotation.event_id, &sender)
            .await
            .unwrap();

        let response = service
            .get_aggregations(&room_id, &relates_to)
            .await
            .unwrap();

        assert!(response.chunk.is_empty());
    });
}

#[test]
fn test_multiple_annotations_same_key_aggregated() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        for i in 0..5 {
            let sender = format!("@sender_{i}_{suffix}:localhost");
            let request = SendAnnotationRequest {
                room_id: room_id.clone(),
                relates_to_event_id: relates_to.clone(),
                sender,
                key: "🔥".to_string(),
                origin_server_ts: 24000 + i as i64,
            };
            service.send_annotation(request).await.unwrap();
        }

        let response = service
            .get_aggregations(&room_id, &relates_to)
            .await
            .unwrap();

        assert_eq!(response.chunk.len(), 1);
        assert_eq!(response.chunk[0].count, 5);
        assert_eq!(response.chunk[0].key.as_deref(), Some("🔥"));
    });
}

#[test]
fn test_get_relations_backward_direction() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        for i in 0..3 {
            let sender = format!("@sender_{i}_{suffix}:localhost");
            let request = SendAnnotationRequest {
                room_id: room_id.clone(),
                relates_to_event_id: relates_to.clone(),
                sender,
                key: format!("emoji_{i}"),
                origin_server_ts: 25000 + i as i64,
            };
            service.send_annotation(request).await.unwrap();
        }

        let response = service
            .get_relations(
                &room_id,
                &relates_to,
                None,
                None,
                None,
                Some("b".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(response.chunk.len(), 3);
    });
}

#[test]
fn test_send_replacement_content_structure() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let service = create_service(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:localhost");
        let sender = format!("@user_{suffix}:localhost");
        let relates_to = format!("$orig_{suffix}:localhost");

        let new_content = serde_json::json!({
            "msgtype": "m.text",
            "body": "corrected message"
        });

        let request = SendReplacementRequest {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            sender: sender.clone(),
            new_content: new_content.clone(),
            origin_server_ts: 26000,
        };

        let result = service.send_replacement(request).await.unwrap();
        let content = result.content.as_object().unwrap();
        assert!(content.contains_key("m.new_content"));
        assert_eq!(content["m.new_content"], new_content);
        let relates = content["m.relates_to"].as_object().unwrap();
        assert_eq!(relates["rel_type"], "m.replace");
        assert_eq!(relates["event_id"], relates_to);
    });
}

#[test]
fn test_relations_response_serialization() {
    let response = RelationsResponse {
        chunk: vec![serde_json::json!({"test": "value"})],
        next_batch: Some("batch_token".to_string()),
        prev_batch: None,
        total: Some(42),
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["chunk"][0]["test"], "value");
    assert_eq!(json["next_batch"], "batch_token");
    assert!(json.get("prev_batch").is_none_or(|v| v.is_null()));
    assert_eq!(json["total"], 42);
}

#[test]
fn test_relations_response_total_skipped_when_none() {
    let response = RelationsResponse {
        chunk: vec![],
        next_batch: None,
        prev_batch: None,
        total: None,
    };

    let json = serde_json::to_value(&response).unwrap();
    assert!(json.get("total").is_none());
}

#[test]
fn test_aggregation_response_serialization() {
    let response = AggregationResponse {
        chunk: vec![AggregationItem {
            event_type: "m.annotation".to_string(),
            key: Some("👍".to_string()),
            count: 3,
            sender: None,
            origin_server_ts: None,
        }],
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["chunk"][0]["type"], "m.annotation");
    assert_eq!(json["chunk"][0]["key"], "👍");
    assert_eq!(json["chunk"][0]["count"], 3);
}

#[test]
fn test_send_annotation_request_deserialization() {
    let json = serde_json::json!({
        "room_id": "!room:localhost",
        "relates_to_event_id": "$orig:localhost",
        "sender": "@user:localhost",
        "key": "👍",
        "origin_server_ts": 12345
    });

    let req: SendAnnotationRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.room_id, "!room:localhost");
    assert_eq!(req.key, "👍");
    assert_eq!(req.origin_server_ts, 12345);
}

#[test]
fn test_send_reference_request_deserialization() {
    let json = serde_json::json!({
        "room_id": "!room:localhost",
        "relates_to_event_id": "$orig:localhost",
        "sender": "@user:localhost",
        "content": {"body": "ref"},
        "origin_server_ts": 54321,
        "relation_type": "m.thread"
    });

    let req: SendReferenceRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.relation_type, Some("m.thread".to_string()));
    assert_eq!(req.content["body"], "ref");
}

#[test]
fn test_send_replacement_request_deserialization() {
    let json = serde_json::json!({
        "room_id": "!room:localhost",
        "relates_to_event_id": "$orig:localhost",
        "sender": "@user:localhost",
        "new_content": {"msgtype": "m.text", "body": "edited"},
        "origin_server_ts": 99999
    });

    let req: SendReplacementRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.new_content["body"], "edited");
    assert!(req.new_content["msgtype"].is_string());
}
