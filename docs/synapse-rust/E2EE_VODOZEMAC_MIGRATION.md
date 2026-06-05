# E2EE → vodozemac 迁移设计 + 互操作测试矩阵

> 分支: `feature/e2ee-vodozemac`
> 关联审计报告: [COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md](./COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) C-5
> 审计风险: 自研 Olm/Megolm 路径与 vodozemac 0.9 行为不一致，跨 Element 客户端互操作存在不可观察的差异

## 一、迁移目标

把 `src/e2ee/` 下的所有自研密码学实现收敛到 vodozemac 0.9（已存在依赖），消除自研 ratchet / message index / 密钥派生代码。具体范围：

| 当前模块 | 自研内容 | 目标 vodozemac 路径 |
|---|---|---|
| `e2ee/olm/service.rs` | `OlmAccount`、`OlmSession` 自研 | `vodozemac::olm::Account` / `vodozemac::olm::Session` |
| `e2ee/olm/session.rs` | ratchet state、message key 派生 | `vodozemac::olm::Session::encrypt/ decrypt` |
| `e2ee/megolm/service.rs` | Megolm ratchet、AES-256-GCM 加密 | `vodozemac::megolm::Session` |
| `e2ee/crypto/aes.rs` | AES-256-GCM 包装 | 删除（由 vodozemac 内部使用） |
| `e2ee/crypto/argon2.rs` | 独立 argon2 包装 | 保留（SSSS passphrase 派生，与 vodozemac 无交集） |
| `e2ee/crypto/x25519.rs` | 手动 X25519 派生共享密钥 | `vodozemac::Curve25519PublicKey::agree` |
| `e2ee/crypto/ed25519.rs` | ed25519-dalek 包装 | 保留（vodozemac 内部使用 ed25519-dalek，对外保持一致接口） |
| `e2ee/key_request/*` | 自定义 key request 协议 | 保持协议层（与 vodozemac 无关） |
| `e2ee/cross_signing/*` | 派生参数与 Synapse 不一致 | 对齐 vodozemac 0.9 默认参数 |

## 二、不在迁移范围

- E2EE 协议层（to-device、SSS、secure backup、cross-signing 的协议消息格式）
- 存储层（`device_keys/storage.rs`、SSS、backup 持久化）
- 设备验证（verification 协议、与 vodozemac 无关的 SAS/MAC 计算）

## 三、分阶段实施

### Phase 1（2 周）— 桥接层 + 单测
1. 在 `e2ee/crypto/vodozemac.rs` 新建统一接口，对 vodozemac 类型做项目内包装。
2. 替换 `e2ee/olm/{service,session}.rs` 的内部状态计算为 vodozemac 调用。
3. 行为保持 bit-level 兼容（PKCS 8 / base64 / pickle 格式不变）。
4. 新增 `e2ee::compat` 单测：把 vodozemac 0.9 文档中的 vector 与自研旧路径做交叉验证。

### Phase 2（1 周）— Megolm 收敛
1. `MegolmSession` 内部改持 `vodozemac::megolm::Session`。
2. `encrypt_at_index` 调 `megolm_session.encrypt(plaintext)`，`decrypt_at_index` 调 `session.decrypt(ciphertext)`。
3. wire 格式（消息基索引、消息 index 编码）与 Synapse v1.153 保持一致。

### Phase 3（2 周）— 跨客户端互操作
1. Element Web 互操作（已有 dev 环境）。
2. Element Android（需要 Android 调试机）。
3. Element iOS（需要 macOS 调试机 + TestFlight）。
4. 三客户端交叉 send/receive 1000 条消息，验证：
   - Olm prekey message 解密成功率 ≥ 99.9%
   - Megolm session 转发成功率 ≥ 99.9%
   - 前向保密：每发送 N 条后轮换 session，旧 session 仍能解密历史消息，新 session 不能解密新消息

### Phase 4（1 周）— 清理
1. 删除 `e2ee/crypto/{aes,x25519,mod}.rs` 中被取代部分。
2. 删除 `e2ee/olm/session.rs` 自研 ratchet。
3. 更新 `Cargo.toml`：将 `vodozemac` 移出 optional。
4. 更新文档：`docs/sdk/e2ee.md` 标记新实现。

## 四、互操作测试矩阵

### 4.1 单元对拍（Vodozemac test vectors）

| Case | 客户端 | 路径 | 期望结果 |
|---|---|---|---|
| V-1 | rust-vodozemac | `e2ee::compat::vectors::olm_pickled_account` | 与 `vodozemac::test_vectors` 完全一致 |
| V-2 | rust-vodozemac | `e2ee::compat::vectors::megolm_pickle` | 与 `vodozemac::test_vectors` 完全一致 |
| V-3 | rust-vodozemac | `e2ee::compat::vectors::ed25519_sign` | 与 `ed25519-dalek` reference 一致 |
| V-4 | rust-vodozemac | `e2ee::compat::vectors::prekey_message_decrypt` | 解出 reference plaintext |

### 4.2 双客户端互操作（synapse-rust ↔ Element Web）

| Case | 触发条件 | 期望结果 |
|---|---|---|
| I-1 | 在 synapse-rust 端创建账号 A，在 Element Web 创建账号 B，邀请进同一房间 | 双方成功同步 m.room.encrypted 状态 |
| I-2 | A 发 1 条 1:1 消息给 B | B 解密并显示明文 |
| I-3 | B 离线期间 A 连续发 1000 条 | B 上线后 1 次 sync 拉全（Megolm session 复用） |
| I-4 | A 轮换 device | B 收到 m.device_list_update，新 device 进入 megolm 转发列表 |
| I-5 | A 撤销 device | B 不再收到该 device 的 to-device 转发 |
| I-6 | A 启用 cross-signing，签 master key | B 端显示 verified |
| I-7 | A 用 UIA 备份私钥 | B 收到 m.secret.send，B 可解密 |
| I-8 | 故意注入错误密钥 | 拒绝并返回 M_INVALID_SIGNATURE / M_BAD_MAC |

### 4.3 协议稳定性

| 指标 | 阈值 |
|---|---|
| Olm 加密-解密成功率 | ≥ 99.99% |
| Megolm 单 session 转发 ≥ 100k 条 | 必须 |
| 前向保密验证 | 旧 device 撤销后无法解密新消息 |
| 后向保密验证 | 旧 device 仍能解密历史消息（用 archive session） |
| 与 Element Android/iOS 跨端互通 | I-1~I-7 全绿 |
| pickle 格式兼容 | vodozemac 0.9 reference 工具能解析 |

### 4.4 性能基线

| 指标 | 当前（自研） | 目标（vodozemac） |
|---|---|---|
| Olm encrypt P50 | x | ≤ x × 1.0（不退化） |
| Megolm encrypt P50 | y | ≤ y × 1.0 |
| Megolm encrypt P99 | z | ≤ z × 1.2 |
| 并发 100 session 创建 | t | ≤ t × 1.5 |

## 五、CI 集成

### 5.1 `tests/e2e/e2ee_vodozemac_interop.rs`（新增）

- 启动本地 vodozemac harness（`vodozemac-cli` 子进程 + Matrix mock server）
- 模拟 A/B 双客户端，跑 4.2 全部 case
- 失败即 dump wire 日志到 `artifacts/e2ee-interop/`

### 5.2 `tests/e2e/cross_client_matrix.yml`（新增）

- 矩阵化测试描述：客户端组合（Element Web / Android / iOS / synapse-rust）交叉
- 默认启 Web × synapse-rust；其余在 nightly 跑

### 5.3 CI 工作流

新文件 `.github/workflows/e2ee-interop.yml`：
- trigger：push 到 `feature/e2ee-vodozemac/**` 与每周 cron
- job：启动 2 实例 homeserver + 1 个 Element Web 实例，跑 4.2 全 case

## 六、灰度与回滚

1. **Feature flag**：`E2EE_USE_VODOZEMAC`（默认 `false` → 灰度到 `true`）。
2. 灰度：dev 灰 1 周 → staging 灰 1 周（仅 1% homeserver 启用）→ 全量。
3. 回滚：feature flag 一键回到自研路径；vodozemac 路径与旧路径并行 ≥ 2 个 release 周期。
4. 监控：
   - `m.room.encrypted` 解密失败率（按 homeserver 维度）
   - `to_device` 解密失败率
   - megolm 转发失败率
   - 每 homeserver 失败率 > 0.5% 自动告警

## 七、风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|
| vodozemac 0.9 API 不兼容旧 client 的 pickle | 中 | 高 | 保留 0.9 + 自研双路径并行 ≥ 2 版本 |
| Element iOS 端存在不同 vodozemac 子版本 | 中 | 中 | 在 4.2 矩阵中固定 Element iOS 1.14.0 |
| 性能退化 > 20% | 低 | 中 | 4.4 性能基线 + 灰度 |
| 私钥派生参数与 Synapse v1.153 不一致 | 中 | 高 | 复用 vodozemac 0.9 默认参数 + Element 客户端基线对比 |
| 跨服务密钥轮换触发密钥缓存失效风暴 | 中 | 中 | 复用 federation_signature_cache 的失效广播 |

## 八、收尾标准

- `src/e2ee/crypto/aes.rs`、`x25519.rs`（与 vodozemac 重叠部分）已删除
- `e2ee/olm/session.rs` 自研 ratchet 已删除
- `cargo test --lib` 全绿
- 4.2 全部 case 通过
- `cargo clippy --all-features --locked -- -D warnings` 通过
- 覆盖率：P0 路径 ≥ 90%（codecov security_p0 块覆盖 e2ee/**）
- 文档：`docs/sdk/e2ee.md` 更新
- 公告：发版说明里加 vodozemac 升级

## 九、Phase 1 状态报告（2026-06-05）

### 9.1 完成项

- **`MegolmProvider` 双路径抽象**（[src/e2ee/megolm/service.rs](../../src/e2ee/megolm/service.rs#L192-L351)）
  - `MegolmBackend` 枚举：`Legacy`（自研 AES-256-GCM，向后兼容）/ `Vodozemac`（0.9 互操作）
  - `MegolmProvider` 枚举统一封装两种实现，对外暴露相同 API 表面
  - 选择规则：环境变量 `E2EE_USE_VODOZEMAC_MEGOLM=true` 强制启用 vodozemac
  - feature flag 关闭时退化为 `MegolmService` 类型别名，最小构建仍可编译

- **`MegolmVodozemacService` 装配**（[src/e2ee/vodozemac_megolm.rs](../../src/e2ee/vodozemac_megolm.rs)）
  - 完整的 vodozemac-backed Megolm 会话管理（`GroupSession` / `InboundGroupSession`）
  - 加密：`encrypt` / `encrypt_many`（批量加密，复用 ratchet）
  - 解密：`decrypt` 接受 vodozemac `MegolmMessage` 字节流
  - 共享：`share_session` 调用 `upsert_session_keys_batch` 批量持久化
  - 接收方读取：`get_session_key_for_user` 走 cache → DB 二级回源
  - 导入：`import_session` 从 `m.room_key` 构造 `InboundGroupSession`

- **Storage 支撑**（[src/e2ee/megolm/storage.rs](../../src/e2ee/megolm/storage.rs)）
  - `increment_message_index`：原子更新 message_index 与 last_used_ts
  - `upsert_session_keys_batch`：批量写入 recipient 的 session key
  - `get_session_key`：recipient 端查询已分享的 key

- **ServiceContainer 集成**（[src/services/container.rs](../../src/services/container.rs#L146-L149)）
  - `E2eeServices::megolm_service` 字段类型改为 `MegolmProvider`
  - 装配时按 feature flag 调用 `MegolmProvider::from_env`
  - `KeyRequestService` / `KeyRotationService` 同步切换为 `MegolmProvider`

- **可观测性补全**（[src/common/server_metrics.rs](../../src/common/server_metrics.rs#L75-L86)）
  - 新增 `megolm_share_total` / `megolm_share_recipients_total`
  - 新增 `megolm_share_db_duration_ms` / `megolm_share_cache_duration_ms` 两个 histogram
  - 新增 `megolm_share_cache_errors_total` / `megolm_share_db_errors_total`
  - 新增 `megolm_session_key_read_total` / `megolm_session_key_read_duration_ms`
  - 新增方法：`record_megolm_share` / `record_megolm_share_cache_error` / `record_megolm_session_key_read`

### 9.2 验证结果

| 步骤 | 命令 | 结果 |
|---|---|---|
| 类型检查 | `cargo check --features vodozemac-megolm` | ✅ 通过 |
| Lint | `cargo clippy --features vodozemac-megolm --locked -- -D warnings` | ✅ 通过 |
| vodozemac 内部对拍 | `vodozemac_megolm_roundtrip` / `pickle_roundtrip` / `message_index_monotonic`（lib test 编译受阻于预存 drift，未跑成） | ⏸ 阻塞 |
| 4.2 跨客户端互操作 | I-1 ~ I-8 | ⏸ 留待 Phase 3 |

### 9.3 已知阻塞（非 Phase 1 范围）

- 预存的 `src/storage/room.rs` 与 `src/storage/room/` 目录冲突导致 `cargo test --lib` 无法编译（[src/storage/mod.rs:39](../../src/storage/mod.rs#L39) `pub mod room;`）
- 预存的 `src/storage/application_service.rs` / `src/web/routes/app_service.rs` 测试代码使用已被重命名的字段（`exclusive` → `is_exclusive`，`rate_limited` → `is_rate_limited`）

两项均不属于 Phase 1 范围，留待单独清理。

### 9.4 Phase 2 入口

Phase 2（Megolm 双写）旨在为存量 legacy session 提供平滑迁移到 vodozemac 路径的能力。
详见 [十、Phase 2 状态报告](#十phase-2-状态报告2026-06-05megolm-双写) 章节。


## 十、Phase 2 状态报告（2026-06-05，Megolm 双写）

### 10.1 完成项

#### 10.1.1 数据模型扩展

- **`megolm_sessions` 表新增字段**（[migrations/20260605120000_megolm_vodozemac_dual_write_v8.sql](../../migrations/20260605120000_megolm_vodozemac_dual_write_v8.sql)）
  - `pickle_format TEXT NOT NULL DEFAULT 'legacy'`（取值 `'legacy'` / `'vodozemac'` / `'dual'`，CHECK 约束）
  - `vodozemac_pickle TEXT`（vodozemac 0.9 pickle 副本，base64 编码 JSON）
  - 部分索引 `idx_megolm_sessions_pickle_format_legacy` 加速懒迁移扫描

- **`PickleFormat` 枚举**（[src/e2ee/megolm/models.rs](../../src/e2ee/megolm/models.rs#L13-L43)）
  - `Legacy`（自研 AES-256-GCM）、`Vodozemac`（vodozemac 0.9 pickle）、`Dual`（同时持有两种）
  - `as_str` / `from_str` 序列化方法，兼容未知字符串 fallback 到 `Legacy`

#### 10.1.2 双写实现

- **`MegolmVodozemacService::create_session` 双写分支**（[src/e2ee/vodozemac_megolm.rs](../../src/e2ee/vodozemac_megolm.rs#L141-L213)）
  - 环境变量 `E2EE_DUAL_WRITE=true` 启用（默认 `false`）
  - 启用时：把 vodozemac 32 字节 session_key 用 `Aes256GcmCipher` 加密，写入 `session_key` 列；同时保留 vodozemac 副本到 `vodozemac_pickle` 列；`pickle_format = 'dual'`
  - 关闭时：仅写 vodozemac pickle 到 `session_key` 列；`pickle_format = 'vodozemac'`
  - 需要先注入 `encryption_key`（通过 `with_encryption_key`），否则双写自动降级为单路径

- **`update_vodozemac_pickle` 持久化最新 ratchet 状态**（[src/e2ee/megolm/storage.rs](../../src/e2ee/megolm/storage.rs#L206-L227)）
  - encrypt_many 加密 N 条后批量更新 `vodozemac_pickle` 列
  - 失败仅记日志：cache 中已有更新副本，不阻塞本次 encrypt 返回
  - decrypt 路径同样调用此方法持久化 inbound 端 ratchet

#### 10.1.3 懒迁移（Lazy Migration）

- **`promote_to_dual`**（[src/e2ee/megolm/storage.rs](../../src/e2ee/megolm/storage.rs#L295-L319)）
  - 仅在 `pickle_format = 'legacy'` 且 `vodozemac_pickle IS NULL` 时生效
  - 幂等：第二次调用返回 `false`（条件不满足）
  - 适用场景：扫描到 legacy 会话时由后台任务或运维脚本调用

- **`list_legacy_sessions` 分页扫描**（[src/e2ee/megolm/storage.rs](../../src/e2ee/megolm/storage.rs#L324-L395)）
  - 游标分页：按 `session_id` 排序，调用方传 `after_session_id` 取下一页
  - `limit` 参数 clamp 到 `[1, 1000]`，避免误调用 OOM
  - 部分索引 `pickle_format = 'legacy'` 命中，O(log n) 查询

- **`count_by_pickle_format` 监控进度**（[src/e2ee/megolm/storage.rs](../../src/e2ee/megolm/storage.rs#L398-L413)）
  - 返回 `[(format, count), ...]`，运维/SRE 用以观察迁移收敛

#### 10.1.4 可观测性

- **新增 7 个 Megolm metrics**（[src/common/server_metrics.rs](../../src/common/server_metrics.rs#L87-L96)）
  - `megolm_vodozemac_pickle_persist_total` / `megolm_vodozemac_pickle_persist_errors_total`
  - `megolm_dual_write_promotions_total` / `megolm_dual_write_promotion_errors_total`
  - `megolm_lazy_migration_sessions_scanned_total` / `megolm_lazy_migration_sessions_promoted_total`
  - `megolm_pickle_persist_duration_ms` histogram

- **3 个记录方法**（[src/common/server_metrics.rs](../../src/common/server_metrics.rs#L402-L430)）
  - `record_megolm_vodozemac_pickle_persist(duration_ms, success)` — 失败时**不**observe histogram
  - `record_megolm_dual_write_promotion(success)` — success/fail 分别累加
  - `record_megolm_lazy_migration_batch(scanned, promoted)` — 批量扫描一次调用

#### 10.1.5 测试覆盖

- **存储层集成测试**（[tests/unit/megolm_dual_write_storage_tests.rs](../../tests/unit/megolm_dual_write_storage_tests.rs)）
  - `test_create_session_writes_dual_pickle_columns` — 双列写入正确性
  - `test_create_session_vodozemac_only_path` — 单路径 vodozemac 写入
  - `test_update_vodozemac_pickle_persists_new_ratchet` — ratchet 持久化
  - `test_update_vodozemac_pickle_no_match_returns_false` — 不存在 session 不报错
  - `test_promote_legacy_to_dual_succeeds` / `test_promote_to_dual_is_idempotent` / `test_promote_to_dual_skips_non_legacy_rows`
  - `test_list_legacy_sessions_pagination` / `test_list_legacy_sessions_clamps_limit`
  - `test_count_by_pickle_format`
  - `test_lazy_migration_end_to_end` — list → promote → count 完整闭环

- **Metrics 单元测试**（[tests/unit/megolm_dual_write_metrics_tests.rs](../../tests/unit/megolm_dual_write_metrics_tests.rs)）
  - 9 个测试覆盖成功/失败/混合路径下 counter 与 histogram 累加正确性
  - 包含端到端循环测试（100 次 90% 成功率场景）

- **模型与 pickle 单元测试**（[src/e2ee/megolm/models.rs](../../src/e2ee/megolm/models.rs)、[src/e2ee/vodozemac_megolm.rs](../../src/e2ee/vodozemac_megolm.rs)）
  - `PickleFormat` 序列化 / 反序列化三种变体
  - vodozemac session_key 长度 sanity check（32 字节 → ~44 字符 base64）
  - pickle roundtrip 通过 storage 格式

### 10.2 验证结果

| 步骤 | 命令 | 结果 |
|---|---|---|
| 类型检查（lib） | `cargo check --lib --tests --features test-utils` | ✅ 通过（无 megolm_dual_write 错误） |
| 类型检查（unit） | `cargo check --test unit --features test-utils` | ✅ 新增测试文件无编译错误 |
| 集成测试运行 | `cargo test --test unit megolm_dual_write_` | ⏸ 待 CI 跑（需 PostgreSQL） |
| Metrics 单元测试 | `cargo test --test unit megolm_dual_write_metrics` | ⏸ 待 CI 跑 |

### 10.3 灰度与回滚路径

- **灰度开关**：`E2EE_DUAL_WRITE=true`（仅影响**新增** session）
  - Phase 2 早期：仅 dev/staging 灰度
  - Phase 2 后期：prod 灰 1% → 10% → 100%
  - 关闭双写后，新 session 回落到 `pickle_format='vodozemac'` 单路径；存量 dual session 仍能正常 decrypt（vodozemac 副本完整）

- **回滚路径**：
  - Feature flag 一键关 → 新数据不再双写
  - 存量 `dual` session 的 `vodozemac_pickle` 列在 fallback 时仍可被 vodozemac-only 路径使用
  - 存量 `legacy` session 走原始自研路径（`MegolmProvider::Legacy` 分支）
  - 监控：`megolm_dual_write_promotion_errors_total` 增长 → 触发回滚

- **监控指标**（[src/common/server_metrics.rs](../../src/common/server_metrics.rs)）
  - `megolm_dual_write_promotions_total` / `megolm_dual_write_promotion_errors_total` 比例
  - `megolm_vodozemac_pickle_persist_errors_total` rate（应 < 0.1%）
  - `megolm_lazy_migration_sessions_promoted_total` 增长曲线（看是否单调递增）

### 10.4 Phase 3 入口

Phase 3 将聚焦：
1. Olm 收敛（替换 `e2ee/olm/{service,session}.rs` 自研实现为 vodozemac 调用）
2. 跨客户端互操作（Element Web / Android / iOS）
3. 准备 Phase 4 清理（删除自研 AES-256-GCM 路径、`x25519.rs` 重叠部分）

## 十一、关联

- [COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md](./COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) — C-5、E2EE 节
- [Cargo.toml](../../Cargo.toml) — 已是 `vodozemac = "0.9"`
- [src/e2ee/mod.rs](../../src/e2ee/mod.rs) — 模块入口
- [docs/sdk/e2ee.md](../sdk/e2ee.md) — 上层协议文档
