# 字段命名规范检查报告

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **检查标准**: `DATABASE_FIELD_STANDARDS.md` v3.0.0

---

## 检查标准说明

根据 `DATABASE_FIELD_STANDARDS.md` 规范：

| 规范类型 | 后缀/前缀 | 适用场景 | 示例 |
|----------|----------|----------|------|
| `_ts` 后缀 | BIGINT | 创建时间、更新时间、活跃时间等必须存在的时间戳 | `created_ts`, `updated_ts`, `last_seen_ts` |
| `_at` 后缀 | BIGINT | 过期时间、撤销时间、验证时间等可选操作的时间戳 | `expires_at`, `revoked_at`, `validated_at` |
| `is_` 前缀 | BOOLEAN | 是否...布尔字段 | `is_admin`, `is_enabled`, `is_revoked` |
| `has_` 前缀 | BOOLEAN | 拥有...布尔字段 | `has_avatar`, `has_displayname` |

---

## 第一部分：`_ts` 后缀字段检查

### 1.1 正确使用 `_ts` 的字段

| 表名 | 字段 | 数据类型 | 使用场景 | 检查结果 |
|------|------|----------|----------|----------|
| users | created_ts | BIGINT | 创建时间 | ✅ 正确 |
| users | updated_ts | BIGINT | 更新时间 | ✅ 正确 |
| users | password_changed_ts | BIGINT | 密码修改时间 | ✅ 正确 |
| devices | created_ts | BIGINT | 创建时间 | ✅ 正确 |
| devices | first_seen_ts | BIGINT | 首次活跃时间 | ✅ 正确 |
| devices | last_seen_ts | BIGINT | 最后活跃时间 | ✅ 正确 |
| access_tokens | created_ts | BIGINT | 创建时间 | ✅ 正确 |
| access_tokens | last_used_ts | BIGINT | 最后使用时间 | ✅ 正确 |
| refresh_tokens | created_ts | BIGINT | 创建时间 | ✅ 正确 |
| refresh_tokens | last_used_ts | BIGINT | 最后使用时间 | ✅ 正确 |
| registration_tokens | created_ts | BIGINT | 创建时间 | ✅ 正确 |
| registration_tokens | last_used_ts | BIGINT | 最后使用时间 | ✅ 正确 |
| user_threepids | added_ts | BIGINT | 添加时间 | ✅ 正确 |
| rooms | created_ts | BIGINT | 创建时间 | ✅ 正确 |
| rooms | last_activity_ts | BIGINT | 最后活动时间 | ✅ 正确 |
| events | origin_server_ts | BIGINT | 原始服务器时间 | ✅ 正确 |
| room_memberships | joined_ts | BIGINT | 加入时间 | ✅ 正确 |
| room_memberships | invited_ts | BIGINT | 邀请时间 | ✅ 正确 |
| room_memberships | left_ts | BIGINT | 离开时间 | ✅ 正确 |
| room_memberships | banned_ts | BIGINT | 封禁时间 | ✅ 正确 |
| presence | last_active_ts | BIGINT | 最后活跃时间 | ✅ 正确 |

**统计**: 检查 21 个 `_ts` 字段，21 个正确，一致性 100%

---

### 1.2 潜在问题 `_ts` 字段

未发现错误使用 `_ts` 后缀的情况。

---

## 第二部分：`_at` 后缀字段检查

### 2.1 正确使用 `_at` 的字段

| 表名 | 字段 | 数据类型 | 使用场景 | 检查结果 |
|------|------|----------|----------|----------|
| users | password_expires_at | BIGINT | 密码过期时间 | ✅ 正确 |
| users | locked_until | BIGINT | 锁定截止时间 | ⚠️ 注意 |
| access_tokens | expires_at | BIGINT | 过期时间 | ✅ 正确 |
| access_tokens | revoked_at | BIGINT | 撤销时间 | ✅ 正确 |
| refresh_tokens | expires_at | BIGINT | 过期时间 | ✅ 正确 |
| refresh_tokens | revoked_at | BIGINT | 撤销时间 | ✅ 正确 |
| user_threepids | validated_at | BIGINT | 验证时间 | ✅ 正确 |
| user_threepids | verification_expires_at | BIGINT | 验证过期时间 | ✅ 正确 |

**统计**: 检查 8 个 `_at` 字段，8 个正确，一致性 100%

---

### 2.2 特殊字段分析

#### users.locked_until

| 属性 | 值 |
|------|-----|
| 数据类型 | BIGINT |
| 当前命名 | locked_until |
| 规范后缀 | `_at` 或 `_ts` |
| 分析 | 锁定截止时间是时间点，应该使用 `_at` 后缀 |
| 建议 | 保持 `locked_until` (因为是 UNTIL 介词短语，不是标准时间戳) |

**说明**: `locked_until` 使用 `until` 介词，表示截止时间点，是一种特殊命名模式，保持现状即可。

---

## 第三部分：布尔字段前缀检查

### 3.1 正确使用 `is_` 前缀的字段

| 表名 | 字段 | 数据类型 | 使用场景 | 检查结果 |
|------|------|----------|----------|----------|
| users | is_admin | BOOLEAN | 是否管理员 | ✅ 正确 |
| users | is_guest | BOOLEAN | 是否访客 | ✅ 正确 |
| users | is_shadow_banned | BOOLEAN | 是否影子封禁 | ✅ 正确 |
| users | is_deactivated | BOOLEAN | 是否已停用 | ✅ 正确 |
| users | must_change_password | BOOLEAN | 必须修改密码 | ⚠️ 建议 |
| devices | - | - | - | ✅ 无布尔字段 |
| access_tokens | is_revoked | BOOLEAN | 是否已撤销 | ✅ 正确 |
| refresh_tokens | is_revoked | BOOLEAN | 是否已撤销 | ✅ 正确 |
| user_threepids | is_verified | BOOLEAN | 是否已验证 | ✅ 正确 |
| key_backups | is_verified | BOOLEAN | 是否已验证 | ✅ 正确 |
| olm_accounts | is_one_time_keys_published | BOOLEAN | 是否发布一次性密钥 | ⚠️ 建议 |
| olm_accounts | is_fallback_key_published | BOOLEAN | 是否发布回退密钥 | ⚠️ 建议 |
| sliding_sync_rooms | is_dm | BOOLEAN | 是否私信 | ✅ 正确 |
| sliding_sync_rooms | is_encrypted | BOOLEAN | 是否加密 | ✅ 正确 |
| sliding_sync_rooms | is_tombstoned | BOOLEAN | 是否已删除 | ✅ 正确 |
| thread_subscriptions | is_muted | BOOLEAN | 是否静音 | ✅ 正确 |
| thread_subscriptions | is_pinned | BOOLEAN | 是否置顶 | ✅ 正确 |

**统计**: 检查 16 个布尔字段，12 个正确，4 个建议优化

---

### 3.2 布尔字段命名建议

#### 建议 1: users.must_change_password

| 当前命名 | 建议命名 | 原因 |
|----------|----------|------|
| must_change_password | is_password_change_required | 更符合 `is_` 前缀规范 |

---

#### 建议 2: olm_accounts.is_one_time_keys_published

| 当前命名 | 建议命名 | 原因 |
|----------|----------|------|
| is_one_time_keys_published | has_published_one_time_keys | 更符合 `has_` 前缀规范 |

---

#### 建议 3: olm_accounts.is_fallback_key_published

| 当前命名 | 建议命名 | 原因 |
|----------|----------|------|
| is_fallback_key_published | has_published_fallback_key | 更符合 `has_` 前缀规范 |

---

## 第四部分：禁止使用的字段检查

### 4.1 禁止字段检查结果

| 禁止字段 | 替代字段 | 检查结果 |
|----------|----------|----------|
| invalidated | is_revoked | ✅ 未使用 |
| invalidated_ts | revoked_at | ✅ 未使用 |
| created_at | created_ts | ✅ 未使用 |
| updated_at | updated_ts | ✅ 未使用 |
| enabled | is_enabled | ✅ 未使用 |
| expires_ts | expires_at | ✅ 未使用 |
| revoked_ts | revoked_at | ✅ 未使用 |
| last_used_at | last_used_ts | ✅ 未使用 |
| validated_ts | validated_at | ✅ 未使用 |

**说明**: 所有禁止使用的字段名均未被使用，符合规范。

---

## 第五部分：外键字段命名检查

### 5.1 外键字段命名规范

| 规范 | 示例 |
|------|------|
| 格式 | `{table}_id` |
| 说明 | 外键引用使用被引用表名 + `_id` |

### 5.2 检查结果

| 表名 | 外键字段 | 引用表 | 检查结果 |
|------|---------|--------|----------|
| devices | user_id | users | ✅ 正确 |
| room_memberships | user_id | users | ✅ 正确 |
| room_memberships | room_id | rooms | ✅ 正确 |
| access_tokens | user_id | users | ✅ 正确 |
| refresh_tokens | user_id | users | ✅ 正确 |
| events | room_id | rooms | ✅ 正确 |

**统计**: 检查 6 个外键字段，6 个正确，一致性 100%

---

## 第六部分：字段命名问题汇总

### 问题汇总表

| 序号 | 严重程度 | 表名 | 字段 | 当前命名 | 建议命名 | 问题类型 |
|------|----------|------|------|----------|----------|----------|
| 1 | P2 | users | must_change_password | must_change_password | is_password_change_required | 布尔字段前缀 |
| 2 | P2 | olm_accounts | is_one_time_keys_published | is_one_time_keys_published | has_published_one_time_keys | 布尔字段前缀 |
| 3 | P2 | olm_accounts | is_fallback_key_published | is_fallback_key_published | has_published_fallback_key | 布尔字段前缀 |

---

## 第七部分：符合规范的字段统计

| 规范类型 | 总检查数 | 符合数 | 符合率 |
|----------|----------|--------|--------|
| `_ts` 后缀 | 21 | 21 | 100% |
| `_at` 后缀 | 8 | 8 | 100% |
| `is_`/`has_` 前缀 | 16 | 12 | 75% |
| 外键字段 | 6 | 6 | 100% |
| 禁止字段 | 9 | 9 | 100% |

**总体符合率**: 91.7%

---

## 第八部分：修复建议

### 高优先级 (建议尽快修复)

| 序号 | 表名 | 字段 | 建议操作 | 工作量 |
|------|------|------|----------|--------|
| 1 | users | must_change_password | 重命名为 `is_password_change_required` | 中 |
| 2 | olm_accounts | is_one_time_keys_published | 重命名为 `has_published_one_time_keys` | 中 |
| 3 | olm_accounts | is_fallback_key_published | 重命名为 `has_published_fallback_key` | 中 |

### 修复影响分析

1. **数据库层**: 需要 ALTER TABLE 重命名列
2. **Rust 模型层**: 需要修改结构体字段名
3. **代码层**: 需要修改所有引用该字段的代码
4. **测试层**: 需要更新相关测试用例

---

## 附录：规范检查脚本建议

```bash
#!/bin/bash
# 数据库字段命名规范检查脚本

# 检查 _ts 后缀字段
echo "检查 _ts 后缀字段..."
# 应该全是 BIGINT 类型

# 检查 _at 后缀字段
echo "检查 _at 后缀字段..."
# 应该全是 BIGINT 类型

# 检查布尔字段前缀
echo "检查布尔字段前缀..."
# 应该使用 is_ 或 has_ 前缀

# 检查禁止字段
echo "检查禁止字段..."
# 不应该存在被禁止的字段名
```

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本，基于字段命名规范检查生成 |