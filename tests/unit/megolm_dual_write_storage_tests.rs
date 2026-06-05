//! Phase 2: Megolm 双写（vodozemac ↔ legacy）存储层集成测试
//!
//! 覆盖 `MegolmSessionStorage` 的 Phase 2 新增方法：
//! - `create_session`（pickle_format / vodozemac_pickle 双列写入）
//! - `update_vodozemac_pickle`（encrypt/decrypt 后持久化最新 ratchet 状态）
//! - `promote_to_dual`（legacy → dual 升级，幂等）
//! - `list_legacy_sessions`（懒迁移分页扫描）
//! - `count_by_pickle_format`（监控 / 迁移进度）
//!
//! 这些测试不依赖 SQL schema 的 `megolm_sessions_keys`（Phase 2 暂未变更），
//! 只覆盖 `megolm_sessions` 表 + 新增列。

#![cfg(test)]

use std::sync::Arc;

use chrono::{Duration, Utc};
use sqlx::{Pool, Postgres};
use tokio::runtime::Runtime;
use uuid::Uuid;

use synapse_rust::e2ee::megolm::models::{MegolmSession, PickleFormat};
use synapse_rust::e2ee::megolm::storage::MegolmSessionStorage;

async fn setup_test_database() -> Arc<Pool<Postgres>> {
    let pool = synapse_rust::test_utils::prepare_empty_isolated_test_pool()
        .await
        .expect("Failed to prepare test pool");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS megolm_sessions (
            id UUID DEFAULT gen_random_uuid(),
            session_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            sender_key TEXT NOT NULL,
            session_key TEXT NOT NULL,
            algorithm TEXT NOT NULL,
            message_index BIGINT DEFAULT 0,
            created_ts BIGINT NOT NULL,
            last_used_ts BIGINT,
            expires_at BIGINT,
            pickle_format TEXT NOT NULL DEFAULT 'legacy',
            vodozemac_pickle TEXT,
            CONSTRAINT pk_megolm_sessions PRIMARY KEY (id),
            CONSTRAINT uq_megolm_sessions_session UNIQUE (session_id),
            CONSTRAINT chk_megolm_sessions_pickle_format CHECK (
                pickle_format IN ('legacy', 'vodozemac', 'dual')
            )
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create megolm_sessions table");

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_megolm_sessions_pickle_format_legacy
            ON megolm_sessions(pickle_format)
            WHERE pickle_format = 'legacy'
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create legacy partial index");

    pool
}

fn make_legacy_session(room_id: &str) -> MegolmSession {
    MegolmSession {
        id: Uuid::new_v4(),
        session_id: format!("legacy_{}_{}", room_id, Uuid::new_v4()),
        room_id: room_id.to_string(),
        sender_key: "sender_key_legacy".to_string(),
        session_key: "legacy_aes_gcm_encrypted_key".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index: 0,
        created_ts: Utc::now(),
        last_used_ts: Utc::now(),
        expires_at: Some(Utc::now() + Duration::days(7)),
        pickle_format: PickleFormat::Legacy,
        vodozemac_pickle: None,
    }
}

fn make_vodozemac_session(room_id: &str) -> MegolmSession {
    let mut s = make_legacy_session(room_id);
    s.pickle_format = PickleFormat::Vodozemac;
    s.vodozemac_pickle = Some("base64_vodozemac_pickle".to_string());
    s.session_key = "vodozemac_pickle_serialized".to_string();
    s
}

fn make_dual_session(room_id: &str) -> MegolmSession {
    let mut s = make_legacy_session(room_id);
    s.pickle_format = PickleFormat::Dual;
    s.vodozemac_pickle = Some("base64_vodozemac_pickle".to_string());
    s
}

/// 双写场景：E2EE_DUAL_WRITE=true 风格 — create_session 同时写入 legacy 加密 + vodozemac pickle
#[test]
fn test_create_session_writes_dual_pickle_columns() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let session = make_dual_session("!room:example.com");
        storage.create_session(&session).await.expect("create_session should succeed");

        let fetched = storage.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(fetched.pickle_format, PickleFormat::Dual);
        assert_eq!(fetched.vodozemac_pickle.as_deref(), Some("base64_vodozemac_pickle"));
        assert_eq!(fetched.session_key, session.session_key, "legacy session_key 列仍保留");
    });
}

/// vodozemac 路径（无双写）：create_session 写入 PickleFormat::Vodozemac + 副本列非空
#[test]
fn test_create_session_vodozemac_only_path() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let session = make_vodozemac_session("!room:example.com");
        storage.create_session(&session).await.unwrap();

        let fetched = storage.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(fetched.pickle_format, PickleFormat::Vodozemac);
        assert_eq!(fetched.vodozemac_pickle.as_deref(), Some("base64_vodozemac_pickle"));
    });
}

/// update_vodozemac_pickle：encrypt 后必须能持久化最新 ratchet state
#[test]
fn test_update_vodozemac_pickle_persists_new_ratchet() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let session = make_vodozemac_session("!room:example.com");
        storage.create_session(&session).await.unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let updated = storage
            .update_vodozemac_pickle(&session.session_id, "new_ratchet_state_v2", now_ms)
            .await
            .unwrap();
        assert!(updated, "update_vodozemac_pickle should report at least one row affected");

        let fetched = storage.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(fetched.vodozemac_pickle.as_deref(), Some("new_ratchet_state_v2"));
        assert_eq!(fetched.last_used_ts.timestamp_millis(), now_ms);
    });
}

/// update_vodozemac_pickle 对不存在的 session 返回 false（不应 panic 或视为错误）
#[test]
fn test_update_vodozemac_pickle_no_match_returns_false() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let now_ms = Utc::now().timestamp_millis();
        let updated = storage
            .update_vodozemac_pickle("nonexistent_session", "any", now_ms)
            .await
            .unwrap();
        assert!(!updated, "no rows should be updated for unknown session_id");
    });
}

/// promote_to_dual：legacy → dual 必须正确升级，且 vodozemac_pickle 必须写入
#[test]
fn test_promote_legacy_to_dual_succeeds() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let session = make_legacy_session("!room:example.com");
        storage.create_session(&session).await.unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let promoted = storage
            .promote_to_dual(&session.session_id, "promoted_vodozemac_pickle", now_ms)
            .await
            .unwrap();
        assert!(promoted, "promote_to_dual should affect 1 row");

        let fetched = storage.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(fetched.pickle_format, PickleFormat::Dual);
        assert_eq!(fetched.vodozemac_pickle.as_deref(), Some("promoted_vodozemac_pickle"));
        // 原始 legacy session_key 列保留（双写不破坏 legacy 读取）
        assert_eq!(fetched.session_key, "legacy_aes_gcm_encrypted_key");
    });
}

/// promote_to_dual 幂等性：第二次调用不应再写（条件不满足）
#[test]
fn test_promote_to_dual_is_idempotent() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let session = make_legacy_session("!room:example.com");
        storage.create_session(&session).await.unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let first = storage
            .promote_to_dual(&session.session_id, "first_pickle", now_ms)
            .await
            .unwrap();
        assert!(first, "first promote should succeed");

        let second = storage
            .promote_to_dual(&session.session_id, "second_pickle", now_ms + 1000)
            .await
            .unwrap();
        assert!(!second, "second promote should be a no-op (already dual)");

        let fetched = storage.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(fetched.vodozemac_pickle.as_deref(), Some("first_pickle"));
    });
}

/// promote_to_dual 对非 legacy 行不生效（dual / vodozemac 行不应被改动）
#[test]
fn test_promote_to_dual_skips_non_legacy_rows() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        let session = make_dual_session("!room:example.com");
        storage.create_session(&session).await.unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let promoted = storage
            .promote_to_dual(&session.session_id, "should_not_apply", now_ms)
            .await
            .unwrap();
        assert!(!promoted, "promote_to_dual must not touch dual/vodozemac rows");

        let fetched = storage.get_session(&session.session_id).await.unwrap().unwrap();
        assert_eq!(fetched.pickle_format, PickleFormat::Dual);
        assert_eq!(fetched.vodozemac_pickle.as_deref(), Some("base64_vodozemac_pickle"));
    });
}

/// list_legacy_sessions：分页扫描 + 游标稳定
#[test]
fn test_list_legacy_sessions_pagination() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        // 插入 5 个 legacy + 2 个 vodozemac + 1 个 dual
        for i in 0..5 {
            let s = make_legacy_session(&format!("!room{}_legacy:example.com", i));
            storage.create_session(&s).await.unwrap();
        }
        for i in 0..2 {
            let s = make_vodozemac_session(&format!("!room{}_vod:example.com", i));
            storage.create_session(&s).await.unwrap();
        }
        let dual = make_dual_session("!dual:example.com");
        storage.create_session(&dual).await.unwrap();

        // 分页扫描：第一页 3 条
        let page1 = storage.list_legacy_sessions(None, 3).await.unwrap();
        assert_eq!(page1.len(), 3, "page1 应为 3 条 legacy 记录");
        assert!(page1.iter().all(|s| s.pickle_format == PickleFormat::Legacy));

        // 用 page1 末位的 session_id 当游标
        let cursor = page1.last().unwrap().session_id.clone();
        let page2 = storage.list_legacy_sessions(Some(&cursor), 3).await.unwrap();
        assert_eq!(page2.len(), 2, "剩余 2 条 legacy 应在 page2");
        assert!(page2.iter().all(|s| s.session_id > cursor));
        assert!(page2.iter().all(|s| s.pickle_format == PickleFormat::Legacy));

        // 合并两页去重校验：所有 session_id 互不重叠
        let all_legacy_ids: Vec<_> = page1.iter().chain(page2.iter()).map(|s| s.session_id.clone()).collect();
        let unique_count = all_legacy_ids.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, all_legacy_ids.len(), "分页结果应无重复");
    });
}

/// list_legacy_sessions 限制参数边界
#[test]
fn test_list_legacy_sessions_clamps_limit() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        for i in 0..3 {
            let s = make_legacy_session(&format!("!limit_room{}:example.com", i));
            storage.create_session(&s).await.unwrap();
        }

        // limit=0 应被 clamp 到 1
        let page = storage.list_legacy_sessions(None, 0).await.unwrap();
        assert_eq!(page.len(), 1);

        // limit=10000 应被 clamp 到 1000
        let page = storage.list_legacy_sessions(None, 10000).await.unwrap();
        assert_eq!(page.len(), 3, "即使 limit=1000 也应只返回实际存在的 3 条");
    });
}

/// count_by_pickle_format：监控 / 迁移进度统计
#[test]
fn test_count_by_pickle_format() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        for _ in 0..4 {
            storage.create_session(&make_legacy_session("!r1:example.com")).await.unwrap();
        }
        for _ in 0..2 {
            storage.create_session(&make_vodozemac_session("!r2:example.com")).await.unwrap();
        }
        storage.create_session(&make_dual_session("!r3:example.com")).await.unwrap();

        let counts = storage.count_by_pickle_format().await.unwrap();
        let map: std::collections::HashMap<_, _> = counts.into_iter().collect();
        assert_eq!(map.get("legacy").copied().unwrap_or(0), 4);
        assert_eq!(map.get("vodozemac").copied().unwrap_or(0), 2);
        assert_eq!(map.get("dual").copied().unwrap_or(0), 1);
    });
}

/// 完整懒迁移闭环：list → promote → count
#[test]
fn test_lazy_migration_end_to_end() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = MegolmSessionStorage::new(&pool);

        for i in 0..5 {
            let s = make_legacy_session(&format!("!lazy{}:example.com", i));
            storage.create_session(&s).await.unwrap();
        }

        // 阶段 1：扫描
        let batch = storage.list_legacy_sessions(None, 10).await.unwrap();
        assert_eq!(batch.len(), 5);

        // 阶段 2：批量 promote
        let now_ms = Utc::now().timestamp_millis();
        let mut promoted = 0;
        for s in &batch {
            if storage.promote_to_dual(&s.session_id, "lazy_vodozemac", now_ms).await.unwrap() {
                promoted += 1;
            }
        }
        assert_eq!(promoted, 5, "全部 5 条 legacy 升级为 dual");

        // 阶段 3：迁移后扫描应为空
        let after = storage.list_legacy_sessions(None, 10).await.unwrap();
        assert_eq!(after.len(), 0, "迁移后无 legacy 残留");

        // 阶段 4：count_by_pickle_format 验证全部归入 dual
        let counts = storage.count_by_pickle_format().await.unwrap();
        let map: std::collections::HashMap<_, _> = counts.into_iter().collect();
        assert_eq!(map.get("dual").copied().unwrap_or(0), 5);
        assert_eq!(map.get("legacy").copied().unwrap_or(0), 0);
    });
}
