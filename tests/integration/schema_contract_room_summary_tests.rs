#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::services::room_summary_service::RoomSummaryService;
use synapse_rust::storage::event::{CreateEventParams, EventStorage};
use synapse_rust::storage::room_summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryStorage,
    UpdateSummaryMemberRequest,
};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping room summary schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn primary_key_columns(pool: &sqlx::PgPool, table_name: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT a.attname
        FROM pg_index i
        JOIN pg_class c ON c.oid = i.indrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
        WHERE i.indisprimary
          AND n.nspname = 'public'
          AND c.relname = $1
        ORDER BY array_position(i.indkey, a.attnum)
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .expect("Failed to query primary key columns")
}

async fn has_unique_constraint_on(pool: &sqlx::PgPool, table_name: &str, columns: &[&str]) -> bool {
    let rows = sqlx::query(
        r#"
        SELECT tc.constraint_name, kcu.column_name, kcu.ordinal_position
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.table_schema = 'public'
          AND tc.table_name = $1
          AND tc.constraint_type = 'UNIQUE'
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .expect("Failed to query unique constraints");

    let mut current_name: Option<String> = None;
    let mut current_columns: Vec<String> = Vec::new();
    for row in rows {
        let name = row.get::<String, _>("constraint_name");
        let column = row.get::<String, _>("column_name");
        if current_name.as_deref() != Some(name.as_str()) {
            if current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>() {
                return true;
            }
            current_name = Some(name);
            current_columns.clear();
        }
        current_columns.push(column);
    }

    current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>()
}

async fn has_index_named(pool: &sqlx::PgPool, index_name: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_indexes
            WHERE schemaname = 'public' AND indexname = $1
        )
        "#,
    )
    .bind(index_name)
    .fetch_one(pool)
    .await
    .expect("Failed to query pg_indexes")
}

async fn assert_column(
    pool: &sqlx::PgPool,
    table_name: &str,
    column_name: &str,
    expected_types: &[&str],
    expected_nullable: bool,
    expected_default_contains: Option<&str>,
) {
    let row = sqlx::query(
        r#"
        SELECT data_type, is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2
        "#,
    )
    .bind(table_name)
    .bind(column_name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("Expected column {}.{} to exist", table_name, column_name));

    let data_type = row.get::<String, _>("data_type");
    assert!(
        expected_types
            .iter()
            .any(|ty| data_type.eq_ignore_ascii_case(ty)),
        "Expected {}.{} type in {:?}, got {}",
        table_name,
        column_name,
        expected_types,
        data_type
    );

    let is_nullable = row.get::<String, _>("is_nullable");
    assert_eq!(
        is_nullable.eq_ignore_ascii_case("YES"),
        expected_nullable,
        "Unexpected nullable flag for {}.{}",
        table_name,
        column_name
    );

    if let Some(expected_default_fragment) = expected_default_contains {
        let column_default = row
            .get::<Option<String>, _>("column_default")
            .unwrap_or_default();
        assert!(
            column_default.contains(expected_default_fragment),
            "Expected {}.{} default to contain {:?}, got {:?}",
            table_name,
            column_name,
            expected_default_fragment,
            column_default
        );
    }
}

async fn seed_users_and_room(pool: &sqlx::PgPool, suffix: &str) -> (String, String, String) {
    let creator = format!("@schema-summary-integration-creator-{suffix}:localhost");
    let hero = format!("@schema-summary-integration-hero-{suffix}:localhost");
    let room_id = format!("!schema-summary-integration-room-{suffix}:localhost");

    for (user_id, username) in [
        (
            &creator,
            format!("schema_summary_integration_creator_{suffix}"),
        ),
        (&hero, format!("schema_summary_integration_hero_{suffix}")),
    ] {
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed user fixture");
    }

    sqlx::query(
        "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
    )
    .bind(&room_id)
    .bind(&creator)
    .bind(0_i64)
    .execute(pool)
    .await
    .expect("Failed to seed room fixture");

    (creator, hero, room_id)
}

async fn cleanup_room_summary_fixtures(pool: &sqlx::PgPool, room_id: &str, user_ids: &[String]) {
    sqlx::query("DELETE FROM room_children WHERE parent_room_id = $1 OR child_room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_children");

    sqlx::query("DELETE FROM room_summary_update_queue WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_summary_update_queue");

    sqlx::query("DELETE FROM room_summary_stats WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_summary_stats");

    sqlx::query("DELETE FROM room_summary_state WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_summary_state");

    sqlx::query("DELETE FROM room_summary_members WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_summary_members");

    sqlx::query("DELETE FROM room_summaries WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room_summaries");

    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup room fixture");

    for user_id in user_ids {
        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup user fixture");
    }
}

#[tokio::test]
async fn test_schema_contract_room_summary_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_eq!(
        primary_key_columns(&pool, "room_summaries").await,
        vec!["room_id".to_string()],
        "Expected room_summaries PRIMARY KEY(room_id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "room_summary_members", &["room_id", "user_id"]).await,
        "Expected room_summary_members UNIQUE(room_id,user_id)"
    );
    assert!(
        has_unique_constraint_on(
            &pool,
            "room_summary_state",
            &["room_id", "event_type", "state_key"]
        )
        .await,
        "Expected room_summary_state UNIQUE(room_id,event_type,state_key)"
    );
    assert!(
        has_unique_constraint_on(&pool, "room_summary_stats", &["room_id"]).await,
        "Expected room_summary_stats UNIQUE(room_id)"
    );

    assert_column(
        &pool,
        "room_summaries",
        "hero_users",
        &["jsonb"],
        false,
        Some("'[]'::jsonb"),
    )
    .await;
    assert_column(
        &pool,
        "room_summary_members",
        "is_hero",
        &["boolean"],
        false,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "room_summary_state",
        "content",
        &["jsonb"],
        false,
        Some("'{}'::jsonb"),
    )
    .await;
    assert_column(
        &pool,
        "room_summary_stats",
        "total_events",
        &["bigint"],
        false,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "room_summary_update_queue",
        "status",
        &["text", "character varying"],
        false,
        Some("pending"),
    )
    .await;
    assert_column(
        &pool,
        "room_children",
        "suggested",
        &["boolean"],
        false,
        Some("false"),
    )
    .await;

    for index_name in [
        "idx_room_summaries_last_event_ts",
        "idx_room_summaries_space",
        "idx_room_summary_members_user_membership_room",
        "idx_room_summary_members_room_membership_hero_active",
        "idx_room_summary_members_room_hero_user",
        "idx_room_summary_state_room",
        "idx_room_summary_update_queue_status_priority_created",
        "idx_room_children_parent_suggested",
        "idx_room_children_child",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_room_summary_query_and_relation_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = RoomSummaryStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;

    let summary = storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: Some("m.room".to_string()),
            name: Some("Schema Summary".to_string()),
            topic: Some("Room summary contract".to_string()),
            avatar_url: None,
            canonical_alias: Some("#schema-summary:localhost".to_string()),
            join_rule: Some("invite".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("forbidden".to_string()),
            is_direct: Some(false),
            is_space: Some(false),
        })
        .await
        .expect("Failed to create room summary contract fixture");

    assert_eq!(summary.room_id, room_id);
    assert_eq!(summary.member_count, 0);

    storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: creator.clone(),
            display_name: Some("Creator".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            is_hero: Some(false),
            last_active_ts: Some(100),
        })
        .await
        .expect("Failed to add creator summary member");

    storage
        .add_member(CreateSummaryMemberRequest {
            room_id: room_id.clone(),
            user_id: hero.clone(),
            display_name: Some("Hero".to_string()),
            avatar_url: None,
            membership: "invite".to_string(),
            is_hero: Some(false),
            last_active_ts: Some(150),
        })
        .await
        .expect("Failed to add hero summary member");

    let after_insert = storage
        .get_summary(&room_id)
        .await
        .expect("Failed to reload room summary after insert")
        .expect("Room summary should exist");
    assert_eq!(after_insert.member_count, 2);
    assert_eq!(after_insert.joined_member_count, 1);
    assert_eq!(after_insert.invited_member_count, 1);

    let updated_member = storage
        .update_member(
            &room_id,
            &hero,
            UpdateSummaryMemberRequest {
                display_name: Some("Hero Updated".to_string()),
                avatar_url: None,
                membership: Some("join".to_string()),
                is_hero: Some(true),
                last_active_ts: Some(999),
            },
        )
        .await
        .expect("Failed to update hero summary member");
    assert_eq!(updated_member.membership, "join");
    assert!(updated_member.is_hero);

    let after_update = storage
        .get_summary(&room_id)
        .await
        .expect("Failed to reload room summary after member update")
        .expect("Room summary should still exist");
    assert_eq!(after_update.joined_member_count, 2);
    assert_eq!(after_update.invited_member_count, 0);

    let visible_to_hero = storage
        .get_summaries_for_user(&hero)
        .await
        .expect("Failed to query room summaries for hero");
    assert_eq!(visible_to_hero.len(), 1);
    assert_eq!(visible_to_hero[0].room_id, room_id);

    let heroes = storage
        .get_heroes(&room_id, 5)
        .await
        .expect("Failed to query room summary heroes");
    assert_eq!(heroes.len(), 2);
    assert_eq!(heroes[0].user_id, hero);
    assert!(heroes[0].is_hero);

    cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
}

#[tokio::test]
async fn test_schema_contract_room_summary_state_and_stats_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = RoomSummaryStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;

    let updated_state = storage
        .set_state(
            &room_id,
            "m.room.name",
            "",
            Some("$summary-state-updated"),
            serde_json::json!({ "name": "Schema Summary Updated" }),
        )
        .await
        .expect("Failed to upsert room summary state");
    assert_eq!(
        updated_state.event_id.as_deref(),
        Some("$summary-state-updated")
    );

    storage
        .set_state(
            &room_id,
            "m.room.topic",
            "",
            Some("$summary-topic"),
            serde_json::json!({ "topic": "Schema Topic" }),
        )
        .await
        .expect("Failed to insert second room summary state");

    let fetched_state = storage
        .get_state(&room_id, "m.room.name", "")
        .await
        .expect("Failed to fetch room summary state")
        .expect("Expected room summary state to exist");
    assert_eq!(
        fetched_state.content,
        serde_json::json!({ "name": "Schema Summary Updated" })
    );

    let all_state = storage
        .get_all_state(&room_id)
        .await
        .expect("Failed to fetch all room summary state");
    assert_eq!(all_state.len(), 2);

    storage
        .update_stats(&room_id, 15, 4, 11, 2, 4096)
        .await
        .expect("Failed to update room summary stats");

    let fetched_stats = storage
        .get_stats(&room_id)
        .await
        .expect("Failed to fetch room summary stats")
        .expect("Expected room summary stats to exist");
    assert_eq!(fetched_stats.total_events, 15);
    assert_eq!(fetched_stats.total_state_events, 4);
    assert_eq!(fetched_stats.total_messages, 11);
    assert_eq!(fetched_stats.total_media, 2);
    assert_eq!(fetched_stats.storage_size, 4096);

    cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
}

#[tokio::test]
async fn test_schema_contract_room_summary_queue_processor_and_driver_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = Arc::new(RoomSummaryStorage::new(&pool));
    let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
    let service = RoomSummaryService::new(storage.clone(), event_storage.clone(), None);

    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;

    storage
        .create_summary(CreateRoomSummaryRequest {
            room_id: room_id.clone(),
            room_type: None,
            name: Some("Processor Before".to_string()),
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: Some("invite".to_string()),
            history_visibility: Some("shared".to_string()),
            guest_access: Some("forbidden".to_string()),
            is_direct: Some(false),
            is_space: Some(false),
        })
        .await
        .expect("Failed to create room summary processor fixture");

    let state_event_id = format!("$summary-service-state-{suffix}");
    let message_event_id = format!("$summary-service-message-{suffix}");
    let other_event_id = format!("$summary-service-other-{suffix}");
    let missing_event_id = format!("$summary-service-missing-{suffix}");

    event_storage
        .create_event(
            CreateEventParams {
                event_id: state_event_id.clone(),
                room_id: room_id.clone(),
                user_id: creator.clone(),
                event_type: "m.room.name".to_string(),
                content: serde_json::json!({ "name": "Processor Updated" }),
                state_key: Some(String::new()),
                origin_server_ts: 100_i64,
            },
            None,
        )
        .await
        .expect("Failed to create state event fixture");
    event_storage
        .create_event(
            CreateEventParams {
                event_id: message_event_id.clone(),
                room_id: room_id.clone(),
                user_id: creator.clone(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({ "body": "hello", "msgtype": "m.text" }),
                state_key: None,
                origin_server_ts: 200_i64,
            },
            None,
        )
        .await
        .expect("Failed to create message event fixture");
    event_storage
        .create_event(
            CreateEventParams {
                event_id: other_event_id.clone(),
                room_id: room_id.clone(),
                user_id: creator.clone(),
                event_type: "org.example.signal".to_string(),
                content: serde_json::json!({ "body": "service update" }),
                state_key: None,
                origin_server_ts: 300_i64,
            },
            None,
        )
        .await
        .expect("Failed to create other event fixture");

    service
        .queue_update(&room_id, &state_event_id, "m.room.name", Some(""))
        .await
        .expect("Failed to queue state update through service");
    service
        .queue_update(&room_id, &message_event_id, "m.room.message", None)
        .await
        .expect("Failed to queue message update through service");
    service
        .queue_update(&room_id, &other_event_id, "org.example.signal", None)
        .await
        .expect("Failed to queue other update through service");
    service
        .queue_update(&room_id, &missing_event_id, "m.room.topic", Some(""))
        .await
        .expect("Failed to queue missing update through service");

    let processed_first = service
        .process_pending_updates(2)
        .await
        .expect("Failed to process first queue batch");
    assert_eq!(processed_first, 2);

    let processed_second = service
        .process_pending_updates(10)
        .await
        .expect("Failed to process second queue batch");
    assert_eq!(processed_second, 1);

    let processed_third = service
        .process_pending_updates(10)
        .await
        .expect("Failed to process third queue batch");
    assert_eq!(processed_third, 0);

    let processed_queue_rows = sqlx::query(
        r#"
        SELECT event_id, status, processed_ts, error_message, retry_count
        FROM room_summary_update_queue
        WHERE room_id = $1
        ORDER BY priority DESC, created_ts ASC, id ASC
        "#,
    )
    .bind(&room_id)
    .fetch_all(&*pool)
    .await
    .expect("Failed to query processed room summary queue rows");
    assert_eq!(processed_queue_rows.len(), 4);
    let queue_state_by_event: std::collections::HashMap<String, (String, Option<String>, i32)> =
        processed_queue_rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("event_id"),
                    (
                        row.get::<String, _>("status"),
                        row.get::<Option<String>, _>("error_message"),
                        row.get::<i32, _>("retry_count"),
                    ),
                )
            })
            .collect();

    assert_eq!(
        queue_state_by_event
            .get(&state_event_id)
            .map(|(status, _, _)| status.as_str()),
        Some("processed")
    );
    assert_eq!(
        queue_state_by_event
            .get(&message_event_id)
            .map(|(status, _, _)| status.as_str()),
        Some("processed")
    );
    assert_eq!(
        queue_state_by_event
            .get(&other_event_id)
            .map(|(status, _, _)| status.as_str()),
        Some("processed")
    );
    assert_eq!(
        queue_state_by_event
            .get(&missing_event_id)
            .map(|(status, _, _)| status.as_str()),
        Some("failed")
    );
    assert_eq!(
        queue_state_by_event
            .get(&missing_event_id)
            .and_then(|(_, error, _)| error.as_deref()),
        Some("Not found: Event not found")
    );
    assert_eq!(
        queue_state_by_event
            .get(&missing_event_id)
            .map(|(_, _, retry_count)| *retry_count),
        Some(1)
    );

    let updated_name_state = service
        .get_state(&room_id, "m.room.name", "")
        .await
        .expect("Failed to fetch processed room summary state")
        .expect("Expected processed room summary state");
    assert_eq!(
        updated_name_state.event_id.as_deref(),
        Some(state_event_id.as_str())
    );

    let updated_summary = storage
        .get_summary(&room_id)
        .await
        .expect("Failed to fetch updated room summary")
        .expect("Expected updated room summary");
    assert_eq!(updated_summary.name.as_deref(), Some("Processor Updated"));
    assert_eq!(
        updated_summary.last_event_id.as_deref(),
        Some(other_event_id.as_str())
    );
    assert_eq!(updated_summary.last_event_ts, Some(300_i64));
    assert_eq!(updated_summary.last_message_ts, Some(200_i64));

    cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
}
