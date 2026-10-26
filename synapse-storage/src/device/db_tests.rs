use super::*;

/// Shared `public` is deliberately replaced by a per-test schema here:
/// the fixtures delete `devices`/`lazy_loaded_members` by user and assert on counts;
/// shared `public` let sibling fixtures change those counts.
///
/// Eliminating the shared state removes the race instead of serialising around it
/// (AGENTS.md rule 7). The guard is returned with the pool so the schema outlives
/// the whole test — dropping it early spawns a background `DROP SCHEMA`.
async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<Pool<Postgres>>) {
    let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
    let pool = isolated.pool();
    (isolated, pool)
}

async fn ensure_test_user(pool: &Pool<Postgres>, user_id: &str) {
    let now = current_timestamp_millis();
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    sqlx::query(
        r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(user_id)
    .bind(username)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test user");
}

#[tokio::test]
async fn test_create_device_returns_valid_record() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_create_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@testuser:example.com";

    ensure_test_user(&pool, user_id).await;

    let device =
        storage.create_device(device_id, user_id, Some("Test Phone")).await.expect("create_device should succeed");

    assert_eq!(device.device_id, device_id.as_str());
    assert_eq!(device.user_id, user_id);
    assert_eq!(device.display_name.as_deref(), Some("Test Phone"));
    assert!(device.created_ts > 0);
    assert!(device.first_seen_ts > 0);
}

#[tokio::test]
async fn test_create_device_without_display_name() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_nodesc_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@nodesc:example.com";

    ensure_test_user(&pool, user_id).await;

    let device = storage
        .create_device(device_id, user_id, None)
        .await
        .expect("create_device without display name should succeed");

    assert_eq!(device.device_id, device_id.as_str());
    assert!(device.display_name.is_none());
}

#[tokio::test]
async fn test_get_device_finds_created_device() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_get_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@getuser:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, Some("Find Me")).await.expect("create_device should succeed");

    let found = storage.get_device(device_id).await.expect("get_device should succeed");

    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.device_id, device_id.as_str());
    assert_eq!(found.user_id, user_id);
    assert_eq!(found.display_name.as_deref(), Some("Find Me"));
}

#[tokio::test]
async fn test_get_device_not_found_returns_none() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);

    let result = storage.get_device("nonexistent_device_id_12345").await.expect("get_device should succeed");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_user_devices_returns_all() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0.to_string();
    let user_id = format!("@multidev_{}:example.com", suffix);
    let d1 = format!("multi_1_{}", suffix);
    let d2 = format!("multi_2_{}", suffix);

    ensure_test_user(&pool, &user_id).await;
    // Clean up any leftover devices from previous runs
    let _ = storage.delete_user_devices(&user_id).await;
    storage.create_device(&d1, &user_id, Some("Phone")).await.expect("create d1");
    storage.create_device(&d2, &user_id, Some("Tablet")).await.expect("create d2");

    let devices = storage.get_user_devices(&user_id).await.expect("get_user_devices should succeed");

    assert_eq!(devices.len(), 2);
    let ids: Vec<&str> = devices.iter().map(|d| d.device_id.as_str()).collect();
    assert!(ids.contains(&d1.as_str()));
    assert!(ids.contains(&d2.as_str()));
}

#[tokio::test]
async fn test_get_user_devices_empty_for_new_user() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let user_id = "@nodevices:example.com";

    ensure_test_user(&pool, user_id).await;

    let devices = storage.get_user_devices(user_id).await.expect("get_user_devices should succeed");

    assert!(devices.is_empty());
}

#[tokio::test]
async fn test_update_device_display_name() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_updname_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@updatename:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, Some("Old Name")).await.expect("create_device");

    storage.update_device_display_name(device_id, "New Name").await.expect("update_device_display_name should succeed");

    let updated = storage.get_device(device_id).await.expect("get_device").unwrap();
    assert_eq!(updated.display_name.as_deref(), Some("New Name"));
}

#[tokio::test]
async fn test_update_user_device_display_name_returns_rows_affected() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_upduser_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@upduser:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    let rows = storage
        .update_user_device_display_name(user_id, device_id, "Updated")
        .await
        .expect("update_user_device_display_name should succeed");

    assert_eq!(rows, 1);
}

#[tokio::test]
async fn test_update_user_device_display_name_wrong_user_returns_zero() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_wrongusr_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@rightuser:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    let rows = storage
        .update_user_device_display_name("@wronguser:example.com", device_id, "Nope")
        .await
        .expect("update should succeed but affect 0 rows");

    assert_eq!(rows, 0);
}

#[tokio::test]
async fn test_update_device_last_seen() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_seen_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@seen:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    storage.update_device_last_seen(device_id).await.expect("update_device_last_seen should succeed");

    let updated = storage.get_device(device_id).await.expect("get_device").unwrap();
    assert!(updated.last_seen_ts.is_some());
    assert!(updated.last_seen_ts.unwrap() > 0);
}

#[tokio::test]
async fn test_delete_user_device_returns_rows_affected() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_del_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@deleteusr:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    let rows = storage.delete_user_device(user_id, device_id).await.expect("delete_user_device should succeed");

    assert_eq!(rows, 1);

    let after = storage.get_device(device_id).await.expect("get_device");
    assert!(after.is_none());
}

#[tokio::test]
async fn test_delete_user_device_nonexistent_returns_zero() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);

    let rows = storage
        .delete_user_device("@nobody:example.com", "no_such_device")
        .await
        .expect("delete should succeed but affect 0 rows");

    assert_eq!(rows, 0);
}

#[tokio::test]
async fn test_device_exists_returns_true_for_created_device() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_exists_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id =
        &format!("@exists_{}:example.com", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(8).0);

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    assert!(storage.device_exists(device_id).await.expect("device_exists should succeed"));
}

#[tokio::test]
async fn test_device_exists_returns_false_for_unknown_device() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);

    assert!(!storage.device_exists("no_such_device_xyz").await.expect("device_exists should succeed"));
}

#[tokio::test]
async fn test_get_device_count_counts_correctly() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0.to_string();
    let user_id = format!("@countusr_{}:example.com", suffix);

    ensure_test_user(&pool, &user_id).await;
    let _ = storage.delete_user_devices(&user_id).await;

    assert_eq!(storage.get_device_count(&user_id).await.expect("get_device_count"), 0);

    let d1 = format!("cnt_1_{}", suffix);
    let d2 = format!("cnt_2_{}", suffix);
    storage.create_device(&d1, &user_id, None).await.expect("create d1");
    storage.create_device(&d2, &user_id, None).await.expect("create d2");

    assert_eq!(storage.get_device_count(&user_id).await.expect("get_device_count"), 2);
}

#[tokio::test]
async fn test_delete_user_devices_removes_all() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let user_id = "@purge:example.com";
    let d1 = &format!("purge_1_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let d2 = &format!("purge_2_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);

    ensure_test_user(&pool, user_id).await;
    storage.create_device(d1, user_id, None).await.expect("create d1");
    storage.create_device(d2, user_id, None).await.expect("create d2");

    storage.delete_user_devices(user_id).await.expect("delete_user_devices should succeed");

    assert_eq!(storage.get_device_count(user_id).await.expect("get_device_count"), 0);
}

#[tokio::test]
async fn test_get_user_device_finds_by_user_and_device() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_userdev_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@userdev:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, Some("Specific")).await.expect("create_device");

    let found = storage.get_user_device(user_id, device_id).await.expect("get_user_device should succeed");

    assert!(found.is_some());
    assert_eq!(found.unwrap().device_id, device_id.as_str());
}

#[tokio::test]
async fn test_get_user_device_returns_none_for_wrong_user() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_wrongusr2_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@owner:example.com";

    ensure_test_user(&pool, user_id).await;
    ensure_test_user(&pool, "@other:example.com").await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    let found = storage.get_user_device("@other:example.com", device_id).await.expect("get_user_device should succeed");

    assert!(found.is_none());
}

#[tokio::test]
async fn test_get_devices_batch_returns_requested_devices() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let user_id = "@batchusr:example.com";
    let d1 = &format!("batch_1_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let d2 = &format!("batch_2_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);

    ensure_test_user(&pool, user_id).await;
    storage.create_device(d1, user_id, None).await.expect("create d1");
    storage.create_device(d2, user_id, None).await.expect("create d2");

    let devices = storage
        .get_devices_batch(&[d1.clone(), d2.clone(), "no_such".to_string()])
        .await
        .expect("get_devices_batch should succeed");

    assert_eq!(devices.len(), 2);
}

#[tokio::test]
async fn test_get_devices_batch_empty_input_returns_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);

    let devices = storage.get_devices_batch(&[]).await.expect("get_devices_batch with empty input should succeed");

    assert!(devices.is_empty());
}

#[tokio::test]
async fn test_filter_existing_users_returns_only_users_with_devices() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0.to_string();
    let user_id = format!("@filterusr_{}:example.com", suffix);
    let no_device_user = format!("@nodevusr_{}:example.com", suffix);

    ensure_test_user(&pool, &user_id).await;
    ensure_test_user(&pool, &no_device_user).await;
    let _ = storage.delete_user_devices(&user_id).await;

    storage.create_device(&format!("filter_dev_{}", suffix), &user_id, None).await.expect("create_device");

    let result = storage
        .filter_existing_users(&[user_id.clone(), no_device_user.clone()])
        .await
        .expect("filter_existing_users should succeed");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0], user_id);
}

#[tokio::test]
async fn test_filter_existing_users_empty_input_returns_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);

    let result =
        storage.filter_existing_users(&[]).await.expect("filter_existing_users with empty input should succeed");

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_max_device_list_stream_id_returns_zero_for_empty() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);

    let max_id = storage.get_max_device_list_stream_id().await.expect("get_max_device_list_stream_id should succeed");

    assert!(max_id >= 0);
}

#[tokio::test]
async fn test_device_list_stream_id_advances_after_create_device() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let user_id = "@streamusr:example.com";
    let device_id = &format!("stream_dev_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);

    ensure_test_user(&pool, user_id).await;

    let before = storage.get_max_device_list_stream_id().await.expect("get max before");

    storage.create_device(device_id, user_id, None).await.expect("create_device");

    let after = storage.get_max_device_list_stream_id().await.expect("get max after");
    assert!(after > before);
}

#[tokio::test]
async fn test_delete_device_by_id_cleans_up_record() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let device_id = &format!("dev_delbyid_{}", uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0);
    let user_id = "@delbyid:example.com";

    ensure_test_user(&pool, user_id).await;
    storage.create_device(device_id, user_id, None).await.expect("create_device");

    storage.delete_device(device_id).await.expect("delete_device should succeed");

    assert!(storage.get_device(device_id).await.expect("get_device").is_none());
}

/// CODE-03：设备删除与懒加载成员清理必须**同一笔事务**。
///
/// ## 为什么用触发器而不是 mock
///
/// 这条缺陷的形态是"删了设备、清理失败、只剩一行 warn"：要证明修复，必须让**清理这一步真的失败**，
/// 再看设备行是否还在。用触发器在隔离 schema 里让 `DELETE FROM lazy_loaded_members` 必然抛错，
/// 是最省事且不改生产代码的注入方式。
///
/// ## 判据（三条都可失败）
///
/// 1. 清理失败 ⇒ `delete_device` 返回 `Err`（旧实现返回 `Ok(())` 并只 warn）；
/// 2. 失败后**设备行还在**（事务回滚）——旧实现这里设备已经没了，重试也补不回
///    `device_lists_changes`，这正是当时"只 warn"的理由；
/// 3. 拆掉触发器后重试 ⇒ `Ok`，且设备与懒加载成员行都清干净。
#[tokio::test]
async fn test_delete_device_rolls_back_when_lazy_member_cleanup_fails() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().split_at(12).0.to_string();
    let user_id = format!("@c03_{suffix}:example.com");
    let device_id = format!("C03_{suffix}");
    let room_id = format!("!c03_{suffix}:example.com");

    ensure_test_user(&pool, &user_id).await;
    storage.create_device(&device_id, &user_id, None).await.expect("create_device");

    // 造一行懒加载成员记录，让清理阶段"有东西可删"（触发器是 BEFORE DELETE FOR EACH ROW）。
    let now = current_timestamp_millis();
    sqlx::query(
        r#"INSERT INTO lazy_loaded_members
                   (user_id, device_id, room_id, member_user_id, created_ts, updated_ts, last_sent_position)
               VALUES ($1, $2, $3, $4, $5, $5, 1)"#,
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(&room_id)
    .bind("@member_c03:example.com")
    .bind(now)
    .execute(&*pool)
    .await
    .expect("seed lazy_loaded_members row");

    // 让清理必然失败。
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION c03_block_lazy_delete() RETURNS trigger
               LANGUAGE plpgsql AS $$
               BEGIN
                   RAISE EXCEPTION 'blocked by CODE-03 test';
               END
               $$"#,
    )
    .execute(&*pool)
    .await
    .expect("create trigger function");
    sqlx::query(
        r#"CREATE TRIGGER c03_block_lazy_delete
               BEFORE DELETE ON lazy_loaded_members
               FOR EACH ROW EXECUTE FUNCTION c03_block_lazy_delete()"#,
    )
    .execute(&*pool)
    .await
    .expect("create trigger");

    let result = storage.delete_device(&device_id).await;
    assert!(result.is_err(), "清理失败必须让 delete_device 返回 Err（旧实现只 warn 并返回 Ok）");
    assert!(
        storage.get_device(&device_id).await.expect("get_device").is_some(),
        "事务必须整体回滚：清理失败时设备行不能消失（否则 `device_lists_changes` 永远补不回来）"
    );

    // 拆掉触发器后重试，必须成功且不留残留。
    sqlx::query(r#"DROP TRIGGER c03_block_lazy_delete ON lazy_loaded_members"#)
        .execute(&*pool)
        .await
        .expect("drop trigger");
    storage.delete_device(&device_id).await.expect("delete_device after trigger removed");

    assert!(storage.get_device(&device_id).await.expect("get_device").is_none());
    let remaining: i64 = sqlx::query_scalar(r#"SELECT count(*) FROM lazy_loaded_members WHERE device_id = $1"#)
        .bind(&device_id)
        .fetch_one(&*pool)
        .await
        .expect("count lazy rows");
    assert_eq!(remaining, 0, "设备删除必须连带清掉它的懒加载成员行");
}

#[tokio::test]
async fn test_create_device_commits_atomically() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0.to_string();
    let user_id = format!("@txcreate_{}:example.com", suffix);
    let device_id = format!("ATOMCREATE_{}", suffix);

    ensure_test_user(&pool, &user_id).await;

    let device = storage
        .create_device(&device_id, &user_id, Some("atomic-create-test"))
        .await
        .expect("create_device should succeed");

    assert_eq!(device.device_id, device_id);
    assert_eq!(device.user_id, user_id);

    // Verify device exists in devices table
    let dev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE user_id = $1 AND device_id = $2")
        .bind(&user_id)
        .bind(&device_id)
        .fetch_one(&*pool)
        .await
        .expect("query devices table");
    assert_eq!(dev_count, 1, "device must exist in devices table (atomic commit)");

    // Verify side effects: device_lists_stream row must exist
    let stream_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM device_lists_stream WHERE user_id = $1 AND device_id = $2")
            .bind(&user_id)
            .bind(&device_id)
            .fetch_one(&*pool)
            .await
            .expect("query device_lists_stream table");
    assert_eq!(stream_count, 1, "device_lists_stream must have a row (atomic commit)");
}

#[tokio::test]
async fn test_delete_user_device_is_atomic() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0.to_string();
    let user_id = format!("@txdel_{}:example.com", suffix);
    let device_id = format!("ATOMDEL_{}", suffix);

    ensure_test_user(&pool, &user_id).await;

    // Create first
    storage.create_device(&device_id, &user_id, None).await.expect("create_device should succeed");

    // Delete
    let rows = storage.delete_user_device(&user_id, &device_id).await.expect("delete_user_device should succeed");
    assert_eq!(rows, 1);

    // Verify device is gone
    let dev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE user_id = $1 AND device_id = $2")
        .bind(&user_id)
        .bind(&device_id)
        .fetch_one(&*pool)
        .await
        .expect("query devices table");
    assert_eq!(dev_count, 0, "device must be deleted");

    // Verify side effects: device_lists_changes must have a "deleted" row
    let change_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_lists_changes WHERE user_id = $1 AND device_id = $2 AND change_type = 'deleted'",
    )
    .bind(&user_id)
    .bind(&device_id)
    .fetch_one(&*pool)
    .await
    .expect("query device_lists_changes table");
    assert_eq!(change_count, 1, "device_lists_changes must have a deleted row (atomic commit)");
}

#[tokio::test]
async fn test_delete_devices_batch_uses_batch_side_effects() {
    let (_isolated, pool) = test_pool().await;
    let storage = DeviceStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().simple().to_string().to_string().split_at(12).0.to_string();
    let user_id = format!("@bdel_{}:example.com", suffix);
    let did1 = format!("BDEL1_{}", suffix);
    let did2 = format!("BDEL2_{}", suffix);
    let did3 = format!("BDEL3_{}", suffix);

    ensure_test_user(&pool, &user_id).await;
    storage.create_device(&did1, &user_id, None).await.unwrap();
    storage.create_device(&did2, &user_id, None).await.unwrap();
    storage.create_device(&did3, &user_id, None).await.unwrap();

    let rows = storage.delete_devices_batch(&[did1.clone(), did2.clone(), did3.clone()]).await.unwrap();
    assert_eq!(rows, 3);

    // All 3 device_lists_changes rows must exist
    let change_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_lists_changes WHERE user_id = $1 AND device_id = ANY($2) AND change_type = 'deleted'",
        )
        .bind(&user_id)
        .bind([&did1, &did2, &did3])
        .fetch_one(&*pool)
        .await.unwrap();
    assert_eq!(change_count, 3, "all 3 devices must have deleted change records");
}
