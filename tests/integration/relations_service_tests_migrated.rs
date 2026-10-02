#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_services::relations_service::{
    AggregationItem, AggregationResponse, RelationQuery, RelationsResponse, RelationsService, SendAnnotationRequest,
    SendReferenceRequest, SendReplacementRequest,
};
use synapse_services::room::messaging::service::{MessagingService, MessagingServiceConfig};
use synapse_services::room::summary::RoomSummaryService;
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::relations::RelationsStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_summary::RoomSummaryStorage;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Mirrors the production wiring: relation senders persist through the same
/// `MessagingService` write entry every other local event uses.
fn create_service(pool: &Arc<sqlx::PgPool>) -> RelationsService {
    let storage = Arc::new(RelationsStorage::new(pool));
    let event_storage: Arc<EventStorage> = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let member_storage = Arc::new(RoomMemberStorage::new(pool, "localhost"));
    let room_summary_service = Arc::new(RoomSummaryService::new(
        Arc::new(RoomSummaryStorage::new(pool)),
        event_storage.clone(),
        Some(member_storage.clone()),
    ));
    let messaging = Arc::new(MessagingService::new(MessagingServiceConfig {
        event_reader: event_storage.clone(),
        event_writer: event_storage,
        room_storage: Arc::new(RoomStorage::new(pool)),
        member_storage,
        server_name: "localhost".to_string(),
        beacon_service: None,
        task_queue: None,
        relations_storage: storage.clone(),
        event_broadcaster: None,
        app_service_manager: None,
        key_rotation_manager: None,
        room_summary_service,
        cache: Arc::new(CacheManager::new(&CacheConfig::default())),
    }));
    RelationsService::new(storage, "localhost".to_string(), messaging)
}

#[tokio::test]
async fn test_send_annotation() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_send_annotation_content_includes_relates_to() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_send_reference() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_send_reference_with_custom_relation_type() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_send_reference_non_object_content_gets_replaced() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_send_replacement() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_send_replacement_second_edit_is_a_new_event() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    // Each edit is its own persisted event; reusing the first edit's id would
    // collide on the `events` primary key.
    assert_ne!(second.event_id, first_event_id, "each edit must be a distinct persisted event");
    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_id = ANY($1)")
        .bind(vec![first_event_id.clone(), second.event_id.clone()])
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(persisted, 2, "both edits must exist in `events`");

    // Reads still surface the most recent edit for that sender.
    let storage = RelationsStorage::new(&pool);
    let latest = storage.get_replacement(&room_id, &relates_to, &sender).await.unwrap().unwrap();
    assert_eq!(latest.event_id, second.event_id);
}

#[tokio::test]
async fn test_send_replacement_different_senders_independent() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[tokio::test]
async fn test_get_relations_empty() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let relates_to = format!("$orig_{suffix}:localhost");

    let response = service.get_relations(&room_id, &relates_to, RelationQuery::default()).await.unwrap();

    assert!(response.chunk.is_empty());
    assert_eq!(response.total, Some(0));
    assert!(response.next_batch.is_none());
    assert!(response.prev_batch.is_none());
}

#[tokio::test]
async fn test_get_relations_with_data() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let response = service.get_relations(&room_id, &relates_to, RelationQuery::default()).await.unwrap();

    assert_eq!(response.chunk.len(), 1);
    assert_eq!(response.total, Some(1));
    let item = &response.chunk[0];
    assert_eq!(item["type"], "m.relates_to");
    assert_eq!(item["sender"], sender);
}

#[tokio::test]
async fn test_get_relations_filtered_by_type() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
        .get_relations(
            &room_id,
            &relates_to,
            RelationQuery { rel_type: Some("m.annotation".to_string()), ..Default::default() },
        )
        .await
        .unwrap();

    assert_eq!(response.chunk.len(), 1);
    assert_eq!(response.total, Some(1));
}

#[tokio::test]
async fn test_get_relations_with_limit() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
        .get_relations(&room_id, &relates_to, RelationQuery { limit: Some(3), ..Default::default() })
        .await
        .unwrap();

    assert_eq!(response.chunk.len(), 3);
    assert_eq!(response.total, Some(5));
}

#[tokio::test]
async fn test_get_aggregations_empty() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let relates_to = format!("$orig_{suffix}:localhost");

    let response = service.get_aggregations(&room_id, &relates_to).await.unwrap();

    assert!(response.chunk.is_empty());
}

#[tokio::test]
async fn test_get_aggregations_with_annotations() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let response = service.get_aggregations(&room_id, &relates_to).await.unwrap();

    assert_eq!(response.chunk.len(), 2);
    let thumbs_up = response.chunk.iter().find(|item| item.key.as_deref() == Some("👍")).unwrap();
    assert_eq!(thumbs_up.count, 3);
    assert_eq!(thumbs_up.event_type, "m.annotation");

    let heart = response.chunk.iter().find(|item| item.key.as_deref() == Some("❤️")).unwrap();
    assert_eq!(heart.count, 1);
}

#[tokio::test]
async fn test_redact_relation_own_sender() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let result = service.redact_relation(&room_id, &annotation.event_id, &sender).await;
    assert!(result.is_ok());

    let storage = RelationsStorage::new(&pool);
    let found = storage.get_relation(&room_id, &annotation.event_id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_redact_relation_different_sender_forbidden() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let result = service.redact_relation(&room_id, &annotation.event_id, &other_sender).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_redact_relation_nonexistent() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let sender = format!("@user_{suffix}:localhost");

    let result = service.redact_relation(&room_id, "$nonexistent:localhost", &sender).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_annotation_exists_true() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let exists = service.annotation_exists(&room_id, &relates_to, &sender, "👍").await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn test_annotation_exists_false_different_sender() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let exists = service.annotation_exists(&room_id, &relates_to, &other_sender, "👍").await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_annotation_exists_false_no_annotation() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let sender = format!("@user_{suffix}:localhost");
    let relates_to = format!("$orig_{suffix}:localhost");

    let exists = service.annotation_exists(&room_id, &relates_to, &sender, "👍").await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_redacted_relation_excluded_from_get_relations() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    service.redact_relation(&room_id, &annotation.event_id, &sender).await.unwrap();

    let response = service.get_relations(&room_id, &relates_to, RelationQuery::default()).await.unwrap();

    assert!(response.chunk.is_empty());
    assert_eq!(response.total, Some(0));
}

#[tokio::test]
async fn test_redacted_annotation_excluded_from_exists() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let sender = format!("@user_{suffix}:localhost");
    let relates_to = format!("$orig_{suffix}:localhost");

    let request = SendAnnotationRequest {
        room_id: room_id.clone(),
        relates_to_event_id: relates_to.clone(),
        sender: sender.clone(),
        key: "👍".to_string(),
        origin_server_ts: 22000,
    };
    let annotation = service.send_annotation(request).await.unwrap();

    service.redact_relation(&room_id, &annotation.event_id, &sender).await.unwrap();

    let storage = RelationsStorage::new(&pool);
    let relations = storage
        .get_relations(synapse_storage::relations::RelationQueryParams {
            room_id: room_id.clone(),
            relates_to_event_id: relates_to.clone(),
            relation_type: Some("m.annotation".to_string()),
            limit: None,
            from: None,
            direction: None,
            recurse: false,
            event_type: None,
        })
        .await
        .unwrap();
    assert!(relations.is_empty());
}

#[tokio::test]
async fn test_get_aggregations_excludes_redacted() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    service.redact_relation(&room_id, &annotation.event_id, &sender).await.unwrap();

    let response = service.get_aggregations(&room_id, &relates_to).await.unwrap();

    assert!(response.chunk.is_empty());
}

#[tokio::test]
async fn test_multiple_annotations_same_key_aggregated() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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

    let response = service.get_aggregations(&room_id, &relates_to).await.unwrap();

    assert_eq!(response.chunk.len(), 1);
    assert_eq!(response.chunk[0].count, 5);
    assert_eq!(response.chunk[0].key.as_deref(), Some("🔥"));
}

#[tokio::test]
async fn test_get_relations_backward_direction() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
        .get_relations(&room_id, &relates_to, RelationQuery { direction: Some("b".to_string()), ..Default::default() })
        .await
        .unwrap();

    assert_eq!(response.chunk.len(), 3);
}

#[tokio::test]
async fn test_send_replacement_content_structure() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!room_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
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
}

#[test]
fn test_relations_response_serialization() {
    let response = RelationsResponse {
        chunk: vec![serde_json::json!({"test": "value"})],
        next_batch: Some("batch_token".to_string()),
        prev_batch: None,
        total: Some(42),
        recursion_depth: Some(3),
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["chunk"][0]["test"], "value");
    assert_eq!(json["next_batch"], "batch_token");
    assert!(json.get("prev_batch").is_none_or(|v| v.is_null()));
    assert_eq!(json["total"], 42);
    assert_eq!(json["recursion_depth"], 3);
}

#[test]
fn test_relations_response_total_skipped_when_none() {
    let response =
        RelationsResponse { chunk: vec![], next_batch: None, prev_batch: None, total: None, recursion_depth: None };

    let json = serde_json::to_value(&response).unwrap();
    assert!(json.get("total").is_none());
    // MSC3981：没传 `recurse` 时 `recursion_depth` 必须缺席。
    assert!(json.get("recursion_depth").is_none());
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

// ── MSC3981: recursive /relations ───────────────────────────────────────

/// 建一条 `rel_type` 关系事件（`m.thread`/`m.edit`/… 走普通消息事件），
/// 返回它的 event_id。
async fn send_relation_event(
    service: &RelationsService,
    room_id: &str,
    sender: &str,
    relates_to: &str,
    relation_type: &str,
    origin_server_ts: i64,
) -> String {
    service
        .send_reference(SendReferenceRequest {
            room_id: room_id.to_string(),
            relates_to_event_id: relates_to.to_string(),
            sender: sender.to_string(),
            content: serde_json::json!({"body": relation_type}),
            origin_server_ts,
            relation_type: Some(relation_type.to_string()),
        })
        .await
        .unwrap()
        .event_id
}

/// 建一条 `m.annotation`（event_type = `m.reaction`）关系事件。
async fn send_reaction(
    service: &RelationsService,
    room_id: &str,
    sender: &str,
    relates_to: &str,
    origin_server_ts: i64,
) -> String {
    service
        .send_annotation(SendAnnotationRequest {
            room_id: room_id.to_string(),
            relates_to_event_id: relates_to.to_string(),
            sender: sender.to_string(),
            key: "👍".to_string(),
            origin_server_ts,
        })
        .await
        .unwrap()
        .event_id
}

fn chunk_ids(response: &RelationsResponse) -> Vec<String> {
    response.chunk.iter().map(|item| item["event_id"].as_str().unwrap().to_string()).collect()
}

/// V-11 / MSC3981：MSC 的示例图 —— 直连 A←B、A←G（`m.thread`）、A←D
/// （`m.edit`），以及挂在 B 上的 E（`m.annotation`）。无关事件也在房间里
/// （MSC 图里 C/F 只出现在 `/messages` 的拓扑链上），用来证明递归不会把无关
/// 事件拉进来。
///
/// `origin_server_ts` 刻意与插入序（= `stream_ordering` = 拓扑序）**相反**：
/// 只有真按拓扑序排序才会得到 MSC 断言的顺序，否则 `dir=f` 会返回倒序。
#[tokio::test]
async fn msc3981_recursion_matches_the_reference_graph() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!msc3981_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let sender = format!("@msc3981_{suffix}:localhost");

    let root = format!("$root_{suffix}:localhost");
    let b = send_relation_event(&service, &room_id, &sender, &root, "m.thread", 5000).await;
    let d = send_relation_event(&service, &room_id, &sender, &root, "m.edit", 4000).await;
    let e = send_reaction(&service, &room_id, &sender, &b, 3000).await;
    let g = send_relation_event(&service, &room_id, &sender, &root, "m.thread", 2000).await;
    let other_root = format!("$other_root_{suffix}:localhost");
    let _unrelated_thread = send_relation_event(&service, &room_id, &sender, &other_root, "m.thread", 1000).await;
    let _unrelated_reaction = send_reaction(&service, &room_id, &sender, &other_root, 900).await;

    // 直连 + `rel_type=m.thread` ⇒ [B, G]（拓扑序，不是 origin_server_ts 序）。
    let direct = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                rel_type: Some("m.thread".to_string()),
                limit: Some(50),
                direction: Some("f".to_string()),
                recurse: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&direct), vec![b.clone(), g.clone()]);
    assert_eq!(direct.recursion_depth, Some(3), "传了 recurse=false 也必须回 recursion_depth");

    // `recurse=true, dir=f` ⇒ 直连 + 关系的关系 = [B, D, E, G]。
    let recursed = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                limit: Some(50),
                direction: Some("f".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&recursed), vec![b.clone(), d.clone(), e.clone(), g.clone()]);
    assert_eq!(recursed.recursion_depth, Some(3));
    assert_eq!(recursed.total, None, "递归路径不报 direct-only 的 total");

    // `recurse=true, dir=b, limit=2` ⇒ 拓扑序倒过来取前两条 = [G, E]。
    let backward = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                limit: Some(2),
                direction: Some("b".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&backward), vec![g.clone(), e.clone()]);

    // 缺省（没传 `recurse`）：只有直连，且**不带** `recursion_depth`。
    let default = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery { limit: Some(50), direction: Some("f".to_string()), ..Default::default() },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&default), vec![b.clone(), d.clone(), g.clone()]);
    assert_eq!(default.recursion_depth, None);

    // `rel_type` 过滤作用于**返回集**：E 挂在 B（`m.thread`）下面，仍被返回。
    // MSC 正文那句「过滤同时剪枝中间节点」与 MSC 自己第 5 个示例互相矛盾，此处
    // 跟随参考实现（Synapse 的 CTE 先递归、后过滤），理由见 MSC_SEMANTICS §1.1。
    let annotations = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                rel_type: Some("m.annotation".to_string()),
                limit: Some(50),
                direction: Some("f".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&annotations), vec![e.clone()]);

    // `event_type` 过滤（spec 的 `/{relType}/{eventType}` 路由的数据面）：
    // 与 `rel_type` 同口径，作用在**返回集**上。
    let reactions = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                rel_type: Some("m.annotation".to_string()),
                event_type: Some("m.reaction".to_string()),
                limit: Some(50),
                direction: Some("f".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&reactions), vec![e.clone()], "m.annotation + m.reaction 只应返回表情回应");

    let messages = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                event_type: Some("m.room.message".to_string()),
                limit: Some(50),
                direction: Some("f".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(chunk_ids(&messages), vec![b.clone(), d.clone(), g.clone()], "event_type 过滤必须排除 E");

    // 客户端自己拼的游标 ⇒ 400，而不是静默退回首页（那会让分页死循环）。
    let malformed = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                limit: Some(50),
                from: Some("not-a-cursor".to_string()),
                direction: Some("f".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await;
    assert!(malformed.is_err(), "malformed `from` cursor must be rejected");
}

/// MSC3981：递归深度上限经 `recursion_depth` 如实上报。本仓与上游一致采用 0 基
/// `depth`、`rt.depth <= 3`，即最深的第 5 跳仍返回、第 6 跳缺席。
#[tokio::test]
async fn msc3981_recursion_stops_at_the_advertised_depth() {
    let pool = crate::require_test_pool().await;
    let service = create_service(&pool);
    let suffix = unique_id();
    let room_id = format!("!msc3981depth_{suffix}:localhost");
    crate::ensure_test_room(&pool, &room_id).await;
    let sender = format!("@msc3981depth_{suffix}:localhost");

    let root = format!("$depth_root_{suffix}:localhost");
    let mut ids = Vec::new();
    let mut parent = root.clone();
    for hop in 0..6 {
        let id = send_reaction(&service, &room_id, &sender, &parent, 6000 - hop).await;
        ids.push(id.clone());
        parent = id;
    }

    let response = service
        .get_relations(
            &room_id,
            &root,
            RelationQuery {
                limit: Some(50),
                direction: Some("f".to_string()),
                recurse: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let got = chunk_ids(&response);
    assert_eq!(got, ids[..5].to_vec(), "depth 0..4 (5 hops of relations) must be returned");
    assert!(!got.contains(&ids[5]), "the 6th hop is beyond the advertised recursion depth");
    assert_eq!(response.recursion_depth, Some(3));
}
