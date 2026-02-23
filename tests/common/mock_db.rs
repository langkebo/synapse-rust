use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::sync::Once;
use std::time::Duration;

static INIT: Once = Once::new();

pub async fn get_test_pool() -> Option<Pool<Postgres>> {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    });

    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| {
            "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
        });

    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await
        .ok()
}

pub async fn setup_test_schema(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id VARCHAR(255) PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            displayname TEXT,
            avatar_url TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            deactivated BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            creation_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW()),
            update_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())
        )
    "#)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS devices (
            device_id VARCHAR(255) PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
            display_name TEXT,
            device_key JSONB,
            last_seen_ts BIGINT,
            last_seen_ip TEXT,
            created_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW()),
            first_seen_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW()),
            appservice_id TEXT,
            ignored_user_list TEXT
        )
    "#)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS rooms (
            room_id VARCHAR(255) PRIMARY KEY,
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            join_rule TEXT DEFAULT 'invite',
            creator VARCHAR(255) NOT NULL,
            version TEXT DEFAULT '6',
            encryption TEXT,
            is_public BOOLEAN DEFAULT FALSE,
            member_count BIGINT DEFAULT 0,
            history_visibility TEXT DEFAULT 'joined',
            creation_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW()),
            last_activity_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())
        )
    "#)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn cleanup_test_data(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM devices").execute(pool).await?;
    sqlx::query("DELETE FROM rooms").execute(pool).await?;
    sqlx::query("DELETE FROM users").execute(pool).await?;
    Ok(())
}

pub async fn create_test_user(
    pool: &Pool<Postgres>,
    user_id: &str,
    username: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(r#"
        INSERT INTO users (user_id, username, creation_ts, update_ts)
        VALUES ($1, $2, $3, $3)
        ON CONFLICT (user_id) DO NOTHING
    "#)
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}
