# M3-ISSUE-3: E2EE 多表 nullable 性审计

**Status**: 🟡 open
**Severity**: 中
**Discovered**: M-3 阶段 D (2026-06-06)
**Origin**: ssss schema-drift 修复暴露同类问题
**Blocks**: 不阻塞 M-3 Batch 1（阶段 D 已为 ssss 模块引入包装 struct 绕过）

---

## 1. 背景

M-3 阶段 D 期间迁移 ssss 模块时，发现最严重的 schema drift：

- `e2ee_secret_storage_keys` 表：`key_name TEXT NOT NULL` / `key_data BYTEA NOT NULL`
- `e2ee_stored_secrets` 表：`secret_data BYTEA NOT NULL` / `key_key_id TEXT NOT NULL`
- Rust 模型 `SecretStorageKey` / `StoredSecret` 完全没有这些字段
- 原 SQL 缺少上述必填列，从未实际运行过（任何运行都会因 NOT NULL 失败）

阶段 D 决策：为 ssss 引入 `SecretStorageKeyRow` / `StoredSecretRow` 包装 struct，吸收 schema 必需列，业务模型不变。

**但 ssss 不是唯一案例**。E2EE 其他表可能存在类似 drift，阶段 D 通过引入包装 struct 暂时绕过，未做全表审计。

## 2. 待审计表

| 表 | 已知问题 | 阶段 D 现状 |
|----|----------|-------------|
| `e2ee_secret_storage_keys` | `key_name` / `key_data` 必填 | ✅ 已包装 |
| `e2ee_stored_secrets` | `secret_data` / `key_key_id` 必填 | ✅ 已包装 |
| `cross_signing_keys` | `key_data` 类型 / nullable 性 | ⚠️ 未审计 |
| `device_signatures` | `signature` 字段 nullable 性 | ⚠️ 未审计 |
| `device_keys` | `added_ts` / `created_ts` / `updated_ts` 关系 | ⚠️ 未审计 |
| `olm_sessions` | `message_index` 类型（i32 vs u32） | ✅ 已处理（包装 struct 中转换） |
| `megolm_sessions` | `epoch_num` 类型 / `pickle_format` enum | ✅ 已处理（包装 struct 中转换） |
| `secure_key_backups` | `key_count` BIGINT | ✅ 已处理（query_scalar!） |
| `secure_backup_session_keys` | (待审) | ⚠️ 未审计 |

## 3. 审计方法

```bash
# Step 1: 提取所有 E2EE 表的 schema
for table in cross_signing_keys device_signatures device_keys olm_sessions \
            megolm_sessions secure_key_backups secure_backup_session_keys; do
  echo "=== $table ==="
  psql $DATABASE_URL -c "\d $table"
done

# Step 2: 对比 Rust struct 字段
for f in src/e2ee/*/models.rs; do
  echo "=== $f ==="
  cat "$f"
done

# Step 3: 列出 mismatch
diff <(psql ... 列) <(cat src/.../models.rs 字段)
```

## 4. 已知/疑似 Drift 详情

### 4.1 `cross_signing_keys`

- DB schema 假设（待 v8 验证）：`key_data TEXT`（允许 NULL？）
- Rust struct `CrossSigningKey.key_data: String`（非空）
- **风险**：读取时如果 DB 有 NULL，会 panic
- **现状**：阶段 D 用 `key_data AS "key_data!"` 强制非空，可能在 NULL 数据上 panic

### 4.2 `device_signatures`

- DB schema 假设：`signature TEXT`（允许 NULL？）
- Rust struct `DeviceSignature.signature: String`（非空）
- **风险**：同上

### 4.3 `device_keys`

- DB schema：`added_ts BIGINT NOT NULL` / `created_ts BIGINT NULL` / `updated_ts BIGINT NULL`
- 阶段 D 包装 struct 用 `as "created_ts!"` / `as "updated_ts?"` 标注，**已处理**

## 5. 修复方案（待决策）

| 方向 | 适用场景 | 优点 | 缺点 |
|------|----------|------|------|
| A. Rust struct 改 `Option<>` | nullable 性不一致 | 向后兼容老数据 | 业务代码需 None 处理 |
| B. DB schema 加 `NOT NULL` | 老数据已保证非空 | 类型严格 | 老数据需回填 |
| C. 包装 struct 吸收 | 仅查询路径 | 业务模型不变 | 引入额外 struct |

**推荐**：方向 C（包装 struct）为主，方向 A 为辅。

## 6. 验收

- [ ] 全 E2EE 表 nullable 性审计完成
- [ ] 每个 drift 项有「包装 struct / 改 struct / 改 schema」三选一决策
- [ ] 决策后批量修复
- [ ] `cargo test --lib e2ee` 全部通过
- [ ] 无 panic 风险（用 `as "field?"` 处理可空字段）

## 7. 工时估计

| 工作量 | 时间 |
|--------|------|
| 全表审计 + 决策 | 0.3 天 |
| 批量修复 | 0.3 天 |
| 测试 + 验证 | 0.2 天 |
| **总计** | **0.8 天** |
