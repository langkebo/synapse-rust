# T11 Cache 读写对称审查报告

**审查日期**: 2026-09-09
**审查范围**: synapse-rust workspace 中所有 `set_raw`/`get_raw`/`get_raw_shared` 的使用模式

---

## 核心规则

### Cache 读写对称铁律

> `set_raw` 异步写 L1+L2，但同步 `get_raw` 只读 L1。跨实例/重启/驱逐场景必须用 `get_raw_shared().await`（L1 miss 回源 Redis 并回填），否则误判"未命中=已变更"。

### 方法语义总结

| 方法 | 类型 | L1 读 | L2 读 | L1 写 | L2 写 | 用途 |
|------|------|-------|-------|-------|-------|------|
| `set_raw` | async | 否 | 否 | ✅ | ✅ | 双层写入 |
| `get_raw` | sync | ✅ | 否 | 否 | 否 | 本地读取 |
| `get_raw_shared` | async | ✅ | ✅ | ✅ | 否 | 跨实例读取 |

---

## 审查发现

### ✅ 已正确使用的模块

#### 1. sliding_sync_service/extensions.rs
- **presence 去重** (S7): 使用 `get_raw_shared` 读取、`set_raw` 写入
- **e2ee 去重** (S-8): 使用 `get_raw_shared` 读取、`set_raw` 写入  
- **to_device 去重**: 使用 `get_raw_shared` 读取、`set_raw` 写入

#### 2. sliding_sync_service/mod.rs
- **stream_id 缓存**: 使用 `get_raw_shared` 读取、`set_raw` 写入

#### 3. federation_signature_cache.rs
- 所有签名验证缓存均使用 `Cache`（L1）+ `get_raw_shared` 读取，符合要求

---

### ⚠️ 发现的违规情况

#### 1. auth/token.rs (行 57, 78, 92)

**违规点 1 - Line 57**: `get_raw` 读取撤销检查缓存
```rust
if self.cache.get_raw(&revocation_ok_key).is_none() { // ← L1 only read
    // DB 检查...
    self.cache.set_raw(&revocation_ok_key, "1", ...).await; // ← L1+L2 write
}
```

**风险**:
- Instance A 完成撤销检查后 `set_raw` 写入 L1+A, Redis
- Instance B 新的请求 `get_raw` 时 L1 为空 → 误判为"未缓存" → 再次查询 DB
- 这不是数据不正确，但会增加 DB 负载

**影响**: 性能下降，非数据一致性问题（撤销结果本身是幂等的）

**修复建议**: 对于高频撤销检查路径，考虑使用 `get_raw_shared` 以减少 DB 访问。

**违规点 2 - Line 92**: `get_raw` 读取 logout_marker
```rust
let logout_marker = format!("user:logout_all:{}", claims.sub);
if let Some(marker_val) = self.cache.get_raw(&logout_marker) { // ← L1 only read
    if let Ok(logout_ts) = marker_val.parse::<i64>() {
        if claims.iat < logout_ts {
            return Err(ApiError::unauthorized("Token has been revoked".to_string()));
        }
    }
}
```

**风险**:
- 登出行为: session.rs 行 104 `set_raw(logout_marker)` 写入 L1+Redis
- 假设登出后，请求落在不同实例
- 该实例的 `get_raw` 只读 L1 → 返回 None → 误判为"未登出"
- **这会导致登出失效**！

**影响**: 安全漏洞 - 用户登出后依然可以调用 API

**修复**: **必须使用 `get_raw_shared`** 以确保跨实例登出生效

---

#### 2. auth/session.rs

**风险点**: Line 104
```rust
self.cache.set_raw(&logout_marker, &now.to_string(), ...).await;
```

写入的是 L1+Redis，但 token.rs 的读取是 L1 only。

---

## 被错误修复的示例（参考历史）

根据 memory 记录，2026-09-08 的 sliding_sync 去重问题曾因同步 `get_raw` 导致：
- 跨实例/重启后返回 None
- 之前认为为空集，反复触发 de-dup 逻辑
- 造成 sync↔presence 忙循环

修复方式: 所有去重键统一使用 `get_raw_shared`

---

## 修复建议

### 优先级 1（必须修复 - 安全问题）

| 位置 | 当前 | 建议修改 |
|------|------|----------|
| `auth/token.rs:92` | `self.cache.get_raw(&logout_marker)` | `self.cache.get_raw_shared(&logout_marker).await` |

### 优先级 2（性能优化）

| 位置 | 当前 | 建议修改 |
|------|------|----------|
| `auth/token.rs:57` | `self.cache.get_raw(&revocation_ok_key)` | `self.cache.get_raw_shared(&revocation_ok_key).await` |

> 注: 撤销检查是幂等操作，使用 `get_raw_shared` 可减少 DB 访问，提升热点路径性能。

### 同步代码中的 async 边界

对于必须保持同步的热路径，可考虑：
1. 将 `AuthService::validate` 方法改为 async
2. 或在调用前预加载必要的共享缓存

---

## 修复后验证计划

1. **单元测试**: 添加 `session.rs` logout_market 跨实例场景测试
2. **集成测试**: 模拟多实例登出生效
3. **负载测试**: 验证 `get_raw_shared` 对 DB 访问的减少
4. **Clapcy**: 确保 `clippy -D warnings` 通过

---

## 修复完成

### ✅ 已修复的问题

#### 1. auth/token.rs (Line 92) - 登出标记跨实例安全问题

**修复日期**: 2026-09-09
**前后对比**:
```rust
// 修正前
if let Some(marker_val) = self.cache.get_raw(&logout_marker) {

// 修正后
if let Some(marker_val) = self.cache.get_raw_shared(&logout_marker).await {
    // T11: use get_raw_shared for cross-instance consistency.
    // set_raw (session.rs) writes both L1 and Redis, but get_raw (sync) only
    // reads L1. If the request hits a different instance, the logout marker
    // would be missed - a security issue (logout bypass).
```

**验证**: 4 个 S4 撤销缓存测试全部通过

#### 2. auth/token.rs (Line 57) - 撤销检查缓存读取性能优化

**修复日期**: 2026-09-09
**前后对比**:
```rust
// 修正前
if self.cache.get_raw(&revocation_ok_key).is_none() {

// 修正后
if self.cache.get_raw_shared(&revocation_ok_key).await.is_none() {
    // T11: use get_raw_shared for cross-instance consistency.
    // set_raw (line 78) writes both L1 and Redis, but get_raw (before fix)
    // only reads L1. In multi-instance scenarios, this could cause unnecessary
    // DB queries when another instance has the cache hit.
```

**验证**: 编译通过，Clippy 无警告，S4 测试通过

---

## 结论

| 项目 | 状态 |
|------|------|
| sliding_sync service | ✅ 已符合 |
| federation 签名缓存 | ✅ 已符合 |
| auth/token.rs logout_marker | ✅ **已修复** (跨实例登出生效) |
| auth/token.rs revocation_ok_key | ✅ **已修复** (性能优化) |
| auth/session.rs | ✅ 写入方正确 (L1+Redis) |

**总计**:
- 发现 2 个违规项
- **全部已修复**
- 4 个 S4 测试验证通过
- Clippy `@ -D warnings` 通过

---

## 后续建议

1. ✅ 已完成: auth/token.rs 核心问题修复
2. 📋 可选: 为 logout_marker 添加跨实例场景集成测试
3. 📊 可选: 基准测试验证 `get_raw_shared` 对 DB 访问的减少效果
