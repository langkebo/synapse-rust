# API 集成测试分析报告

> **生成日期**: 2026-03-31
> **项目**: synapse-rust
> **分析对象**: `scripts/test/api-integration_test.sh`

---

## 一、测试结果总览

| 测试类型 | 结果 | 日期 |
|----------|------|------|
| API 集成测试（修复后） | ✅ 486 passed, 0 failed, 54 skipped | 2026-03-31 |
| API 集成测试（脚本增强后） | ✅ 487 passed, 0 failed, 53 skipped | 2026-03-31 |
| 新增测试后（581-584） | ✅ ~491 passed, 0 failed, ~57 skipped | 2026-03-31 |

---

## 二、测试代码问题分类

### 2.1 断言方法不一致（高优先级）

**问题描述**: 测试脚本中存在多种断言方法，导致检测标准不统一。

| 断言类型 | 使用场景 | 问题 |
|----------|----------|------|
| `grep -q` | Media、Filter、OpenID Token | 断言过宽，空响应也会通过 |
| `check_success_json` | Widget、Room Version 等 | 严格校验 HTTP 状态码和 JSON 字段 |
| `curl ... && pass` | 大部分 Federation 测试 | 缺少状态码和错误码校验 |

**影响**:
- Media Download/Thumbnail：返回空 `{}` 时 `grep -q ""` 仍通过
- Get Filter：仅依赖关键字判断
- Request OpenID Token：缺少 HTTP 状态码校验

**建议修复**:
```bash
# 错误示例（断言过宽）
curl -s "$SERVER_URL/..." | grep -q "keyword" && pass || fail

# 正确示例（严格断言）
http_json GET "$SERVER_URL/..." "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "expected_field"
```

### 2.2 脚本误配问题（中优先级）

根据 api-error.md 确认，以下是测试脚本路径/方法错误导致的"假性"跳过：

| 测试名称 | 脚本路径 | 正确路径 | 问题 |
|----------|----------|----------|------|
| Get Presence List | `POST /presence/list` | `GET /presence/list/{user}` | 后端已实现 GET |
| Get Thread | `/$ROOM_ID/threads/$ROOM_ID` | 需要真实 thread_id | THREAD_ID 赋值为 ROOM_ID |
| Server Key Query | `/key/v2/query/{server}` | `/key/v2/query/{server}/{key_id}` | 缺少 key_id |
| Friend Request | `/v3/friends/request` | `/v1/r0/friends/request` | 版本路径错误 |
| Admin User Tokens | `/login` | `/users/{user}/tokens` | 路由错误 |
| Admin Rate Limit | `ratelimit` | `rate_limit` | 拼写错误 |
| Admin Media | `/media/stats` | `/media` 或 `/media/quota` | 路径错误 |

### 2.3 测试数据依赖问题（中优先级）

**Thread 测试问题**:
```bash
# 当前问题：THREAD_ID 直接使用 ROOM_ID
curl "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads/$THREAD_ID"
```
- 需要先创建真实线程并获取 thread_id
- 或使用 rooms/{room_id}/thread/{thread_id} 获取现有线程

**Federation 测试问题**:
- 大部分 Federation 端点需要 `Origin` 头认证
- 客户端 token 会被 `federation_auth_middleware` 拒绝
- 需要模拟联邦请求或使用服务账号

### 2.4 Token 处理问题（低优先级）

| 问题 | 说明 | 影响 |
|------|------|------|
| Admin Token 获取 | 使用共享密钥而非登录获取 | 可能与某些安全配置不兼容 |
| Token 持久化 | 部分测试使用 `$TOKEN` 而非动态获取 | 并发测试时可能冲突 |

---

## 三、项目代码问题分类

### 3.1 数据库契约漂移（已修复 ✅）

| 问题 | 影响 | 状态 |
|------|------|------|
| `rooms.member_count` 缺失 | 建房、房间摘要失败 | ✅ 已修复 |
| `rooms.encryption` 缺失 | 公共房间目录失败 | ✅ 已修复 |
| `registration_tokens.uses_allowed` 缺失 | Admin Token 接口失败 | ✅ 已修复 |
| `events.processed_ts` 缺失 | 事件查询失败 | ✅ 已修复 |

### 3.2 确认未实现的端点

| 端点 | 路径 | 优先级 |
|------|------|--------|
| Admin Devices | `/_synapse/admin/v1/devices` | 🟡 中 |
| Admin Auth | `/_synapse/admin/v1/auth` | 🟡 中 |
| Admin Capabilities | `/_synapse/admin/v1/capabilities` | 🟡 中 |
| Room Shares | `/_synapse/admin/v1/rooms/{id}/shares` | 🟢 低 |
| User Count | `/_synapse/admin/v1/users/count` | 🟢 低 |
| Room Count | `/_synapse/admin/v1/rooms/count` | 🟢 低 |
| Pending Joins | `/_synapse/admin/v1/rooms/{id}/pending_joins` | 🟢 低 |

### 3.3 服务配置问题

| 配置项 | 当前状态 | 影响测试 |
|--------|----------|----------|
| TURN 服务器 | 未配置真实凭据 | VoIP TURN Server 测试 |
| Retention 策略 | 未配置 | Retention 相关测试 |
| CAS/SAML/OIDC | 基础实现 | SSO 相关测试 |

---

## 四、测试覆盖率分析

### 4.1 当前测试覆盖

| 模块 | 总端点 | 已测试 | 覆盖率 |
|------|--------|--------|--------|
| mod (核心) | 57 | 55 | 96% |
| admin/user | 25+ | 22 | 88% |
| admin/room | 35+ | 30 | 86% |
| device | 8 | 8 | 100% |
| e2ee_routes | 27 | 27 | 100% |
| key_backup | 20+ | 20 | 100% |
| room_extended | 100+ | 85+ | 85% |
| federation | 55+ | 38 | 69% |
| other (widget/rendezvous/bg) | 15+ | 12 | 80% |
| **总计** | **680+** | **545** | **80%** |

### 4.2 待改进覆盖率

| 模块 | 建议补充测试 |
|------|-------------|
| Federation Extended | 6 个代表端点（需联邦认证） |
| Thirdparty | 1 个协议测试 |
| Admin Federation | 4 个代表端点 |

---

## 五、行动项

### 高优先级

| # | 问题 | 建议 | 状态 |
|---|------|------|------|
| 1 | 断言方法不一致 | 统一使用 `check_success_json` 替代 `grep -q` | 待处理 |
| 2 | Thread 测试数据问题 | 修复 THREAD_ID 赋值逻辑 | 待处理 |
| 3 | Federation 测试认证 | 添加 `ORIGIN` 头模拟联邦请求 | 待处理 |

### 中优先级

| # | 问题 | 建议 | 状态 |
|---|------|------|------|
| 4 | 脚本误配路径 | 修正 Friend Request、Rate Limit 等路径 | 待处理 |
| 5 | Admin Media 路径 | 修正为正确路径 | 待处理 |
| 6 | 未实现端点 | 根据业务需求实现或标记为 WON'T FIX | 评估中 |

### 低优先级

| # | 问题 | 建议 | 状态 |
|---|------|------|------|
| 7 | Token 获取方式 | 考虑使用登录获取替代共享密钥 | 可选 |
| 8 | 服务配置问题 | 添加配置检查和友好错误提示 | 可选 |

---

## 六、测试脚本质量评估

### 当前评估

| 指标 | 评分 | 说明 |
|------|------|------|
| 覆盖率 | ⭐⭐⭐⭐☆ | 80% 覆盖率，核心功能基本覆盖 |
| 断言严格性 | ⭐⭐⭐☆☆ | 部分测试断言过宽，存在假通过风险 |
| 可维护性 | ⭐⭐⭐⭐☆ | 结构清晰，函数封装良好 |
| 可扩展性 | ⭐⭐⭐⭐⭐ | 模块化设计，易于添加新测试 |
| 可靠性 | ⭐⭐⭐☆☆ | 存在环境依赖和数据依赖问题 |

### 总体评价

测试脚本适合作为**冒烟测试**使用，但不足以直接作为 Matrix/Synapse 合规判定依据。建议优先修复高优先级的断言问题和测试数据问题。

---

*本文档将随项目进展持续更新*
