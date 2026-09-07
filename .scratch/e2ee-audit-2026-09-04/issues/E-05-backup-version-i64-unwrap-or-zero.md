# E-05: 备份版本号解析在 KeyBackupStorage 中以 `i64::unwrap_or(0)` 兜底

## 严重等级

🟡 **Medium** — 版本号降级为 0，破坏多版本隔离与查询语义

## 涉及代码

- `synapse-e2ee/src/backup/storage.rs:get_backup_version`（行 ~140）
- 涉及表：`key_backups.version`（TEXT 类型，但 `create_backup` 用 `chrono::Utc::now().timestamp()` 即 i64 字符串）

## 问题描述

`KeyBackupStorage::get_backup_version` 在解析客户端传入的 version 字符串时，使用：

```rust
let version_num: i64 = version.parse().unwrap_or(0);
// ...
sqlx::query_as!(...,
    r#"
    SELECT ...
    FROM key_backups
    WHERE user_id = $1
      AND (CAST(version AS BIGINT) = $2 OR version = $2)
    "#,
    user_id,
    version_num,
)
```

`unwrap_or(0)` 的问题：
1. **降级到 0**：若客户端传 UUID 格式（如 `e8b3a1c2-...`），`parse::<i64>()` 失败 → `unwrap_or(0)` → 后续 SQL 用 `version = 0` 查询，但同时 `version = $2`（原始字符串）也会匹配。也就是说，**逻辑上**是"UUID 也能查到"（通过第二个 OR 条件），但 `unwrap_or(0)` 本身是危险反模式。

2. **隐式 i64 上限**：i64 最大值约 9.2e18，而 `chrono::Utc::now().timestamp()` 截至 2030 年约 1.9e9，远低于上限。但若版本号由外部系统生成（如基于时间戳 + 自定义格式），`i64` 截断可能引入碰撞。

3. **与 `get_keys_for_version` 的 SQL 模式一致**：
   ```sql
   WHERE backup_id_text = $2 OR version::text = $2
   ```
   `get_keys_for_version` 已经接受字符串 `version`（不做 parse），证明 `version` 本质上是字符串。`get_backup_version` 的 `unwrap_or(0)` 是历史遗留。

## 实际影响

- **多版本场景**：用户可同时有 `version=1234567890`（i64）和 `version=2026-09-04-snapshot`（非 i64 字符串），后者会被 CAST 截断失败后通过 `version = $2` 查。
- **i64 解析失败时返回 0 → 误匹配**：
  - 如果用户 A 有一个真实的 `version=0`（理论上不会，因为 timestamp 总是 > 0），攻击者构造 `version=abc` 可命中。
  - 实际更可能：负数版本号 `-1` → `unwrap_or(0)` → CAST 失败 → 第二个 OR 条件匹配。

## 修复建议

**最简单修复**：去掉 `unwrap_or(0)`，用 `Option<i64>` 区分"纯数字版本"和"字符串版本"：

```rust
// backup/storage.rs
let version_num: Option<i64> = version.parse().ok();

let row = if let Some(n) = version_num {
    sqlx::query_as!(KeyBackupRow,
        r#"
        SELECT ...
        FROM key_backups
        WHERE user_id = $1
          AND (CAST(version AS BIGINT) = $2 OR version = $3)
        "#,
        user_id,
        n,
        version,
    )
    .fetch_optional(&self.pool)
    .await?
} else {
    sqlx::query_as!(KeyBackupRow,
        r#"
        SELECT ...
        FROM key_backups
        WHERE user_id = $1 AND version = $2
        "#,
        user_id,
        version,
    )
    .fetch_optional(&self.pool)
    .await?
};
```

**更深层修复**：将 `version` 字段在 schema 中明确为 `TEXT` 主键（已有），并删除 SQL 中的 `CAST(version AS BIGINT)` 路径，所有版本号都走字符串比较。

## 与规范的契合度

Matrix 规范中备份 `version` 是任意字符串（推荐时间戳字符串，但允许 UUID、hash 等）。当前实现的混合解析体现了历史技术债。

## 测试覆盖

现有测试 `test_get_backup_version_uuid` 验证了 UUID 格式版本可正确获取。但未测试：
- 负数版本号（`-1`）→ 期望返回 `None` 或 4xx。
- 字母数字混合版本号（`v1.2.3`）→ 期望字符串匹配。
- 超大 i64（`99999999999999999999`）→ 期望 `parse` 失败后走字符串匹配，不应 unwrap_or(0) 误匹配。
