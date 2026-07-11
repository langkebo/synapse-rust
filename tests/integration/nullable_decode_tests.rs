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

#[tokio::test]
async fn user_privacy_settings_decodes_null_updated_ts() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let row: synapse_storage::privacy::UserPrivacySettings = sqlx::query_as(
        "SELECT 0::bigint AS id, ''::text AS user_id, ''::text AS profile_visibility, \
         ''::text AS avatar_visibility, ''::text AS displayname_visibility, \
         ''::text AS presence_visibility, ''::text AS room_membership_visibility, \
         0::bigint AS created_ts, NULL::bigint AS updated_ts",
    )
    .fetch_one(&*ctx.pool)
    .await
    .expect("NULL updated_ts must decode to None, not error");
    assert_eq!(row.updated_ts, None);
}

#[tokio::test]
async fn background_update_decodes_null_created_ts() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let row: synapse_storage::background_update::BackgroundUpdate = sqlx::query_as(
        "SELECT ''::text AS job_name, ''::text AS job_type, NULL::text AS description, \
         NULL::text AS table_name, NULL::text AS column_name, ''::text AS status, \
         '{}'::jsonb AS progress, 0::int AS total_items, 0::int AS processed_items, \
         NULL::bigint AS created_ts, NULL::bigint AS started_ts, NULL::bigint AS completed_ts, \
         NULL::bigint AS updated_ts, NULL::text AS error_message, 0::int AS retry_count, \
         0::int AS max_retries, 0::int AS batch_size, 0::int AS sleep_ms, \
         NULL::jsonb AS depends_on, NULL::jsonb AS metadata",
    )
    .fetch_one(&*ctx.pool)
    .await
    .expect("NULL created_ts must decode to None, not error");
    assert_eq!(row.created_ts, None);
}

#[tokio::test]
async fn captcha_template_decodes_null_updated_ts() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let row: synapse_storage::captcha::CaptchaTemplate = sqlx::query_as(
        "SELECT 0::bigint AS id, ''::text AS template_name, ''::text AS captcha_type, \
         NULL::text AS subject, ''::text AS content, '{}'::jsonb AS variables, \
         true AS is_default, true AS is_enabled, 0::bigint AS created_ts, \
         NULL::bigint AS updated_ts",
    )
    .fetch_one(&*ctx.pool)
    .await
    .expect("NULL updated_ts must decode to None, not error");
    assert_eq!(row.updated_ts, None);
}

#[tokio::test]
async fn application_service_transaction_decodes_null_sent_ts() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let row: synapse_storage::application_service::ApplicationServiceTransaction = sqlx::query_as(
        "SELECT 0::bigint AS id, ''::text AS as_id, ''::text AS txn_id, \
         NULL::text AS transaction_id, '{}'::jsonb AS events, NULL::bigint AS sent_ts, \
         NULL::bigint AS completed_ts, 0::int AS retry_count, NULL::text AS last_error",
    )
    .fetch_one(&*ctx.pool)
    .await
    .expect("NULL sent_ts must decode to None, not error");
    assert_eq!(row.sent_ts, None);
}
