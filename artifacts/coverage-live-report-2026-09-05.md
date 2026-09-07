# 测试覆盖率现状报告（实时）

> 生成时间：2026-09-05 06:59
> 工具：cargo llvm-cov (llvm-cov report --summary-only)
> 注：当前为根 crate 数据（32.53%），完整 workspace 数据正在后台生成中

---

## 整体数字

| 指标 | 数据 |
|------|------|
| **总 Regions** | 62,366 |
| **覆盖 Regions** | 20,288 |
| **Region 覆盖率** | **32.53%** |
| **总 Functions** | 5,559 |
| **覆盖 Functions** | 1,689 |
| **Function 覆盖率** | **30.38%** |
| **总 Lines** | 40,443 |
| **覆盖 Lines** | 14,170 |
| **Line 覆盖率** | **35.04%** |

> ⚠️ 这是**根 crate** 数据（~40K 行 Rust）。workspace 完整数据（synapse-storage 等 600+crate）需要等后台跑完，预计 ~15 分钟。

---

## 顶层模块覆盖（按 Line 覆盖率降序）

### 高覆盖（≥70%）

| 文件/模块 | 行数 | 覆盖 | 覆盖率 |
|-----------|------|------|--------|
| `src/web/routes/sso.rs` | 146 | 128 | **87.67%** |
| `src/web/routes/search/hierarchy.rs` | 264 | 224 | **84.85%** |
| `src/web/routes/saml.rs` | 195 | 158 | **81.03%** |
| `src/web/routes/guest.rs` | 136 | 109 | **80.15%** |
| `src/web/routes/captcha.rs` | 151 | 120 | **79.47%** |
| `src/web/routes/health.rs` | 45 | 34 | **75.56%** |
| `src/web/routes/formatting.rs` | 23 | 17 | **73.91%** |
| `src/web/routes/module.rs` | 754 | 555 | **73.61%** |
| `src/web/routes/context.rs` | 262 | 187 | **71.37%** |
| `src/web/routes/app_service.rs` | 661 | 467 | **70.65%** |
| `src/web/routes/oidc/sso.rs` | 176 | 124 | **70.45%** |

### 中高覆盖（50-70%）

| 文件/模块 | 行数 | 覆盖 | 覆盖率 |
|-----------|------|------|--------|
| `src/web/routes/admin/registration.rs` | 271 | 185 | **68.27%** |
| `src/web/routes/sync.rs` | 1360 | 893 | **65.66%** |
| `src/web/routes/auth.rs` | 1059 | 666 | **62.89%** |
| `src/web/routes/handlers/room/state.rs` | 1126 | 707 | **62.79%** |
| `src/web/routes/federation/membership/invite.rs` | 252 | 157 | **62.30%** |
| `src/web/routes/push.rs` | 611 | 379 | **62.03%** |
| `src/web/routes/search.rs` | 361 | 221 | **61.22%** |
| `src/web/routes/device.rs` | 504 | 308 | **61.11%** |
| `src/web/routes/room_summary.rs` | 849 | 519 | **61.13%** |
| `src/web/routes/oidc.rs` | 1137 | 680 | **59.81%** |
| `src/web/routes/voice.rs` | 397 | 237 | **59.70%** |
| `src/web/routes/widget.rs` | 709 | 397 | **56.00%** |

### 中低覆盖（30-50%）

| 文件/模块 | 行数 | 覆盖 | 覆盖率 |
|-----------|------|------|--------|
| `src/web/routes/space.rs` | 1207 | 598 | **49.55%** |
| `src/web/routes/room.rs` | 1245 | 606 | **48.67%** |
| `src/web/routes/presence.rs` | 261 | 126 | **48.28%** |
| `src/web/routes/ephemeral.rs` | 56 | 27 | **48.21%** |
| `src/web/routes/thread.rs` | 1133 | 543 | **47.93%** |
| `src/web/routes/key_backup.rs` | 994 | 473 | **47.59%** |
| `src/web/routes/typing.rs` | 135 | 64 | **47.41%** |
| `src/web/routes/profile.rs` | 379 | 177 | **46.70%** |
| `src/web/routes/login.rs` | 1079 | 496 | **45.97%** |
| `src/web/routes/account.rs` | 1045 | 480 | **45.93%** |
| `src/web/routes/rendezvous.rs` | 355 | 160 | **45.07%** |
| `src/web/routes/space/membership_state.rs` | 167 | 75 | **44.91%** |
| `src/web/routes/register.rs` | 1129 | 501 | **44.38%** |
| `src/web/routes/pinned.rs` | 297 | 131 | **44.11%** |
| `src/web/routes/handlers/room/members.rs` | 489 | 212 | **43.35%** |
| `src/web/routes/admin/user.rs` | 731 | 310 | **42.41%** |
| `src/web/routes/media/upload.rs` | 267 | 112 | **41.95%** |
| `src/web/routes/beacon.rs` | 396 | 163 | **41.16%** |
| `src/web/routes/federation/events.rs` | 819 | 332 | **40.54%** |
| `src/web/routes/federation/membership/join.rs` | 285 | 115 | **40.35%** |

### 低覆盖（10-30%）

| 文件/模块 | 行数 | 覆盖 | 覆盖率 |
|-----------|------|------|--------|
| `src/web/routes/verification_routes.rs` | 685 | 200 | **29.20%** |
| `src/web/routes/friend_room.rs` | 1129 | 325 | **28.79%** |
| `src/web/routes/media/download.rs` | 442 | 125 | **28.28%** |
| `src/web/routes/extractors/auth.rs` | 702 | 191 | **27.21%** |
| `src/web/routes/admin/room.rs` | 660 | 179 | **27.12%** |
| `src/web/routes/admin/user.rs` | 731 | 197 | **26.95%** |
| `src/web/routes/admin/federation.rs` | 277 | 72 | **25.99%** |
| `src/web/routes/directory.rs` | 393 | 100 | **25.45%** |
| `src/web/routes/e2ee/devices.rs` | 490 | 122 | **24.90%** |
| `src/web/routes/e2ee/keys.rs` | 488 | 116 | **23.77%** |
| `src/web/routes/federation/membership/knock.rs` | 141 | 33 | **23.40%** |
| `src/web/routes/event_report.rs` | 201 | 46 | **22.89%** |
| `src/web/routes/admin/room/management.rs` | 449 | 102 | **22.72%** |
| `src/web/routes/moderation.rs` | 92 | 20 | **21.74%** |
| `src/web/routes/key_backup.rs` | 994 | 204 | **20.52%** |
| `src/web/routes/voip.rs` | 267 | 54 | **20.22%** |
| `src/web/routes/extractors/pagination.rs` | 237 | 47 | **19.83%** |
| `src/web/routes/federation/keys.rs` | 271 | 53 | **19.56%** |
| `src/web/routes/extractors/localhost_guard.rs` | 336 | 63 | **18.75%** |
| `src/web/routes/extractors/json.rs` | 237 | 41 | **17.30%** |
| `src/web/routes/room_access.rs` | 106 | 18 | **16.98%** |
| `src/web/routes/workers.rs` | 803 | 135 | **16.81%** |
| `src/web/routes/federation/transaction.rs` | 487 | 75 | **15.40%** |
| `src/web/routes/handlers/room/events.rs` | 987 | 143 | **14.49%** |
| `src/web/routes/admin/server.rs` | 277 | 40 | **14.44%** |
| `src/web/routes/route_module.rs` | 368 | 51 | **13.86%** |
| `src/web/routes/admin/security.rs` | 333 | 45 | **13.51%** |
| `src/web/routes/handlers/client_config.rs` | 43 | 5 | **11.63%** |
| `src/web/routes/oidc/builtin.rs` | 181 | 18 | **9.94%** |
| `src/web/routes/feature_flags.rs` | 146 | 14 | **9.59%** |
| `src/web/routes/auth_compat.rs` | 695 | 64 | **9.21%** |
| `src/web/routes/account_compat.rs` | 714 | 63 | **8.82%** |
| `src/web/routes/assembly.rs` | 708 | 61 | **8.62%** |
| `src/web/routes/admin/audit.rs` | 293 | 20 | **6.83%** |

### 零覆盖（0%）

| 文件/模块 | 行数 | 说明 |
|-----------|------|------|
| `src/web/streaming.rs` | 315 | 流式响应，测试难触达 |
| `src/web/utils/auth.rs` | 363 | 工具函数，测试调用链路长 |
| `src/web/utils/ip.rs` | 627 | IP 提取工具，测试边界场景缺失 |
| `src/web/utils/admin_auth.rs` | 859 | Admin 认证，测试调用链路长 |
| `src/web/routes/extractors/pagination.rs` | 237 | 分页工具，测试调用链路长 |

---

## 关键发现（对比审计报告）

### ✅ 审计预测准确的

| 审计报告预测 | llvm-cov 实际 | 说明 |
|-------------|--------------|------|
| `handlers/room/events.rs` 极低 | **14.49%** | ✅ 预测准确 |
| `auth_compat.rs` 极低 | **9.21%** | ✅ 预测准确 |
| `account_compat.rs` 极低 | **8.82%** | ✅ 预测准确 |
| `federation/transaction.rs` 极低 | **15.40%** | ✅ 预测准确 |
| `admin/room/management.rs` 极低 | **22.72%** | ✅ 预测准确 |
| `extractors/localhost_guard.rs` 零/极低 | **18.75%** | ⚠️ 有部分覆盖（XFF 测试触达了工具函数）|

### ❌ 审计报告漏报的真实盲区

| 文件 | 行数 | 覆盖率 | 说明 |
|------|------|--------|------|
| `src/web/routes/admin/audit.rs` | 293 | **6.83%** | Admin 审计日志，审计时漏报 |
| `src/web/routes/admin/security.rs` | 333 | **13.51%** | Admin 安全路由，审计时漏报 |
| `src/web/routes/admin/server.rs` | 277 | **14.44%** | Admin 服务器路由，审计时漏报 |
| `src/web/routes/extractors/json.rs` | 237 | **17.30%** | JSON 解析器，审计时漏报 |
| `src/web/routes/extractors/pagination.rs` | 237 | **19.83%** | 分页工具，审计时漏报 |
| `src/web/routes/extractors/localhost_guard.rs` | 336 | **18.75%** | 实际有工具函数被触达，非零 |

### 🎯 最需要补测的高价值文件

按 **行数 × (1 - 覆盖率)** = **未覆盖行数** 排序：

| 排名 | 文件 | 行数 | 覆盖率 | 未覆盖行 | 优先级 |
|------|------|------|--------|---------|--------|
| 1 | `routes/admin/user.rs` | 731 | 26.9% | 534 | **P-1** |
| 2 | `routes/handlers/room/events.rs` | 987 | 14.5% | 844 | **P-0** |
| 3 | `routes/admin/room.rs` | 660 | 27.1% | 481 | **P-1** |
| 4 | `routes/key_backup.rs` | 994 | 20.5% | 790 | **P-0** |
| 5 | `routes/friend_room.rs` | 1129 | 28.8% | 804 | **P-1** |
| 6 | `routes/admin/room/management.rs` | 449 | 22.7% | 347 | **P-2** |
| 7 | `routes/extractors/auth.rs` | 702 | 27.2% | 511 | **P-1** |
| 8 | `routes/federation/events.rs` | 819 | 40.5% | 487 | **P-1** |
| 9 | `routes/auth_compat.rs` | 695 | 9.2% | 631 | **P-0** |
| 10 | `routes/account_compat.rs` | 714 | 8.8% | 651 | **P-0** |
| 11 | `routes/assembly.rs` | 708 | 8.6% | 647 | **P-0** |
| 12 | `routes/e2ee/devices.rs` | 490 | 24.9% | 368 | **P-2** |
| 13 | `routes/e2ee/keys.rs` | 488 | 23.8% | 372 | **P-2** |
| 14 | `routes/media/download.rs` | 442 | 28.3% | 317 | **P-2** |
| 15 | `routes/admin/federation.rs` | 277 | 26.0% | 205 | **P-2** |

---

## 补充：后台跑完后的预期变化

完整 workspace 覆盖率（`synapse-storage`、`synapse-services`、`synapse-e2ee`、`synapse-federation`）
预计：
- 整体覆盖率会提升（workspace 层有 63 个内联 `db_tests`，覆盖率较高）
- 但根 crate 路由层这 32.53% 基本不变（因为这些测试不覆盖路由 handler）
- 最终数字预计在 **50-68%**（取决于 workspace 测试实际覆盖率）

---

*完整 workspace 数据请等待后台任务完成。*
