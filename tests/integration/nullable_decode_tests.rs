//! Regression tests for latent NULL-decode bugs (OPT-013, audit 03 §7).
//!
//! Each test proves a `FromRow` struct can decode a NULL from a nullable
//! column WITHOUT seeding tables/FKs, by decoding a synthetic SELECT whose
//! column list matches the struct's non-`#[sqlx(skip)]` fields.

use super::TestContext;
use synapse_storage::module::{AccountValidity, ModuleExecutionLog};

#[tokio::test]
async fn account_validity_decodes_null_expiration_at() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let row: AccountValidity = sqlx::query_as(
        "SELECT $1::text AS user_id, NULL::bigint AS expiration_at, NULL::bigint AS last_check_at, \
         NULL::text AS renewal_token, true AS is_valid, 0::bigint AS created_ts, 0::bigint AS updated_ts",
    )
    .bind("@opt013_av:localhost")
    .fetch_one(&*ctx.pool)
    .await
    .expect("NULL expiration_at must decode to None, not error");
    assert_eq!(row.expiration_at, None);
}

#[tokio::test]
async fn module_execution_log_decodes_null_execution_time_ms() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let row: ModuleExecutionLog = sqlx::query_as(
        "SELECT 0::bigint AS id, ''::text AS module_name, ''::text AS module_type, \
         NULL::text AS event_id, NULL::text AS room_id, NULL::bigint AS execution_time_ms, \
         true AS is_success, NULL::text AS error_message, NULL::jsonb AS metadata, \
         0::bigint AS executed_ts",
    )
    .fetch_one(&*ctx.pool)
    .await
    .expect("NULL execution_time_ms must decode to None, not error");
    assert_eq!(row.execution_time_ms, None);
}
