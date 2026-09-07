# synapse-rust API 测试体系方案

**版本**: 1.0
**作者**: API Testing Expert
**日期**: 2026-09-01
**状态**: 草稿 v1（待评审）
**适用项目**: synapse-rust Matrix Homeserver
**关联文档**: `api-test-strategy-2026-09-01-part2.md`

---

## 🎯 一、设计目标

| 目标 | 当前状态 | 目标状态 |
|------|---------|---------|
| **功能覆盖率** | 543 用例 / ~52 endpoints | 95%+ endpoint / 每个 endpoint ≥ 3 场景 |
| **执行时间** | 单跑约 20 分钟 | 核心套件 < 5 分钟 / 全量 < 15 分钟 |
| **失败定位** | 需手工 JSONL 分析 | 失败直接定位 endpoint + 输入 + 期望 |
| **CI 集成** | 仅手动触发 | PR 必跑 / main 必跑 / nightly 全量 |
| **安全覆盖** | 无自动化 | OWASP API Top 10 100% |
| **性能基线** | 无 | p95 < 200ms / error < 0.1% / 10x burst |
| **契约测试** | 无 | OpenAPI 双向校验 |

## 📐 二、测试金字塔分层

```
┌─────────────┐
│  E2E 烟雾    │   5%  — 健康检查 + 5 个核心用户旅程
│   测试       │
├─────────────┤
│  契约测试    │  10%  — OpenAPI 双向校验 / 版本兼容
├─────────────┤
│  API 集成    │  35%  — 现有 api-integration_test.sh 增强
│   测试       │
├─────────────┤
│  组件层      │  30%  — 路由 handler 单元测试 (mocked)
├─────────────┤
│  单元测试    │  20%  — service / storage / 业务逻辑
└─────────────┘
```

## 🔍 三、API 端点盘点

### 3.1 现有覆盖矩阵

| 类别 | Endpoints | 测试状态 | 关键缺失 |
|------|-----------|---------|---------|
| **认证 / 登录** | `/v3/login` `/v3/logout` `/v3/refresh` | ✅ 部分 | MFA / UIA 流程 |
| **用户管理** | `/v3/account/*` `/v3/profile/*` | ✅ 部分 | 头像上传 / 3PID 绑定 |
| **设备管理** | `/v3/devices` `/v3/devices/{id}` | ✅ 通过 | 跨用户横向测试 |
| **房间生命周期** | `/v3/createRoom` `/v3/rooms/{id}` | ✅ 通过 | 加密房间 / 软删 |
| **消息收发** | `/v3/rooms/{id}/send` `/v3/rooms/{id}/messages` | ✅ 通过 | 消息事件穷举 |
| **同步 (Sliding Sync)** | `/v3/sync` | ⚠️ 部分 | 大数据量增量 |
| **E2EE 密钥** | `/v3/keys/upload` `/v3/keys/query` `/v3/keys/claim` | ⚠️ 部分 | 签名验证 |
| **联邦 (Federation)** | `/v1/federation/*` `/_matrix/federation/*` | ❌ 缺 | 真实联邦 |
| **管理员 API** | `/_synapse/admin/v1/*` | ✅ 部分 | 权限矩阵 |
| **OpenID / SSO** | `/v3/sso/*` | ❌ 缺 | 完整 OIDC 流 |
| **媒体 (Media)** | `/v3/media/*` | ⚠️ 部分 | 大文件 / 缩略图 |
| **Push 推送** | `/v3/push/*` | ❌ 缺 | APNs/FCM 集成 |
| **Search 搜索** | `/v3/search` | ❌ 缺 | 性能 |
| **Account Data** | `/v3/user/{id}/account_data` | ⚠️ 部分 | 命名空间 |
| **Typing/Receipts** | `/v3/rooms/{id}/typing` `/receipt` | ❌ 缺 | 实时性 |
| **Appservice** | `/_matrix/app/v1/*` | ❌ 缺 | 注册协议 |
| **Third-party 协议** | `/v3/thirdparty/*` | ⚠️ 部分 | IRC/SMS 网关 |
| **Spaces 空间** | `/v3/spaces/*` | ⚠️ 部分 | 层级遍历 |

### 3.2 优先级（按业务影响 × 风险）

| 优先级 | 类别 | 原因 |
|--------|------|------|
| P0 | 认证、Token、Rooms、Messages | 核心用户旅程，失败 = 整体不可用 |
| P0 | Admin 权限 | 安全敏感，权限越权 = 数据泄露 |
| P1 | Federation、E2EE | 跨服务/Mesh 失败 = 业务中断 |
| P1 | Sliding Sync、Push | 性能敏感，移动端体验 |
| P2 | Appservice、Third-party、Spaces | 高级功能，影响部分用户 |
| P3 | Receipts、Typing、Profile | 低风险，渐进覆盖 |

## 🧪 四、各层测试详细设计

### 4.1 单元测试（Component Layer）

**目标**: 隔离测试 service / handler 逻辑，不依赖 HTTP 层

**位置**: `synapse-*/src/**/tests.rs`

**样例模板** (Rust):

```rust
#[tokio::test]
async fn login_with_invalid_password_returns_401() {
    let svc = TestEnv::new().await;
    let user = svc.create_test_user("alice", "correct_password").await;

    let result = svc.auth_service.login("alice", "wrong_password").await;

    assert!(matches!(result, Err(ApiError::Unauthorized(_))));
    assert_eq!(result.unwrap_err().code(), "M_FORBIDDEN");
}
```

**覆盖目标**: 每个 service crate ≥ 70% 行覆盖；handler ≥ 50% 行覆盖

**测试运行**: `cargo test --workspace`

### 4.2 集成测试（API Integration Layer）

**位置**: `docker/deploy/api-integration_test.sh`（已有）+ 新增 `tests/api/`

**升级方向**:

1. **拆分 Profile**: `core` / `full` / `nightly` — 按场景执行
2. **角色参数化**: `TEST_ROLE=user|admin|super_admin` — 自动切换期望
3. **可重复性**: 每次运行使用 `RUN_ID` 隔离房间名（已实现）
4. **结构化结果**: 保留 JSONL + 添加 `JUnit XML` 输出供 CI 解析

**样例新增** (`tests/api/test_thirdparty.sh`):

```bash
#!/bin/bash
# @group thirdparty
# @profile full
# 描述：Third-party protocols 全场景

PROTOCOL="${1:-irc}"
BASE="http://localhost:8008/_matrix/client/v3"

echo "→ List third-party protocols"
http_json GET "$BASE/thirdparty/protocols" "$TOKEN"
assert_success_object "List Protocols"

echo "→ Get specific protocol ($PROTOCOL)"
http_json GET "$BASE/thirdparty/protocol/$PROTOCOL" "$TOKEN"
expect_2xx_or_skip_404 "Get Protocol"
```

**运行入口**:

```bash
./tests/api/run.sh --profile core          # 5 分钟
./tests/api/run.sh --profile full          # 15 分钟
./tests/api/run.sh --profile nightly       # 30 分钟 + 性能
```

### 4.3 契约测试（Contract Layer）

**目标**: OpenAPI 规范双向校验 — 服务实现 ↔ 规范文档

**工具**: `schemathesis` (Python) 或 `dredd` (Node)

**位置**: `docs/openapi/{client,server,admin,federation}.yaml`

**测试用例**:

1. **Schema 校验**: 任何响应必须匹配 OpenAPI 定义的 schema
2. **Required Fields**: 必填字段缺失即 fail
3. **Type 校验**: 字段类型不一致即 fail
4. **Status Code Matrix**: 文档声明的所有状态码必须可触发
5. **向后兼容**: 旧客户端 SDK 仍能工作

**样例** (`tests/contract/test_openapi_compliance.py`):

```python
import schemathesis

schema = schemathesis.openapi.from_path("docs/openapi/client.yaml")

@schema.parametrize()
def test_api_compliance(case):
    case.call_and_validate()
```

**新增 OpenAPI 文档**: 当前项目**没有完整的 OpenAPI 规范**（已有部分 `api_doc/*.rs` 中的 doc 注释），需要从代码生成。

### 4.4 E2E 烟雾测试

**目标**: 真实用户场景，5 个核心 journey

**样例旅程** (5 分钟可跑完):

1. **注册 → 登录 → 创建房间 → 发送消息 → 登出**
2. **联邦密钥发现 → 服务器签名验证**
3. **E2EE 密钥上传 → 设备间密钥交换**
4. **管理员：禁用用户 → 撤销 Token**
5. **OAuth/OIDC: 重定向 → 回调 → JWT 验证**

**工具**: `playwright` (Node) 或 `k6` (Go) — 选 k6 因 Go 性能 + 跨平台

---

**续**: 见 `api-test-strategy-2026-09-01-part2.md`（性能测试、安全测试、CI/CD、工具栈、Roadmap、风险）
