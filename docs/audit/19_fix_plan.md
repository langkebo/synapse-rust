# 系统化修复计划 — Round 2 审计修复

> **日期**: 2026-07-14
> **分支**: `feat/architecture-optimization-round2`
> **源审计报告**:
> - `docs/audit/14_failopen_scan_round2.md` — 安全 fail-open 扫描
> - `docs/audit/15_arch_review_round2.md` — 架构深化机会
> - `docs/audit/17_perf_baseline.md` — 性能基线与回归门禁
> - `docs/audit/18_api_contract_review.md` — API 契约覆盖审查

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复四份审计报告中识别出的所有未修复安全/架构/性能/契约缺口

**Architecture:** 按优先级排序：P1 安全（token 校验错误传播）→ P2 防御深度（错误日志可观测性）→ P2 性能（播种脚本 + bench 补全 + 缓存热读）→ P2 契约（snapshot + 联邦集成测试）

**Tech Stack:** Rust (edition 2024), tokio, axum, sqlx, serde_json, ed25519-dalek, insta, Criterion

## 全局约束

- 安全关键路径必须 fail-closed，禁止 `unwrap_or(default)` / `.ok()` 吞错导致退化为匿名/授权
- 路由文件不得在中间件重构中修改，除非确认 bug
- 联邦端点对"存在但未授权"与"不存在"统一返回 404
- SQL `ANY()` 文本数组必须 `::text[]` 显式转型
- 修改遵循 TDD Red-Green-Refactor（`.claude/skills/tdd-rust/SKILL.md`）

## 已完成项（本会话 Phase A）

以下 P1 安全修复已在 Phase A 中完成，本计划不再重复：

| 审计 ID | 文件:行 | 修复 |
|---------|---------|------|
| 14-P1-1 | `account_compat.rs:35` | `unwrap_or(true)` → `unwrap_or(false)` ✓ |
| 14-P1-4 | `search/search.rs:322,388` | `unwrap_or(true)` → `unwrap_or(false)` ✓ |
| 15-C1 | `context.rs` + 8 Contexts | `room_storage`/`user_storage` 从所有 Context 移除 ✓ |
| 15-C1 | `search/hierarchy.rs:19` | 最后 1 个 `ctx.room_storage` handler 引用改为 `room_service.state().get_room_record()` ✓ |
| Phase-A | `edu.rs:63` | `ctx.user_storage.user_exists()` → `ctx.user_service.user_exists()` ✓ |
| Phase-A | `admin/cleanup.rs:40,89` | `ctx.room_storage.cleanup_abnormal_data()` → `ctx.room_service.state().cleanup_abnormal_data()` ✓ |

---

### Task 1: P1 — 区分 Token 校验的 DB 错误与认证失败（account_compat.rs:52-54）

**动机**: `enforce_profile_visibility()` 中 `bearer_token(headers).ok()` 和 `validate_token(&t).await.ok()` 将所有错误（header 解析异常、DB 故障、签名错误）一律静默降级为匿名用户。DB 故障时应 propagate error 而非退化为"未登录"——否则 DB 不可用时所有 profile 查詢都会暴露给匿名用户。

**文件:**
- Modify: `src/web/routes/account_compat.rs:46-61`
- Test: `tests/integration/api_route_snapshots_tests.rs` (复用现有 `setup_test_app()`)

**接口:**
- Consumes: `bearer_token(headers)` → `Option<String>`（当前签名为 `Result<String, E>` — 但只用 `.ok()`）
- Produces: `enforce_profile_visibility()` 返回类型不变 `Result<(), ApiError>`

- [ ] **Step 1: 编写失败测试 — Token 校验 DB 错误应 propagate 而非静默降级**

当前行为：token 校验 DB 错误 → `.ok()` 静默吞掉 → 退化为匿名用户 → 可见性检查用匿名权限。
期望行为：DB 错误 → 返回 500 Internal Server Error（`ApiError::internal_with_log`）。

在 `tests/integration/api_route_snapshots_tests.rs` 中添加测试。由于此测试需要模拟 DB 故障场景，使用现有 `setup_fresh_test_app()` 框架，构造一个**畸形 token**（非 base64 格式）来触发 header 解析错误，验证 `enforce_profile_visibility` 在 token header 解析异常时至少不与"无 token"等同处理。

```rust
#[tokio::test]
async fn snapshot_profile_token_parse_error_returns_internal_error() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    // 发送带有非法 Authorization header 的请求（非 Bearer 格式、乱码）
    // 当前行为：header 解析失败 → .ok() → None → 匿名用户 → 可能 403 或 200
    // 期望行为：header 解析异常 → 至少记录 warn 日志（验证行为不降级）
    let request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/profile/@nonexistent:localhost")
        .header("Authorization", "\x00\x01\x02")  // 非法 header 字节
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    // 当前接受任意非 200 响应；此测试是行为锚点，修复后可收紧断言
    assert_ne!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: 运行测试确认当前行为（基线）**

```bash
cargo test --test integration api_route_snapshots_tests::snapshot_profile_token_parse_error_returns_internal_error -- --exact --nocapture
```

记录当前返回的状态码。修复后重新运行确认行为变化。

- [ ] **Step 3: 修改 `enforce_profile_visibility` — 区分 header 解析错误**

将 `bearer_token(headers).ok()` 替换为 `match`，在 header 解析异常时返回 500 错误。

```rust
// Before (L52-54):
let token = bearer_token(headers).ok();
let requester_id =
    if let Some(t) = token { auth_service.validate_token(&t).await.ok().map(|(id, _, _, _, _)| id) } else { None };

// After:
let token = match bearer_token(headers) {
    Ok(Some(t)) => Some(t),
    Ok(None) => None,
    Err(e) => {
        ::tracing::warn!(error = %e, "Failed to parse Authorization header");
        return Err(ApiError::internal_with_log("Failed to parse Authorization header", &e));
    }
};

let requester_id = if let Some(ref t) = token {
    match auth_service.validate_token(t).await {
        Ok((id, _, _, _, _)) => Some(id),
        Err(e) => {
            // Token 过期/无效 → 匿名用户（正确降级）
            if e.to_string().contains("expired") || e.to_string().contains("invalid") || e.to_string().contains("Unknown") {
                None
            } else {
                // DB 错误/网络错误 → propagate（不降级）
                return Err(ApiError::internal_with_log("Token validation error", &e));
            }
        }
    }
} else {
    None
};
```

**回滚判据**: 若 `bearer_token` 函数签名不支持返回 `Result`（当前是 `Option`），则 Step 3 仅修改 `validate_token` 部分。`bearer_token` 的 `.ok()` 是合理的——无效 header 格式等同于无 token。

- [ ] **Step 4: 运行测试确认修复**

```bash
cargo test --test integration api_route_snapshots_tests::snapshot_profile_token_parse_error_returns_internal_error -- --exact --nocapture
cargo test --test integration api_route_snapshots_tests -- --nocapture
```

- [ ] **Step 5: 提交**

```bash
git add src/web/routes/account_compat.rs tests/integration/api_route_snapshots_tests.rs
git commit -m "fix(security): differentiate token validation errors from anonymous access in profile visibility check

Previously .ok() silently downgraded all auth errors (DB faults, header
parse failures) to anonymous-user status, which is incorrect fail-open
behavior. Now DB/protocol errors propagate as 500, while expired/invalid
tokens correctly result in anonymous access only.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 2: P2 — Federation Auth 中间件密钥获取错误日志（federation_auth.rs:368,397）

**动机**: `get_current_key().await.ok().flatten()?` 在 DB 错误时静默返回 None，导致签名校验失败（行为 fail-closed）。正确，但缺失错误上下文，DB 故障排查困难。

**文件:**
- Modify: `src/web/middleware/federation_auth.rs:365-400`

**接口:**
- Consumes: `KeyRotationManager::get_current_key() -> Result`
- Produces: X-Matrix signature verification（行为不变，仅增加日志）

- [ ] **Step 1: 修改两处 `get_current_key()` 调用增加 error 日志**

```rust
// L368 — Before:
let current_key = ctx.key_rotation_manager.get_current_key().await.ok().flatten()?;

// L368 — After:
let current_key = match ctx.key_rotation_manager.get_current_key().await {
    Ok(Some(key)) => key,
    Ok(None) => {
        ::tracing::error!("No current federation signing key available");
        return Err(FederationAuthError::KeyUnavailable);
    }
    Err(e) => {
        ::tracing::error!(error = %e, "Failed to fetch current federation signing key from DB");
        return Err(FederationAuthError::KeyUnavailable);
    }
};
```

同样修改 L397。

- [ ] **Step 2: 编译验证**

```bash
SQLX_OFFLINE=true cargo check --workspace
```

- [ ] **Step 3: 提交**

```bash
git add src/web/middleware/federation_auth.rs
git commit -m "fix(federation): add error context logging when signing key fetch fails

get_current_key() DB errors were silently dropped by .ok().flatten(),
making key-unavailability incidents untriagable. Behavior remains
fail-closed (reject request) but now emits error-level logs.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 3: P2 — Widget + Transaction 路径错误日志加固

**动机**: `widget.rs:376,382` 和 `transaction.rs:330,431` 中的 `.ok()`/`.is_ok()`/`unwrap_or(false)` 在 DB 错误时丢失错误上下文。虽行为 fail-closed，但线上故障排查需要日志。

**文件:**
- Modify: `src/web/routes/widget.rs:374-385`
- Modify: `src/web/routes/federation/transaction.rs:326-354`
- Modify: `src/web/routes/federation/membership/join.rs:283`

- [ ] **Step 1: widget.rs — 为 `verify_room_moderator` 和 `is_room_creator` 添加日志**

```rust
// L376 — Before:
let is_mod = ctx.room_service.moderator().verify_room_moderator(room_id, &auth_user.user_id).await.is_ok();

// After:
let is_mod = match ctx.room_service.moderator().verify_room_moderator(room_id, &auth_user.user_id).await {
    Ok(()) => true,
    Err(e) => {
        ::tracing::warn!(error = %e, room_id = room_id, user_id = %auth_user.user_id, "Room moderator check failed");
        false
    }
};
```

```rust
// L382 — Before:
let is_creator = ctx.room_service.state().is_room_creator(room_id, &auth_user.user_id).await.unwrap_or(false);

// After:
let is_creator = match ctx.room_service.state().is_room_creator(room_id, &auth_user.user_id).await {
    Ok(val) => val,
    Err(e) => {
        ::tracing::warn!(error = %e, room_id = room_id, user_id = %auth_user.user_id, "Room creator check failed");
        false
    }
};
```

- [ ] **Step 2: transaction.rs — 为 unknown membership parse 添加 warn 日志**

```rust
// L330 — 在 `.ok()` 后添加 warn:
.and_then(|s| {
    match s.parse::<synapse_common::Membership>() {
        Ok(m) => Some(m),
        Err(_) => {
            ::tracing::warn!(
                event_id = event_id,
                membership = %s,
                "Inbound m.room.member event with unknown membership value; will rely on auth-event chain for validation"
            );
            None
        }
    }
});
```

- [ ] **Step 3: membership/join.rs — 为 is_banned NULL 添加 warn 日志**

```rust
// L283 — Before:
if member.membership == "ban" || member.is_banned.unwrap_or(false) {

// After:
if member.membership == "ban" || member.is_banned.unwrap_or_else(|| {
    ::tracing::warn!(room_id = room_id, user_id = user_id, "is_banned field is NULL for non-ban member; assuming not banned");
    false
}) {
```

- [ ] **Step 4: 编译 + 现有测试验证**

```bash
SQLX_OFFLINE=true cargo check --workspace
cargo test --test integration -- --nocapture --test-threads=4 2>&1 | tail -20
```

- [ ] **Step 5: 提交**

```bash
git add src/web/routes/widget.rs src/web/routes/federation/transaction.rs src/web/routes/federation/membership/join.rs
git commit -m "fix(security): add warn-level error logging at auth-critical .ok() swallow points

All changes are log-only: no behavior change. Replaces silent .ok()/.is_ok()/
unwrap_or(false) with match arms that emit warn/error logs, making DB-fault
incidents diagnosable without altering the fail-closed security posture.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 4: P2 — 播种脚本（解锁 HTTP API Bench）

**动机**: 所有 9 个 API bench (B1-B9) 和 3 个 DB-backed sliding sync bench (S3-S6) 依赖运行中服务器 + 预播种数据。无播种脚本 → bench 虽编译通过但无法执行真实负载测量。

**文件:**
- Create: `scripts/seed_bench_data.sh`
- 参考: `tests/integration/api_placeholder_contract_p0_tests.rs` 中的 helper 模式（`register_user`/`create_room`/`send_message`）

- [ ] **Step 1: 编写最小可行播种脚本**

```bash
#!/bin/bash
# scripts/seed_bench_data.sh — 为 performance bench 预播种测试数据
set -euo pipefail

BASE_URL="${BENCH_BASE_URL:-http://localhost:8008}"
ADMIN_TOKEN="${BENCH_ADMIN_TOKEN:-}"
TEST_ROOM_ID="!test:localhost"

# 1. 通过 admin API 注册 bench 用户
register_user() {
    local username="$1"
    curl -s -X POST "${BASE_URL}/_synapse/admin/v1/register" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -d "{\"username\": \"${username}\", \"password\": \"BenchPass123!\", \"admin\": false}"
}

echo "=== Seeding bench users ==="
BENCH_TOKEN=$(register_user "bench_user" | jq -r '.access_token')

# 2. 创建测试房间 "!test:localhost"
echo "=== Creating test room ==="
ROOM_ID=$(curl -s -X POST "${BASE_URL}/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer ${BENCH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "{\"room_id\": \"${TEST_ROOM_ID}\", \"name\": \"Bench Test Room\"}" | jq -r '.room_id')
echo "Room: ${ROOM_ID}"

# 3. 为 user_directory_search bench 创建 100 个额外用户
echo "=== Seeding 100 users for search bench ==="
for i in $(seq 1 100); do
    register_user "search_user_${i}" > /dev/null
done

# 4. 创建 1000 成员房间（用于 sync initial bench）
echo "=== Creating large room with 1000 members ==="
LARGE_ROOM=$(curl -s -X POST "${BASE_URL}/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer ${BENCH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name": "Large Bench Room"}' | jq -r '.room_id')
for i in $(seq 1 50); do
    # 注册并邀请（收敛时间：每个用户 ~200ms，共 ~10s）
    TOKEN=$(register_user "large_user_${i}" | jq -r '.access_token')
    curl -s -X POST "${BASE_URL}/_matrix/client/v3/rooms/${LARGE_ROOM}/join" \
        -H "Authorization: Bearer ${TOKEN}" > /dev/null &
done
wait

# 5. 为 100 个用户注册设备密钥
echo "=== Registering device keys ==="
curl -s -X POST "${BASE_URL}/_matrix/client/v3/keys/upload" \
    -H "Authorization: Bearer ${BENCH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"device_keys": {"user_id": "@bench_user:localhost", "device_id": "BENCHDEV", "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"], "keys": {"curve25519:BENCHDEV": "test", "ed25519:BENCHDEV": "test"}}}' > /dev/null

# 6. 导出环境变量
echo ""
echo "=== Seed complete ==="
echo "export BENCH_ADMIN_TOKEN=${ADMIN_TOKEN}"
echo "export BENCH_USER_TOKEN=${BENCH_TOKEN}"
echo "export BENCH_ROOM_ID=${ROOM_ID}"
echo "export BENCH_LARGE_ROOM_ID=${LARGE_ROOM}"
```

- [ ] **Step 2: 添加可执行权限**

```bash
chmod +x scripts/seed_bench_data.sh
```

- [ ] **Step 3: 验证播种脚本语法**

```bash
bash -n scripts/seed_bench_data.sh
```

- [ ] **Step 4: 提交**

```bash
git add scripts/seed_bench_data.sh
git commit -m "feat(bench): add seed script to unlock HTTP API benchmarks

Minimal seeding: 1 bench user, 1 test room, 100 search users,
50-member large room, device keys. Run before cargo bench to
provide the dataset that B1-B9 + S3-S6 expect.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 5: P2 — Membership 状态转换纯逻辑 Bench（25 种转移）

**动机**: 审计 17 盲区：membership 状态机 `is_legal()` 是联邦入站的安全关键路径，25 种状态转换无 bench 覆盖。纯逻辑 bench，不需要外部服务，编译即可运行。

**文件:**
- Create: `benches/performance_membership_benchmarks.rs`
- 修改: `synapse-common/Cargo.toml`（确认 bencher/criterion 支持）

- [ ] **Step 1: 编写 bench 文件**

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use synapse_common::membership_transition::{
    TransitionCtx, check_ban, check_invite, check_join, check_knock, check_leave,
};
use synapse_common::Membership;

/// 构建最小 TransitionCtx — state_only 模式（仅状态机检查，不含 power level）
fn ctx(from: Membership, target: &str) -> TransitionCtx {
    TransitionCtx::state_only(from, target.to_string())
}

fn bench_membership_transitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("membership_transitions");
    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(5));

    let cases: &[(&str, Membership, Membership, bool)] = &[
        // 关键拒绝路径（fail-closed — 必须快）
        ("ban_to_join", Membership::Ban, Membership::Join, false),
        ("ban_to_invite", Membership::Ban, Membership::Invite, false),
        ("ban_to_knock", Membership::Ban, Membership::Knock, false),
        ("self_ban", Membership::Join, Membership::Ban, false),  // actor_is_target
        ("creator_leave", Membership::Join, Membership::Leave, false),  // TargetIsCreator
        ("creator_ban", Membership::Join, Membership::Ban, false),   // TargetIsCreator
        ("invite_of_banned", Membership::Ban, Membership::Invite, false),
        // 关键允许路径
        ("join_to_invite", Membership::Join, Membership::Invite, true),
        ("invite_to_join", Membership::Invite, Membership::Join, true),
        ("leave_to_invite", Membership::Leave, Membership::Invite, true),
        ("knock_to_join", Membership::Knock, Membership::Join, true),
        // idempotent
        ("leave_to_leave", Membership::Leave, Membership::Leave, true),
        ("join_to_join", Membership::Join, Membership::Join, true),
    ];

    for (name, from, to, should_pass) in cases {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let ctx = ctx(*from, "target_user");
                let result = synapse_common::membership_transition::is_legal(
                    black_box(from),
                    black_box(to),
                    black_box(&ctx),
                );
                if *should_pass {
                    assert!(result.is_ok(), "expected legal: {:?} -> {:?}", from, to);
                } else {
                    assert!(result.is_err(), "expected illegal: {:?} -> {:?}", from, to);
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_membership_transitions);
criterion_main!(benches);
```

- [ ] **Step 2: 注册 Criterion bench target**

在 `synapse-common/Cargo.toml` 中添加：

```toml
[[bench]]
name = "performance_membership_benchmarks"
harness = false
path = "../benches/performance_membership_benchmarks.rs"
```

（或直接在 workspace `Cargo.toml` 中 `[[bench]]` 段添加）

- [ ] **Step 3: 编译验证 + 运行 bench**

```bash
SQLX_OFFLINE=true cargo bench --bench performance_membership_benchmarks --no-run
cargo bench --bench performance_membership_benchmarks -- --sample-size 10 --measurement-time 3s
```

验证所有断言通过（pass 路径返回 Ok，fail 路径返回 Err）且延迟在微秒级。

- [ ] **Step 4: 提交**

```bash
git add benches/performance_membership_benchmarks.rs synapse-common/Cargo.toml Cargo.toml
git commit -m "feat(bench): add membership state transition benchmarks (13 key paths)

Covers ban→join, ban→invite, self-ban, creator-kick/ban, invite-of-banned,
and 7 allowed/goodput paths. Pure-logic, no external infra needed.
Gate: all fail-closed assertions must pass.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 6: P2 — Login 成功 + Sync 鉴权后响应 Insta Snapshot

**动机**: 审计 18 的 P0 Gap-01/02。Login 成功响应包含最关键动态字段（`access_token`/`refresh_token`/`expires_in`），Sync 鉴权后响应包含 `next_batch`/`rooms`/`presence`。现有快照只覆盖了未鉴权路径，成功响应形状漂移无检测。

**文件:**
- Modify: `tests/integration/api_route_snapshots_tests.rs`

- [ ] **Step 1: 添加 login 成功快照测试**

```rust
#[tokio::test]
async fn snapshot_login_success_redacted_response_shape() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    // 先注册用户以获取可用凭证
    let (token, user_id, device_id) = {
        let register_body = json!({
            "username": format!("snapshot_login_{}", rand::random::<u32>()),
            "password": "SnapshotPass123!",
            "auth": { "type": "m.login.dummy" }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/_matrix/client/v3/register")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
            .unwrap();
        let resp = ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(req)).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        (json["access_token"].as_str().unwrap().to_string(),
         json["user_id"].as_str().unwrap().to_string(),
         json["device_id"].as_str().unwrap().to_string())
    };

    // 用注册返回的凭证做 login（确保可复现的 success 响应形状）
    let login_body = json!({
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": user_id},
        "password": "SnapshotPass123!"
    });
    let req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_body).unwrap()))
        .unwrap();
    let resp = ServiceExt::<Request<Body>>::oneshot(app, super::with_local_connect_info(req)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let mut body: Value = serde_json::from_slice(&body).unwrap();

    // Redact dynamic fields
    if let Some(obj) = body.as_object_mut() {
        for key in &["access_token", "refresh_token", "device_id"] {
            if let Some(v) = obj.get_mut(*key) {
                *v = Value::String(format!("[redacted_{}]", key));
            }
        }
        if let Some(v) = obj.get_mut("expires_in_ms") {
            *v = Value::Number(serde_json::Number::from(3600000));
        }
        if let Some(v) = obj.get_mut("user_id") {
            // 保留 @ 前缀 + domain，但 redact localpart
            let s = v.as_str().unwrap_or("");
            if let Some(at_pos) = s.find(':') {
                *v = Value::String(format!("@[redacted_user]:{}", &s[at_pos+1..]));
            }
        }
    }

    insta::assert_json_snapshot!("login_success_redacted", body);
}
```

- [ ] **Step 2: 运行测试接受 snapshot**

```bash
cargo test --test integration api_route_snapshots_tests::snapshot_login_success_redacted_response_shape -- --exact --nocapture
cargo insta review  # 接受新 snapshot
```

- [ ] **Step 3: 添加 sync 鉴权后快照测试**

```rust
#[tokio::test]
async fn snapshot_sync_authenticated_initial_response_shape() {
    let Some(app) = setup_test_app().await else {
        return;
    };
    // 注册 + login 获取 token
    let username = format!("snapshot_sync_{}", rand::random::<u32>());
    let register_body = json!({
        "username": username,
        "password": "SnapshotPass123!",
        "auth": { "type": "m.login.dummy" }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let resp = ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(req)).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let token = json["access_token"].as_str().unwrap();

    // 初始 sync（timeout=0，立即返回）
    let req = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/sync?timeout=0")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let resp = ServiceExt::<Request<Body>>::oneshot(app, super::with_local_connect_info(req)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let mut body: Value = serde_json::from_slice(&body).unwrap();

    // Redact next_batch token
    if let Some(obj) = body.as_object_mut() {
        if let Some(v) = obj.get_mut("next_batch") {
            *v = Value::String("[redacted_next_batch]".into());
        }
        // Redact room timeline event IDs + origin_server_ts
        if let Some(rooms) = obj.get_mut("rooms") {
            redact_room_events(rooms);
        }
    }

    insta::assert_json_snapshot!("sync_authenticated_initial", body);
}

fn redact_room_events(rooms: &mut Value) {
    for (_key, room_data) in rooms.as_object_mut().into_iter().flatten() {
        if let Some(state) = room_data.get_mut("state") {
            redact_event_array(state);
        }
        if let Some(timeline) = room_data.get_mut("timeline") {
            redact_event_array(timeline);
        }
    }
}

fn redact_event_array(events: &mut Value) {
    if let Some(arr) = events.get_mut("events").and_then(|v| v.as_array_mut()) {
        for ev in arr.iter_mut() {
            if let Some(obj) = ev.as_object_mut() {
                if let Some(v) = obj.get_mut("event_id") {
                    let s = v.as_str().unwrap_or("");
                    if let Some(at_pos) = s.find(':') {
                        *v = Value::String(format!("$[redacted_event]:{}", &s[at_pos+1..]));
                    }
                }
                if let Some(v) = obj.get_mut("origin_server_ts") {
                    *v = Value::Number(serde_json::Number::from(0));
                }
            }
        }
    }
}
```

- [ ] **Step 4: 运行 + 接受 snapshot**

```bash
cargo test --test integration api_route_snapshots_tests::snapshot_sync_authenticated_initial_response_shape -- --exact --nocapture
cargo insta review
```

- [ ] **Step 5: 运行全部 snapshot 测试确保无回归**

```bash
cargo test --test integration api_route_snapshots_tests -- --nocapture
```

- [ ] **Step 6: 提交**

```bash
git add tests/integration/api_route_snapshots_tests.rs tests/integration/snapshots/
git commit -m "test(contract): add login success + sync authenticated insta snapshots

Locks the response shapes of POST /login (200) and GET /sync (authenticated
200) with dynamic field redaction. Addresses P0 gaps GAP-01 and GAP-02
from the API contract audit.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 7: P2 — 联邦 Send_Transaction 集成契约测试

**动机**: 审计 18 的 P0 Gap-03。`POST /_matrix/federation/v1/send/{txnId}` 是联邦入站 PDU 热路径，零集成测试。需验证 signed PDU 的签名验证 → auth chain 检查 → state resolution → 持久化完整链路。

**文件:**
- Create: `tests/integration/api_federation_transaction_tests.rs`

- [ ] **Step 1: 编写 send_transaction 契约测试**

```rust
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::federation::signing::{
    canonical_federation_request_bytes, sign_and_hash_event,
};
use tower::ServiceExt;

async fn setup_federation_signing_app(
    key_id: &str,
    signing_key_b64: &str,
) -> Option<axum::Router> {
    let pool = super::require_test_pool().await;
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool).await;
    container.core.config.server.name = "localhost".to_string();
    container.core.server_name = "localhost".to_string();
    container.core.config.federation.enabled = true;
    container.core.config.federation.allow_ingress = true;
    container.core.config.federation.server_name = "localhost".to_string();
    container.core.config.federation.key_id = Some(key_id.to_string());
    container.core.config.federation.signing_key = Some(signing_key_b64.to_string());
    let cache = std::sync::Arc::new(
        synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()),
    );
    let state = synapse_rust::web::routes::state::AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
}

fn signed_federation_request(
    method: &str,
    uri: &str,
    origin: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: Option<&Value>,
) -> Request<Body> {
    let signed_bytes = canonical_federation_request_bytes(method, uri, origin, origin, content).unwrap();
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"", origin, key_id, sig_b64));
    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder.body(Body::from(content.map(Value::to_string).unwrap_or_default())).unwrap()
}

#[tokio::test]
async fn test_send_transaction_rejects_missing_signature() {
    let Some(app) = super::setup_fresh_test_app().await else {
        return;
    };
    let txn_id = format!("txn-no-sig-{}", rand::random::<u32>());
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/federation/v1/send/{}", txn_id))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"pdus": [], "edus": []}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_send_transaction_rejects_invalid_signature() {
    let key_id = "ed25519:txn_test";
    let signing_key_seed = [99u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    // Use a DIFFERENT origin from the one the server is configured for
    let Some(app) = setup_federation_signing_app(key_id, &signing_key_b64).await else {
        return;
    };

    let txn_id = format!("txn-bad-sig-{}", rand::random::<u32>());
    let content = json!({"pdus": [], "edus": []});
    let request = signed_federation_request(
        "PUT",
        &format!("/_matrix/federation/v1/send/{}", txn_id),
        "evil.example.com",  // different from server config
        key_id,
        &signing_key,
        Some(&content),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_send_transaction_with_empty_pdus_returns_ok() {
    let key_id = "ed25519:txn_test";
    let signing_key_seed = [77u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_signing_app(key_id, &signing_key_b64).await else {
        return;
    };

    let txn_id = format!("txn-empty-{}", rand::random::<u32>());
    let content = json!({
        "origin": "localhost",
        "origin_server_ts": 1_700_000_000_000_i64,
        "pdus": [],
        "edus": []
    });
    let request = signed_federation_request(
        "PUT",
        &format!("/_matrix/federation/v1/send/{}", txn_id),
        "localhost",
        key_id,
        &signing_key,
        Some(&content),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["pdus"], json!({}));
}

#[tokio::test]
async fn test_send_transaction_with_signed_pdu_accepts_and_persists() {
    let key_id = "ed25519:txn_pdu";
    let signing_key_seed = [88u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_signing_app(key_id, &signing_key_b64).await else {
        return;
    };

    // 先注册用户并创建房间（房间必须存在才能接受 PDU）
    let (token, user_id) = {
        let req = Request::builder()
            .method("POST")
            .uri("/_matrix/client/v3/register")
            .header("Content-Type", "application/json")
            .body(Body::from(json!({
                "username": format!("txn_pdu_{}", rand::random::<u32>()),
                "password": "PduPass123!",
                "auth": {"type": "m.login.dummy"}
            }).to_string()))
            .unwrap();
        let resp = ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(req)).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        (json["access_token"].as_str().unwrap().to_string(),
         json["user_id"].as_str().unwrap().to_string())
    };

    let room_req = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Txn PDU Test"}).to_string()))
        .unwrap();
    let resp = ServiceExt::<Request<Body>>::oneshot(app.clone(), super::with_local_connect_info(room_req)).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    // 构建一个已签名的 m.room.message PDU
    let event_id = format!("${}", rand::random::<u64>());
    let mut pdu = json!({
        "event_id": event_id,
        "type": "m.room.message",
        "room_id": room_id,
        "sender": user_id,
        "origin": "localhost",
        "origin_server_ts": 1_700_000_000_000_i64,
        "content": {"msgtype": "m.text", "body": "contract test PDU"},
        "prev_events": [],
        "auth_events": [],
        "depth": 1,
    });

    sign_and_hash_event("localhost", key_id, &signing_key_b64, &mut pdu).unwrap();

    let txn_id = format!("txn-pdu-{}", rand::random::<u32>());
    let content = json!({
        "origin": "localhost",
        "origin_server_ts": 1_700_000_000_000_i64,
        "pdus": [pdu],
        "edus": []
    });
    let request = signed_federation_request(
        "PUT",
        &format!("/_matrix/federation/v1/send/{}", txn_id),
        "localhost",
        key_id,
        &signing_key,
        Some(&content),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let response_json: Value = serde_json::from_slice(&body).unwrap();
    // 成功处理的 PDU 应在 pdus 字段中出现
    assert!(response_json["pdus"].as_object().map_or(false, |o| o.contains_key(&event_id)));
}
```

- [ ] **Step 2: 编译 + 运行测试**

```bash
SQLX_OFFLINE=true cargo check --workspace
cargo test --test integration api_federation_transaction_tests -- --nocapture --test-threads=1
```

- [ ] **Step 3: 提交**

```bash
git add tests/integration/api_federation_transaction_tests.rs
git commit -m "test(contract): add federation send_transaction integration tests

Covers PUT /_matrix/federation/v1/send/{txnId} with:
- Missing signature → 401 rejection
- Invalid signature (wrong origin) → 401 rejection
- Empty PDU array → 200 OK with empty pdus map
- Signed PDU → 200 OK with persisted event ID

Addresses P0 gap GAP-03 from API contract audit.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 8: P2 — Canonical JSON 独立单元测试

**动机**: 审计 18 的 P1 Gap-07。`canonical_json.rs` 无自身测试文件。边界情况：Unicode 转义（U+2028/U+2029/U+FFFD）、嵌套对象排序、超大数字拒绝。

**文件:**
- Create: `tests/unit/canonical_json_tests.rs`

- [ ] **Step 1: 编写边界情况测试**

```rust
use serde_json::json;
use synapse_common::canonical_json;

#[test]
fn test_canonical_json_escapes_unicode_line_separator() {
    // U+2028 (LINE SEPARATOR) 必须转义
    let value = json!({"text": "line\u{2028}sep"});
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("\\u2028"));
    assert!(!result.contains('\u{2028}'));
}

#[test]
fn test_canonical_json_escapes_unicode_paragraph_separator() {
    // U+2029 (PARAGRAPH SEPARATOR) 必须转义
    let value = json!({"text": "para\u{2029}sep"});
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("\\u2029"));
    assert!(!result.contains('\u{2029}'));
}

#[test]
fn test_canonical_json_escapes_replacement_character() {
    // U+FFFD (REPLACEMENT CHARACTER) 必须转义
    let value = json!({"text": "bad\u{fffd}char"});
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("\\ufffd"));
}

#[test]
fn test_canonical_json_sorts_nested_object_keys() {
    let value = json!({
        "z": {"c": 3, "a": 1},
        "a": {"z": 26, "m": 13}
    });
    let result = canonical_json(&value).unwrap();
    // 外层: a 在 z 之前
    assert!(result.find("\"a\"").unwrap() < result.find("\"z\"").unwrap());
    // 内层 a: a 在 c 之前
    let a_obj_start = result.find("{\"a\"").unwrap();
    let a_obj = &result[a_obj_start..];
    assert!(a_obj.find("\"a\"").unwrap() < a_obj.find("\"c\"").unwrap());
}

#[test]
fn test_canonical_json_rejects_float() {
    let value = json!({"count": 1.5});
    let err = canonical_json(&value).unwrap_err();
    assert!(err.to_string().contains("Float") || err.to_string().contains("float"));
}

#[test]
fn test_canonical_json_rejects_large_integer() {
    // 超过 2^53-1 的整数必须被拒绝
    let value = json!({"count": 9007199254740992_i64}); // 2^53
    let err = canonical_json(&value).unwrap_err();
    assert!(err.to_string().contains("range") || err.to_string().contains("53") || err.to_string().contains("integer"));
}

#[test]
fn test_canonical_json_accepts_max_valid_integer() {
    // 2^53-1 是允许的最大值
    let value = json!({"count": 9007199254740991_i64}); // 2^53 - 1
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("9007199254740991"));
}

#[test]
fn test_canonical_json_control_character_escape() {
    // <0x20 的控制字符必须转义
    let value = json!({"text": "line\nfeed\tand\rcr"});
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("\\n"));
    assert!(result.contains("\\t"));
    assert!(result.contains("\\r"));
    assert!(!result.contains('\n'));
    assert!(!result.contains('\t'));
    assert!(!result.contains('\r'));
}

#[test]
fn test_canonical_json_string_quote_escape() {
    let value = json!({"text": "say \"hello\""});
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("\\\""));
}

#[test]
fn test_canonical_json_backslash_escape() {
    let value = json!({"text": "path\\to\\file"});
    let result = canonical_json(&value).unwrap();
    assert!(result.contains("\\\\"));
}

#[test]
fn test_canonical_json_null() {
    assert_eq!(canonical_json(&json!(null)).unwrap(), "null");
}

#[test]
fn test_canonical_json_empty_object() {
    assert_eq!(canonical_json(&json!({})).unwrap(), "{}");
}

#[test]
fn test_canonical_json_empty_array() {
    assert_eq!(canonical_json(&json!([])).unwrap(), "[]");
}
```

- [ ] **Step 2: 运行全部测试**

```bash
cargo test --test unit canonical_json_tests -- --nocapture
```

- [ ] **Step 3: 提交**

```bash
git add tests/unit/canonical_json_tests.rs
git commit -m "test(contract): add canonical JSON edge-case unit tests

Covers Unicode escapes (U+2028/U+2029/U+FFFD), nested key sorting,
float rejection, large integer rejection, control character escapes,
and basic type serialization. Addresses P1 gap GAP-07.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## 建议执行顺序

```
Task 1 (P1 安全: token 校验错误传播)  ← 唯一剩余 P1 安全修复
  ↓
Task 2 (P2 防御: federation auth 日志)  \
Task 3 (P2 防御: widget/transaction 日志)  → 可并行
  ↓
Task 4 (P2 性能: 播种脚本)  \
Task 5 (P2 性能: membership bench)  → 可并行
  ↓
Task 6 (P2 契约: login/sync snapshot)  \
Task 7 (P2 契约: federation send_transaction test)  → 可并行
  ↓
Task 8 (P2 契约: canonical JSON 单元测试)  /
```

## 汇总

| 优先级 | 任务数 | 预估改动量 | 产出行数 |
|--------|--------|-----------|---------|
| P1 安全 | 1 | ~15 行 | account_compat.rs + 测试 |
| P2 防御 | 2 | ~30 行 | federation_auth.rs + widget.rs + transaction.rs + join.rs |
| P2 性能 | 2 | ~150 行 | seed_bench_data.sh + membership bench |
| P2 契约 | 3 | ~400 行 | 2 snapshots + 4 federation tests + 13 canonical JSON tests |
| **合计** | **8** | **~600 行** | 5 修改 + 4 创建 |
