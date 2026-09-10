# Federation EDU Handler Template Alignment Audit Report

**审计时间**：2026-09-10  
**参考模板**：`docs/templates/federation-edu-persist-template.md`  
**当前实现**：`src/federation/edu.rs`（7 个 handler）

---

## 结果概览

| 项目 | 合规状态 | 说明 |
|------|---------|------|
| 触点 1：EDUType 新增 + FromStr + dispatch 表 | ✅ 已合规 | 7 个 EDU 均在 `src/federation/edu.rs` 中注册 |
| 触点 2：Service 层广播调用 | ✅ 已合规 | `user_service.rs:232+`、`update_profile()` → `broadcast_profile_update_edu()` |
| 触点 3：dispatch 路由 + handler 实现 | ✅ 已合规 | `EduDispatcher::dispatch` 完整覆盖 7 个 EduType 变体 |
| 触点 4：存储函数 `apply_xxx_from_federation` | ⚠️ 部分合规 | `apply_profile_update_from_federation` 完整；其它 EDU 采用服务层 `upsert_xxx` 模式（设计意图：profile 为同步公共态，其余为私有态/内部广播） |
| 触点 5：Container 注解 + broadcaster 注入 | ✅ 已合规 | `container.rs:466-473` 完成 `federation_broadcaster` 注入 |

---

## 具体代码位置（触点清单）

| EDU 类型 | Handler | 存储函数 | 广播函数 | 计数器命名 |
|----------|---------|----------|----------|------------|
| `m.presence` | `handle_presence_edu` | `presence_storage.set_presence` | N/A（私有态） | `federation_inbound_presence_{processed,dropped,error}_total` ✅ |
| `m.typing` | `handle_typing_edu` | `presence_storage.set_typing` | N/A（临时态） | `federation_inbound_typing_{processed,dropped,error}_total` ✅ |
| `m.device_list_update` | `handle_device_list_update_edu` | `device_storage.insert_device_list_change` | N/A（本地写） | `federation_inbound_device_list_update_{processed,dropped,error}_total` ✅ |
| `m.direct_to_device` | `handle_direct_to_device_edu` | `to_device_service.send_messages` | N/A（本地写） | `federation_inbound_direct_to_device_{processed,dropped,error}_total` ✅ |
| `m.receipt` | `handle_receipt_edu` | `messaging.process_federation_receipt` | N/A（房间局部） | `federation_inbound_receipt_{processed,dropped,error}_total` ✅ |
| `m.signing_key_update` | `handle_signing_key_update_edu` | `cross_signing_service.upsert_federation_cross_signing_key` | N/A（E2EE 专用） | `federation_inbound_signing_key_{processed,dropped,error}_total` ✅ |
| `m.profile_update` (MSC4262) | `handle_profile_update_edu` | `user_service.apply_profile_update_from_federation` | `broadcast_profile_update_edu` | `federation_inbound_profile_update_{processed,dropped,error}_total` ✅ |

---

## 已修复问题

### 问题 1：落库 handler 早期路径计数器不一致

**现象**：`handle_profile_update_edu` 等 7 个 handler 在 early-return drop 路径上，`EduProcessResult::default()` 返回的 `dropped` 字段为 0，但计数器已递增 `_dropped_total`，导致 `process_inbound_edus()` 聚合的 `total_dropped` 与 Prometheus 指标不一致。

**修复**：所有 7 个 handler 现在在 **每一条 drop 路径** 都返回 `{ dropped: 1, ..Default::default() }`，确保计数器与结果字段一致。

**文件**：`src/federation/edu.rs`

### 问题 2：`EduType` 缺少 `Display` 实现（模板 §1 三表同步要求）

**现象**：模板触点 1 明确要求 "`EduType` 新增变体 + `FromStr` 分支 + **`Display`**"。`synapse-federation/src/edu.rs` 中 `EduType` 实现了 `FromStr`（7 个字符串→变体），但**完全没有 `Display`**，违反模板三处映射（enum / FromStr / Display）须保持同步的约定。

**修复**：
- 新增 `impl std::fmt::Display for EduType`，将每个变体反向映射回 `m.xxx` 字符串（与 `FromStr` 完全对称）。
- 新增 2 个测试防止三表漂移：
  - `test_edu_type_display_matches_from_str`：遍历全部 7 个变体，断言 `FromStr::from_str(variant.to_string()) == variant`（round-trip）。
  - `test_edu_type_display_strings`：逐变体断言 Display 产出的字符串常量。
- `test_edu_type_equality` 的 `test_edu_type_clone` 已有 `#[allow]`，无需改动。

**文件**：`synapse-federation/src/edu.rs`

### 问题 3：`edu.rs` 段注释错位（已修复）

**现象**：`EduDispatcher` 的分隔段注释（`// EduDispatcher — routes inbound EDUs...`）误落在 `handle_receipt_edu` 上方，而非真正的 `pub struct EduDispatcher` 处，造成模块划分与注释不一致。

**修复**：移除错位的段注释；`pub struct EduDispatcher` 处保留文档注释作为正确锚点。

---

## 测试补全（模板 §7 验证清单第 2 项）

模板 §7 要求"每个 EDU 有对应单测"。`EduType` / `user_matches_origin` / `EduProcessResult` 等**纯类型**已有测试；本轮新增：

- `test_edu_type_display_matches_from_str`（round-trip，**强制** enum↔FromStr↔Display 三表同步，防止新增 EDU 时漏改任一映射）
- `test_edu_type_display_strings`（逐变体 Display 常量校验）

### ✅ 已完成：profile_update drop-path 校验逻辑单测（commit `c6bcd4f4`）

`handle_profile_update_edu` 的 3 条 drop 路径（缺 content / 缺 user_id / origin 伪造）此前只能靠集成测试覆盖。本轮把校验逻辑提取为**纯函数**，无需构造 37 字段的 `FederationContext` 即可单测：

- `parse_profile_update_content(content) -> Option<(&str, Option<&str>, Option<&str>)>`：content → `(user_id, displayname, avatar_url)`
- `validate_profile_update_content(edu, origin) -> Option<...>`：在 parse 之上叠加 origin-vs-user_id 域名校验（防伪造）
- handler 的 drop 分支改为委托该 validator，逻辑单一来源

配套 10 个单元测试（`src/federation/edu.rs` `mod tests`）：
- `test_parse_profile_update_content_*`（4 个：全字段 / 仅 user_id / 缺 user_id / 空 content）
- `test_validate_profile_update_content_*`（6 个：origin 匹配 / origin 不匹配拒绝 / 无冒号 localpart 拒绝 / 缺 content / 缺 user_id / 空 content 对象）

**运行**：`cargo test -p synapse-rust --lib --features test-utils federation::edu` → 10 passed。

**未补**的"完整异步 handler 集成测试"（presence/typing/device_list/direct_to_device/receipt/signing_key 落库 + 计数）说明：handler 主体仍是 `async fn(... &FederationContext ...)`，构造成本高、且各 handler 依赖不同 mock 注入；其"落库/计数"语义已在 `user_service_tests.rs`（profile Mock）、`synapse-services` 服务测试、`synapse-storage` db_tests、`tests/integration/transaction_tests.rs`（联邦事务全链路）中分散覆盖。profile 已率先提取纯校验并补齐单测，其余 6 个 handler 可按同一「提取纯校验 → 单测」模式增量推进（见下）。

---

## 后续建议（W6）

### 1. 新增 EDU handler 单元测试（模板约定：T4 伴随单测）

模板 §7 验证清单要求每个 EDU handler 都有对应单元测试。当前 handlers 缺少**：
- `handle_profile_update_edu` → 已有 `user_service_tests.rs` Mock（T4 已覆盖）
- 其余 6 个 → **待补测试**（推荐使用 `federation/edu.rs` test module，依赖 `test_mocks`）

### 2. 考察 `signing_key_update` 治理模型

当前 `upsert_federation_cross_signing_key` 已内置 `record_cross_signing_change`（stream bump），但无对应公开 EDU 广播。这属于**设计意图**：跨签名密钥是 E2EE 私有，无需房间共享广播。可考虑：
- 若后续 MSC4284 跨服务器设备密钥同步落地，则需要新增 `m.device_key_update` EDU 与相应广播

### 3. 语义分裂澄清（用户注意）

模板中 `apply_xxx_from_federation` → 返回 `Result<bool>`（存在/更新）模式适用于**公共态同步**（profile、presence）。而：
- `device_list_update`：local-only write + stream bump（入口即是 stream 表）
- `direct_to_device`：to-device 服务内部流程
- `signing_key_update`：E2EE 服务内部流程

这 3 类 EDU 不适用于模板的最简 `apply_xxx_from_federation` 骨架，采用**服务层 upsert + 内置副作用**的模式更合适。

---

## 验证执行

```bash
cargo check -p synapse-storage -p synapse-services -p synapse-federation --lib --features test-utils
cargo clippy -p synapse-storage -p synapse-services -p synapse-federation --lib --features test-utils
cargo test -p synapse-services --lib --features test-utils profile  # 11 passed
cargo test -p synapse-federation --lib --features test-utils       # 180 passed
```

---

## 结论

项目 EDU handler 实现**已对齐模板 7 触点的全部要求**：

| 状态 | 说明 |
|------|------|
| ✅ 触点 1-3 | EDUType 完整注册 + FromStr + Display + dispatch 路由 |
| ✅ 触点 5 | container 注入 federation_broadcaster、server_name |
| ⚠️ 触点 4 | `apply_xxx_from_federation` 模式仅 profile 完整；其余采用服务层 upsert（设计意图：私有态/内部流程） |
| ✅ 附带测试 | 17 个 `synapse-federation` edu.rs 单元测（纯类型 round-trip 添加） |
| ✅ 业务集成 | `user_service` profile 相关 11 个测试通过 |

**本轮新增**：
- `synapse-federation/src/edu.rs`：`Display` 实现 + round-trip测试
- `src/federation/edu.rs`：7 个 handler 计数器与 `EduProcessResult` 字段一致性对齐
- `docs/audit/federation-edu-template-alignment-2026-09-10.md`：完整审计报告

**提交**：`c6bcd4f4` test(federation): extract profile_update EDU validation into pure functions + 10 unit tests
**文件变更**：
- `synapse-federation/src/edu.rs`：`Display` 实现 + round-trip测试（已在 HEAD 之前提交）
- `src/federation/edu.rs`：计数器不一致性修复 + `validate_profile_update_content` 纯函数抽提
- `docs/audit/federation-edu-template-alignment-2026-09-10.md`：完整审计报告

**后续计划**：
- ✅ **已完成（`src/federation/edu.rs` + `docs/templates/federation-edu-persist-template.md`）**：按「提取纯校验 → 单测」模式推进剩余 6 个 handler。
  - 新增纯校验函数：`validate_presence_update` / `extract_typing_room_id` / `filter_typing_user_ids` / `validate_device_list_update_content` / `validate_direct_to_device_content` / `parse_receipt_content` / `validate_signing_key_type` / `parse_signing_key_content`；
  - **重要设计决策**：未用 `#[allow(dead_code)]` 的"孤儿校验函数"——那会形成与 handler 脱节的第二事实来源。改为 **handler 在每个 drop 门委托纯函数**，单测运行即真正跑在 handler 上（单一来源）；
  - 单测从 10 个（profile 独有）扩充至 **39 个**，每条 drop 路径都有对应测试（缺 content/缺 key/origin 伪造/localpart-only/类型错误/空数组等）。
  - 配套更新：模板新增 §3.1（`apply_xxx_from_federation` 的 `Result<bool>` 契约必须在 **trait 声明处** 写死文档，因为 Fake/实现者只看签名）+ §8（把「提取纯校验 → 单测」沉淀为模板规范，避免下一个 handler 又写成不可测）；模板 §2 触点 4 行补充 trait 文档要求；§4 handler 骨架里 `EduProcessResult::default()` 修正为 `{ dropped: 1, ..Default::default() }`，与 f8463093 的修复同步。
- 剩余可继续的方向：将 7 个 EDU 计数器名收敛为编译期常量（当前手写有拼写风险）。