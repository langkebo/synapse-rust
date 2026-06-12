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

### Phase 4（1 周）— 清理 / 边界冻结
1. ✅ 已完成：运行时 Megolm 主路径切到 vodozemac，`vodozemac` 已移出 optional，迁移期开关 `vodozemac-megolm` 已收口。
2. ✅ 已完成：`e2ee/crypto/mod.rs` 改为显式导出，`aes.rs` / `ed25519.rs` 的冗余桥接与测试辅助 API 已大幅收窄，子模块已收为私有实现细节。
3. 🚧 待最终关闭：`e2ee/olm/session.rs` 与更激进的自研协议包装删除仍需等待 Phase 3 跨客户端矩阵全绿后再评估。
4. 🚧 文档与公告：待最终关闭时再统一更新 `docs/sdk/e2ee.md` / 发版说明等对外口径。

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
- job 1：运行本地 `vodozemac` smoke，对拍低层 Olm/Megolm reference 行为
- job 2：checkout `matrix-js-sdk`，启动真实 `synapse-rust` docker stack，执行 `pnpm test:real-backend:verification`
- job 3：在同一 live backend 上叠加 `docker-compose.web.yml`，通过 `scripts/test/run_element_web_browser_harness.sh` 启动 Element Web + nginx，并默认运行 `tests/element-web-harness/basic-interactions.mjs`；`workflow_dispatch` 可切回 `smoke:login`
- 说明：当前 CI 已接入 SDK-backed real-backend verification，并把 Element Web 浏览器级 harness 默认提升到登录 + 房间创建 + 发消息的基础交互；Android/iOS 真机矩阵与更完整的浏览器互操作矩阵仍是 Phase 3 后续项

### 5.3.1 Android/iOS 手动验收入口（2026-06-11）

- **统一后端入口**：优先复用现有 `scripts/test/run_sdk_verification_real_backend.sh`，新增 `SKIP_SDK_TEST=1` 可只启动 live backend、不跑 `matrix-js-sdk`，并自动落基础证据到 artifact 目录

```bash
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-mobile"
SKIP_SDK_TEST=1 \
KEEP_STACK_RUNNING=1 \
SDK_INTEROP_ARTIFACT_DIR="artifacts/e2ee-interop/mobile/${RUN_ID}/backend" \
bash scripts/test/run_sdk_verification_real_backend.sh
```

- **Element Web 叠加入口**：若需要与 Android/iOS 做同一后端上的交叉验证，可在 backend ready 后叠加浏览器层

```bash
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-mobile"
BROWSER_ONLY_OVERLAY=1 \
KEEP_STACK_RUNNING=1 \
TEST_SCRIPT=test:basic \
ELEMENT_HARNESS_ARTIFACT_DIR="artifacts/e2ee-interop/mobile/${RUN_ID}/element-web" \
bash scripts/test/run_element_web_browser_harness.sh
```

- **客户端接入地址建议**：
  - `Element iOS Simulator`：`http://localhost:8008`
  - `Element Android Emulator`：`http://10.0.2.2:8008`
  - 真机：准备自定义 `DOCKER_ENV_FILE`，把 `SERVER_NAME` / `PUBLIC_BASEURL` / `ALLOWED_ORIGINS` 改成宿主机可达地址后，再以同样方式运行 backend 脚本
- **执行顺序建议**：
  1. 运行 backend-only 入口并保留栈
  2. 在 Android / iOS 登录两个测试账号并加入同一加密房间
  3. 如需 Web 参与交叉验证，再叠加 `BROWSER_ONLY_OVERLAY=1` 的 Element Web harness
  4. 按 4.2 的 I-1 ~ I-8 case 顺序记录结果

### 5.3.2 结果记录规范（Android/iOS）

- **artifact 目录约定**：

```text
artifacts/e2ee-interop/mobile/<run-id>/
├── backend/
│   ├── backend-entry.txt
│   ├── docker-compose.ps.txt
│   └── client-versions.json
├── android/
│   ├── summary.md
│   ├── case-I1.png
│   ├── case-I2.png
│   └── case-matrix.md
├── ios/
│   ├── summary.md
│   ├── case-I1.png
│   ├── case-I2.png
│   └── case-matrix.md
└── element-web/
    └── ...
```

- **每个平台至少保留**：
  - `summary.md`：客户端版本、设备型号、运行环境（模拟器/真机）、homeserver 地址、执行时间
  - `case-matrix.md`：I-1 ~ I-8 每项的 `pass/fail/not-run`、失败摘要、对应截图或日志路径
  - 关键截图：登录成功、加密房间创建/加入、消息明文可见、cross-signing/backup 状态页
- **`summary.md` 最小模板**：

```md
# Android Interop Summary

- run_id: 20260611T120000Z-mobile
- client_version: Element Android x.y.z
- device: Pixel 7 Emulator / Android 15
- homeserver: http://10.0.2.2:8008
- companion_clients: Element Web / Element iOS
- result: partial
- notes: I-1~I-3 pass, I-4 blocked by device re-login
```

- **`case-matrix.md` 最小模板**：

```md
| Case | Result | Evidence | Notes |
|---|---|---|---|
| I-1 | pass | `case-I1.png` | 房间加密状态同步成功 |
| I-2 | pass | `case-I2.png` | 明文可见 |
| I-3 | not-run | - | 待长时间离线场景 |
```

### 5.3.3 Android/iOS 执行 Checklist（I-1 ~ I-8）

- **通用前置检查**
  - [ ] 已生成 `RUN_ID`，并运行 backend-only 入口保留 `artifacts/e2ee-interop/mobile/<run-id>/backend/`
  - [ ] `backend/client-versions.json` 已生成，确认 homeserver 可访问
  - [ ] Android / iOS 客户端版本、设备型号、模拟器/真机信息已写入各自 `summary.md`
  - [ ] 如需 Web 交叉验证，已运行 Element Web overlay，并确认 `element-web/` 目录可写入截图与日志
  - [ ] 已准备 2 个测试账号：
    - `A`：主发送方，优先放在 Android 或 iOS
    - `B`：主接收方，优先放在另一个移动端或 Element Web
  - [ ] 两端都已完成首次登录后的密钥初始化，未停留在 `Setting up keys` 或恢复引导页面

- **I-1 房间加密状态同步**
  - [ ] 用 `A` 创建私聊或小群房间，并邀请 `B`
  - [ ] 确认房间已开启 E2EE，双方房间详情页都能看到加密状态
  - [ ] Android 侧截图保存为 `android/case-I1-room-state.png`
  - [ ] iOS 侧截图保存为 `ios/case-I1-room-state.png`
  - [ ] 在两端 `case-matrix.md` 记录 `I-1`
  - **通过标准**：双方都成功进入同一加密房间，且都能观察到 `m.room.encrypted` 生效

- **I-2 单条消息端到端解密**
  - [ ] `A` 向 `B` 发送 1 条纯文本消息，例如 `I2 hello from <client>`
  - [ ] `B` 成功看到明文，不出现“无法解密”占位
  - [ ] 反向再发 1 条，从 `B` 到 `A`
  - [ ] Android / iOS 各保存 1 张明文消息截图：`case-I2-send.png` / `case-I2-recv.png`
  - [ ] 在 `case-matrix.md` 记录发送端、接收端、房间 ID、消息时间
  - **通过标准**：双向消息都能明文显示，无红锁错误、未知设备阻断或解密失败提示

- **I-3 离线期间批量消息补拉**
  - [ ] 让 `B` 完全离线：
    - Android：强制停止应用或关闭网络
    - iOS：杀掉应用并打开飞行模式，或确保不再前台 sync
  - [ ] `A` 在离线窗口内连续发送消息；快速验收至少先做 20 条，若通过再扩到目标批量
  - [ ] 重新让 `B` 上线并等待首次 sync 完成
  - [ ] 确认 `B` 能一次性拉到离线期消息，且中间不出现明显缺口
  - [ ] 保存离线前后截图：`case-I3-offline.png`、`case-I3-recovered.png`
  - [ ] 在 `case-matrix.md` 记录发送条数、恢复耗时、缺失条数
  - **通过标准**：离线期间消息恢复后可连续查看，未出现 Megolm session 丢失或重复建房间密钥导致的大面积解密失败

- **I-4 新 device 加入与 device list 更新**
  - [ ] 为 `A` 新增第二台设备：
    - Android：新开一个 emulator 或登出后在另一设备登录
    - iOS：使用 Simulator/真机上的第二实例或切换到另一设备
  - [ ] 新设备登录完成后，等待 `B` 侧收到设备列表更新
  - [ ] 在 `B` 侧打开用户/设备安全信息，确认可见 `A` 的新 device
  - [ ] 用 `A` 的新 device 发一条消息，确认 `B` 能正常解密
  - [ ] 保存证据：设备列表截图 `case-I4-device-list.png`，新 device 发消息截图 `case-I4-message.png`
  - [ ] 在 `case-matrix.md` 记录新 device 标识、是否看到 `device_list_update`
  - **通过标准**：新 device 出现在 `B` 的已知设备中，且加入后发送的加密消息能被 `B` 成功解密

- **I-5 device 撤销后的前向保密**
  - [ ] 选定 `A` 的旧 device，在会话中执行登出/移除/撤销
  - [ ] 让 `A` 的仍有效 device 再发送新消息
  - [ ] 确认 `B` 仍能解密新消息
  - [ ] 如可操作，检查被撤销 device 不再收到新的 to-device / 同步更新
  - [ ] 保存撤销前后截图：`case-I5-device-before.png`、`case-I5-device-after.png`
  - [ ] 在 `case-matrix.md` 记录撤销方式、撤销后首条新消息时间
  - **通过标准**：撤销不会破坏现有有效设备的消息解密；被撤销设备不再参与新的设备分发

- **I-6 cross-signing 验证状态传播**
  - [ ] 在 `A` 上启用 cross-signing，并完成需要的密码/UIA 步骤
  - [ ] 等待 `B` 同步到 `A` 的最新验证状态
  - [ ] 在 `B` 的设备/用户安全界面确认 `A` 显示为 `verified`
  - [ ] 保存截图：`case-I6-cross-signing-a.png`、`case-I6-cross-signing-b.png`
  - [ ] 在 `case-matrix.md` 记录是否需要重新登录、是否出现 bootstrap 卡点
  - **通过标准**：`A` 完成 cross-signing 后，`B` 最终能看到已验证状态，且过程中无签名校验失败提示

- **I-7 key backup / secret send**
  - [ ] 在 `A` 上开启 key backup，完成 UIA 或恢复密钥保存流程
  - [ ] 触发一次需要 secret send / key backup 同步的场景
  - [ ] 在 `B` 侧确认能收到相关恢复材料，或在恢复流程中成功解密历史消息
  - [ ] 保存 key backup 状态页或恢复成功截图：`case-I7-backup.png`
  - [ ] 如有日志，可把关键事件摘要写入 `summary.md`
  - **通过标准**：key backup 建立成功，恢复材料可被另一端消费，不出现 `M_BAD_MAC` / `M_INVALID_SIGNATURE` 之类的错误

- **I-8 错误密钥注入拒绝**
  - [ ] 仅在可控测试环境执行，不要污染长期保留账号
  - [ ] 通过错误恢复密钥、过期 secret、或手工替换错误 key material 触发失败路径
  - [ ] 观察客户端是否明确拒绝并给出预期错误，而不是静默接受
  - [ ] 保存错误界面或日志：`case-I8-invalid-key.png`
  - [ ] 在 `case-matrix.md` 记录触发方式与实际错误文案
  - **通过标准**：错误密钥被拒绝，且能映射到 `M_INVALID_SIGNATURE` / `M_BAD_MAC` 或客户端等价错误

- **收尾检查**
  - [ ] `android/case-matrix.md` 已补齐 I-1 ~ I-8
  - [ ] `ios/case-matrix.md` 已补齐 I-1 ~ I-8
  - [ ] `summary.md` 中已写明 `result: pass / partial / fail`
  - [ ] 若本轮只完成部分 case，已在 `notes` 中说明阻塞点和下一步建议

### 5.4 Phase 4 边界冻结补充（2026-06-11）

- `Aes256Gcm*` 仍需保留：生产路径仍由 `src/e2ee/ssss/service.rs` 用于 SSSS 密钥封装，并由 `src/e2ee/vodozemac_megolm.rs` 用于 Phase 2 双写 legacy `session_key` 兼容写入
- `Ed25519*` 仍需保留：`src/e2ee/signed_json.rs` 仍承担 Matrix signed JSON 校验，`src/e2ee/signature/service.rs` 仍承担事件/键签名
- 方法级清单已进一步冻结：
  - `AES 保留`：`Aes256GcmKey::{generate, from_bytes}`、`Aes256GcmCipher::encrypt_with_nonce`
  - `AES 已删桥接/内聚`：`Aes256GcmKey::as_bytes`、`Aes256GcmNonce::as_bytes`、`Aes256GcmCipher::new` 已在 `src/` 与 `synapse-e2ee/` 两棵树同步删除；`Aes256GcmNonce::{generate, from_bytes}` 与 `Aes256GcmCipher::{encrypt, decrypt}` 已进一步收为模块私有实现细节，只保留同文件内部与测试使用；新增 `Aes256GcmCipher::split_encrypted_data` 私有辅助方法用于测试中提取 nonce 和密文，减少对私有构造的分散依赖
  - `Ed25519 保留`：`Ed25519PublicKey::{from_base64, verify}`、`Ed25519KeyPair::{generate, public_key, sign}`
  - `Ed25519 已收窄`：`Ed25519PublicKey::from_bytes` 已收为模块私有，原仅用于测试桥接的 `Ed25519PublicKey::as_bytes` 与 `Ed25519KeyPair::verify` 已在 `src/` 与 `synapse-e2ee/` 两棵树同步删除，测试现直接覆盖 `public_key.verify()` 公开面；`Ed25519SecretKey` 保持模块私有，`to_base64` / 测试构造辅助只留在 `#[cfg(test)]` 或 crate 内部
- 已完成的可见性收口：
  - `src/e2ee/mod.rs` 与 `synapse-e2ee/src/lib.rs` 已移除对 `Aes256Gcm*` / `Ed25519*` / `CryptoError` 的顶层 re-export，后续调用应显式经过 `e2ee::crypto::*` 或具体协议模块
  - `src/e2ee/crypto/mod.rs` 与 `synapse-e2ee/src/crypto/mod.rs` 已把 `aes` / `ed25519` 子模块改为私有实现细节；外部仅保留 `crypto::{Aes256Gcm*, Ed25519*, CryptoError}` 顶层入口，不再暴露 `crypto::aes::*` / `crypto::ed25519::*` 路径

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
| 类型检查 | `cargo check --locked --lib` | ✅ 通过 |
| Lint | `cargo clippy --locked --lib -- -D warnings` | ✅ 通过 |
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
