# M3-ISSUE-4: media_service::link_signer 字段缺失

**Status**: 🟢 resolved (issue obsolete — file relocated during facade conversion, no sqlx queries present)
**Severity**: 低
**Discovered**: M-3 阶段 B-Round 3 验收中 (2026-06-06)
**Origin**: `SQLX_OFFLINE=true cargo check --lib` 报缺失字段
**Blocks**: 不阻塞 M-3 Batch 1；仅影响 media 域的 `link_signer` 相关功能

---

## 1. 背景

M-3 阶段 B-Round 3 验收中发现，`media_service.rs::link_signer` 在 `SQLX_OFFLINE=true` 模式下报告 2 个字段缺失错误。这表明：

- 该文件已有 `sqlx::query!` / `query_as!` 宏（被 `.sqlx/` 缓存识别）
- 缓存中的查询引用了某个表 / 列，**当前 schema 中找不到**

阶段 B-Round 3 决策：**stash 测试**证实该错误为仓库既存漂移，与阶段 B-Round 3 的 token 认证迁移无关。Media 模块不在 M-3 阶段 B/C/D 范围内。

## 当前状态（2026-07-23 验证）

- `SQLX_OFFLINE=true cargo check --workspace --lib` 退出码 0，无 `link_signer` 相关错误
- 原 issue 引用的 `src/media/link_signer.rs` 路径在 facade 转换期间已不存在
- `MediaLinkSigner` 当前位于 `synapse-common/src/media_link_signer.rs`（纯 Rust，无 sqlx 查询）
- `synapse-services/src/media_service.rs` 使用 `MediaLinkSigner` 但无 sqlx 查询
- 结论：本 issue 为 facade 转换前的遗留漂移，当前代码库中已不触发

## 原始内容（归档）

以下内容为 2026-06-06 发现时的原始记录，保留供参考：

```
error: no rules expected `!`
   --> src/media/link_signer.rs:<line>
    |
    |             SELECT <col> AS "field!"
    |                                          ^ no rules expected this token
```

具体行号 / 列名待审计时确认。

## 3. 可能原因

| 原因 | 概率 | 验证方法 |
|------|------|----------|
| DB schema 已删除该列 | 高 | `psql $DATABASE_URL -c "\d <table>"` |
| Rust struct 字段名与 DB 列名拼写不一致 | 中 | 对比 `link_signer.rs` 字段名与 schema |
| `.sqlx/` 缓存陈旧 | 中 | 重新 `cargo sqlx prepare` 看是否消失 |
| 字段已重命名 / 重构 | 中 | `git log -p src/media/link_signer.rs` |

## 4. 修复方案

```bash
# Step 1: 复现错误
SQLX_OFFLINE=true cargo check --lib 2>&1 | grep link_signer

# Step 2: 检查 schema
psql $DATABASE_URL -c "\d <related_table>"

# Step 3: 重新生成缓存
cargo sqlx prepare --workspace

# Step 4: 如果错误消失，单纯是缓存陈旧，commit 新缓存即可
# Step 5: 如果错误仍存在，根据 root cause 修复 schema 或代码
```

## 5. 验收

- [x] 错误根因已识别：facade 转换期间文件已迁移，原路径不再存在
- [x] `SQLX_OFFLINE=true cargo check --workspace --lib` 退出码 0
- [x] 当前代码库中无 `link_signer` 相关 sqlx 查询
- [x] `.sqlx/` 缓存与代码同步（无相关查询需缓存）

## 6. 工时估计

| 工作量 | 时间 |
|--------|------|
| 复现 + 定位 | 0.1 天 |
| 修复 + 验证 | 0.1 天 |
| **总计** | **0.2 天** |

## 7. 备注

- 本 issue 是 M-3 阶段 B-Round 3 验收时的**副产物**
- M-3 阶段 C/D 的工作**未触及** `media_service.rs`
- 修复后建议在 M-3 阶段 F（CI 门禁）之前完成，避免阶段 F 的 `check_sqlx_dynamic_ratio.sh` 阈值调整时误判
