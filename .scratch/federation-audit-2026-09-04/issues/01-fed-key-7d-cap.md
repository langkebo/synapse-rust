# F-01 🔴 P0 — 缺失 spec 要求的 7-day valid_until_ts 截断

**审计报告**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.scratch/federation-audit-2026-09-04/report.md`
**审计日期**：2026-09-04

## 问题描述

Matrix spec v1.6 §1.2 明确要求：
> Servers MUST use the lesser of this field and 7 days into the future when determining if a key is valid.

当前 `effective_cache_ttl_secs` 函数只取 `valid_until_ts - now`，未与 7 天取小。

## 风险

- 攻击者控制中间 notary 返回 `valid_until_ts = now + 365 days` 的 key
- homeserver 缓存该 key 365 天
- 即使 6 个月后合法 owner 撤销该 key，攻击者仍能用它伪造签名

## 受影响位置

1. `src/web/middleware/federation_auth.rs:407-410`（inbound）
2. `synapse-federation/src/client.rs:518-519`（outbound）

## 修复方案

```rust
const MAX_SERVER_KEY_VALIDITY_MS: i64 = 7 * 24 * 60 * 60 * 1000;  // 7 days

fn effective_key_validity_ms(keys: &ServerKeys, now_ms: i64) -> i64 {
    (keys.valid_until_ts.min(now_ms + MAX_SERVER_KEY_VALIDITY_MS) - now_ms).max(0)
}
```

## 验证

- 单元测试 `cache_ttl_shrinks_to_valid_until_ts_window` 已存在
- 新增测试：`cache_ttl_caps_at_7_days_when_valid_until_is_1_year`
- 集成测试：发送 `valid_until_ts = now + 365 days` 的 key，验证 8 天后无法验签

## 优先级

🔴 P0（spec 合规 + 主动威胁场景）

## 工作量

0.5d
