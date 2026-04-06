#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::storage::space::{AddChildRequest, CreateSpaceRequest, SpaceStorage};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping space schema contract integration tests because test database is unavailable: {}",
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

async fn seed_space_users(pool: &sqlx::PgPool, suffix: &str) -> (String, String) {
    let creator = format!("@schema-space-integration-creator-{suffix}:localhost");
    let member = format!("@schema-space-integration-member-{suffix}:localhost");

    for (user_id, username) in [
        (
            &creator,
            format!("schema_space_integration_creator_{suffix}"),
        ),
        (&member, format!("schema_space_integration_member_{suffix}")),
    ] {
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed space user fixture");
    }

    (creator, member)
}

async fn cleanup_space_fixtures(pool: &sqlx::PgPool, space_id: &str, user_ids: &[String]) {
    sqlx::query("DELETE FROM space_events WHERE space_id = $1")
        .bind(space_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup space_events");

    sqlx::query("DELETE FROM space_statistics WHERE space_id = $1")
        .bind(space_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup space_statistics");

    sqlx::query("DELETE FROM space_summaries WHERE space_id = $1")
        .bind(space_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup space_summaries");

    sqlx::query("DELETE FROM space_children WHERE space_id = $1")
        .bind(space_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup space_children");

    sqlx::query("DELETE FROM space_members WHERE space_id = $1")
        .bind(space_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup space_members");

    sqlx::query("DELETE FROM spaces WHERE space_id = $1")
        .bind(space_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup spaces");

    for user_id in user_ids {
        sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup space user fixture");
    }
}

#[tokio::test]
async fn test_schema_contract_space_summary_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert!(
        has_unique_constraint_on(&pool, "space_members", &["space_id", "user_id"]).await,
        "Expected space_members UNIQUE(space_id,user_id)"
    );
    assert!(
        has_unique_constraint_on(&pool, "space_summaries", &["space_id"]).await,
        "Expected space_summaries UNIQUE(space_id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "space_events").await,
        vec!["event_id".to_string()],
        "Expected space_events PRIMARY KEY(event_id)"
    );
    assert_eq!(
        primary_key_columns(&pool, "space_statistics").await,
        vec!["space_id".to_string()],
        "Expected space_statistics PRIMARY KEY(space_id)"
    );

    assert_column(
        &pool,
        "space_members",
        "membership",
        &["text", "character varying"],
        false,
        Some("join"),
    )
    .await;
    assert_column(
        &pool,
        "space_summaries",
        "summary",
        &["jsonb"],
        true,
        Some("'{}'::jsonb"),
    )
    .await;
    assert_column(
        &pool,
        "space_summaries",
        "children_count",
        &["bigint"],
        true,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "space_summaries",
        "member_count",
        &["bigint"],
        true,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "space_statistics",
        "is_public",
        &["boolean"],
        false,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "space_statistics",
        "child_room_count",
        &["bigint"],
        true,
        Some("0"),
    )
    .await;
    assert_column(
        &pool,
        "space_statistics",
        "member_count",
        &["bigint"],
        true,
        Some("0"),
    )
    .await;

    for index_name in [
        "idx_space_members_space",
        "idx_space_members_user",
        "idx_space_members_membership",
        "idx_space_summary_space",
        "idx_space_events_space",
        "idx_space_events_space_type_ts",
        "idx_space_events_space_ts",
        "idx_space_statistics_member_count",
    ] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_space_summary_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = SpaceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let (creator, member) = seed_space_users(&pool, &suffix).await;
    let child_room_id = format!("!schema-space-child-{suffix}:localhost");

    let space = storage
        .create_space(CreateSpaceRequest {
            room_id: format!("!schema-space-root-{suffix}:localhost"),
            name: Some("Schema Space".to_string()),
            topic: Some("Space contract".to_string()),
            avatar_url: None,
            creator: creator.clone(),
            join_rule: Some("invite".to_string()),
            visibility: Some("private".to_string()),
            is_public: Some(false),
            parent_space_id: None,
        })
        .await
        .expect("Failed to create space contract fixture");

    storage
        .add_space_member(&space.space_id, &member, "join", Some(&creator))
        .await
        .expect("Failed to add second space member");
    storage
        .add_child(AddChildRequest {
            space_id: space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: creator.clone(),
            is_suggested: true,
            via_servers: vec!["localhost".to_string()],
        })
        .await
        .expect("Failed to add space child");
    storage
        .update_space_summary(&space.space_id)
        .await
        .expect("Failed to update space summary");

    let summary = storage
        .get_space_summary(&space.space_id)
        .await
        .expect("Failed to query space summary")
        .expect("Space summary should exist");
    assert_eq!(summary.space_id, space.space_id);
    assert_eq!(summary.children_count, 1);
    assert_eq!(summary.member_count, 2);

    sqlx::query(
        r#"
        INSERT INTO space_statistics (space_id, name, is_public, child_room_count, member_count, created_ts, updated_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (space_id) DO UPDATE SET
            name = EXCLUDED.name,
            is_public = EXCLUDED.is_public,
            child_room_count = EXCLUDED.child_room_count,
            member_count = EXCLUDED.member_count,
            updated_ts = EXCLUDED.updated_ts
        "#,
    )
    .bind(&space.space_id)
    .bind("Schema Space")
    .bind(false)
    .bind(1_i64)
    .bind(2_i64)
    .bind(0_i64)
    .bind(100_i64)
    .execute(&*pool)
    .await
    .expect("Failed to upsert space_statistics fixture");

    let statistics = storage
        .get_space_statistics()
        .await
        .expect("Failed to query space statistics");
    let statistics_row = statistics
        .iter()
        .find(|row| row["space_id"] == space.space_id)
        .expect("Expected space statistics row");
    assert_eq!(statistics_row["child_room_count"], 1);
    assert_eq!(statistics_row["member_count"], 2);

    let event_id_old = format!("$space-event-old-{suffix}");
    let event_id_new = format!("$space-event-new-{suffix}");
    sqlx::query(
        r#"
        INSERT INTO space_events (event_id, space_id, event_type, sender, content, state_key, origin_server_ts, processed_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NULL), ($8, $2, $3, $4, $9, $10, $11, NULL)
        "#,
    )
    .bind(&event_id_old)
    .bind(&space.space_id)
    .bind("m.space.child")
    .bind(&creator)
    .bind(serde_json::json!({"room_id": child_room_id, "via": ["localhost"]}))
    .bind(Some(child_room_id.as_str()))
    .bind(100_i64)
    .bind(&event_id_new)
    .bind(serde_json::json!({"room_id": "!schema-space-child-new", "via": ["localhost"]}))
    .bind(Some("!schema-space-child-new"))
    .bind(200_i64)
    .execute(&*pool)
    .await
    .expect("Failed to seed space_events fixtures");

    let events = storage
        .get_space_events(&space.space_id, Some("m.space.child"), 10)
        .await
        .expect("Failed to query filtered space events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_id, event_id_new);
    assert_eq!(events[1].event_id, event_id_old);

    storage
        .remove_space_member(&space.space_id, &member)
        .await
        .expect("Failed to remove joined space member");
    storage
        .remove_child(&space.space_id, &child_room_id)
        .await
        .expect("Failed to remove space child");
    storage
        .update_space_summary(&space.space_id)
        .await
        .expect("Failed to refresh space summary after removals");

    let summary_after_update = storage
        .get_space_summary(&space.space_id)
        .await
        .expect("Failed to reload space summary")
        .expect("Space summary should still exist");
    assert_eq!(summary_after_update.children_count, 0);
    assert_eq!(summary_after_update.member_count, 1);

    cleanup_space_fixtures(&pool, &space.space_id, &[creator, member]).await;
}

#[tokio::test]
async fn test_schema_contract_space_children_and_hierarchy_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert!(
        has_unique_constraint_on(&pool, "space_children", &["space_id", "room_id"]).await,
        "Expected space_children UNIQUE(space_id,room_id)"
    );
    assert_column(
        &pool,
        "space_children",
        "is_suggested",
        &["boolean"],
        true,
        Some("false"),
    )
    .await;
    assert_column(
        &pool,
        "space_children",
        "via_servers",
        &["jsonb"],
        true,
        Some("'[]'::jsonb"),
    )
    .await;
    assert_column(
        &pool,
        "space_children",
        "added_ts",
        &["bigint"],
        false,
        None,
    )
    .await;

    for index_name in ["idx_space_children_space", "idx_space_children_room"] {
        assert!(
            has_index_named(&pool, index_name).await,
            "Expected index {} to exist",
            index_name
        );
    }
}

#[tokio::test]
async fn test_schema_contract_space_children_and_hierarchy_query_and_write_read_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = SpaceStorage::new(&pool);
    let root_suffix = uuid::Uuid::new_v4().to_string();
    let child_suffix = format!("{root_suffix}-nested");
    let (root_creator, root_member) = seed_space_users(&pool, &root_suffix).await;
    let (child_creator, child_member) = seed_space_users(&pool, &child_suffix).await;
    let leaf_room_id = format!("!schema-space-leaf-{root_suffix}:localhost");

    let root_space = storage
        .create_space(CreateSpaceRequest {
            room_id: format!("!schema-space-root-room-{root_suffix}:localhost"),
            name: Some("Root Space".to_string()),
            topic: Some("Hierarchy root".to_string()),
            avatar_url: None,
            creator: root_creator.clone(),
            join_rule: Some("invite".to_string()),
            visibility: Some("private".to_string()),
            is_public: Some(false),
            parent_space_id: None,
        })
        .await
        .expect("Failed to create root space fixture");
    let child_space = storage
        .create_space(CreateSpaceRequest {
            room_id: format!("!schema-space-child-room-{root_suffix}:localhost"),
            name: Some("Nested Space".to_string()),
            topic: Some("Hierarchy child".to_string()),
            avatar_url: None,
            creator: child_creator.clone(),
            join_rule: Some("public".to_string()),
            visibility: Some("public".to_string()),
            is_public: Some(true),
            parent_space_id: Some(root_space.space_id.clone()),
        })
        .await
        .expect("Failed to create child space fixture");

    storage
        .add_space_member(
            &root_space.space_id,
            &root_member,
            "join",
            Some(&root_creator),
        )
        .await
        .expect("Failed to add root space member");
    storage
        .add_space_member(
            &child_space.space_id,
            &child_member,
            "join",
            Some(&child_creator),
        )
        .await
        .expect("Failed to add child space member");

    storage
        .add_child(AddChildRequest {
            space_id: root_space.space_id.clone(),
            room_id: child_space.room_id.clone(),
            sender: root_creator.clone(),
            is_suggested: true,
            via_servers: vec!["localhost".to_string()],
        })
        .await
        .expect("Failed to add child space relation");

    storage
        .add_child(AddChildRequest {
            space_id: child_space.space_id.clone(),
            room_id: leaf_room_id.clone(),
            sender: child_creator.clone(),
            is_suggested: false,
            via_servers: vec!["localhost".to_string(), "example.com".to_string()],
        })
        .await
        .expect("Failed to add nested leaf relation");

    let root_children = storage
        .get_space_children(&root_space.space_id)
        .await
        .expect("Failed to query root space children");
    assert_eq!(root_children.len(), 1);
    assert_eq!(root_children[0].room_id, child_space.room_id);

    let nested_children = storage
        .get_child_spaces(&leaf_room_id)
        .await
        .expect("Failed to query nested child spaces");
    assert_eq!(nested_children.len(), 1);
    assert_eq!(nested_children[0].room_id, leaf_room_id);
    assert_eq!(
        nested_children[0].via_servers,
        vec!["localhost".to_string(), "example.com".to_string()]
    );

    let recursive = storage
        .get_recursive_hierarchy(&root_space.space_id, 3, false)
        .await
        .expect("Failed to query recursive space hierarchy");
    assert_eq!(recursive.len(), 2);
    assert_eq!(recursive[0].room_id, child_space.room_id);
    assert!(recursive[0].is_space);
    assert_eq!(recursive[0].depth, 0);
    assert_eq!(recursive[1].room_id, leaf_room_id);
    assert!(!recursive[1].is_space);
    assert_eq!(recursive[1].depth, 1);

    let paginated = storage
        .get_space_hierarchy_paginated(&root_space.space_id, 3, false, Some(1), None)
        .await
        .expect("Failed to query paginated space hierarchy");
    assert_eq!(paginated.rooms.len(), 1);
    assert_eq!(paginated.rooms[0].room_id, child_space.room_id);
    assert_eq!(paginated.rooms[0].room_type.as_deref(), Some("m.space"));
    assert_eq!(paginated.rooms[0].join_rule, "public");
    assert!(paginated.rooms[0].world_readable);
    assert!(paginated.rooms[0].guest_can_join);
    assert_eq!(paginated.rooms[0].num_joined_members, 2);
    assert_eq!(paginated.next_batch.as_deref(), Some(leaf_room_id.as_str()));
    assert_eq!(paginated.rooms[0].children_state.len(), 1);
    assert_eq!(
        paginated.rooms[0].children_state[0]["state_key"],
        serde_json::json!(leaf_room_id)
    );
    assert_eq!(
        paginated.rooms[0].children_state[0]["content"]["suggested"],
        serde_json::json!(false)
    );

    cleanup_space_fixtures(&pool, &root_space.space_id, &[root_creator, root_member]).await;
    cleanup_space_fixtures(&pool, &child_space.space_id, &[child_creator, child_member]).await;
}
