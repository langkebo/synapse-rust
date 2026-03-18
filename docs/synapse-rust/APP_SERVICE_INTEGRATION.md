# App Service (应用服务) 模块文档

> 版本: v1.0.0
> 更新日期: 2026-03-17
> 适用项目: synapse-rust

---

## 1. 概述

### 1.1 什么是 App Service

App Service（应用服务）是 Matrix 协议中的核心组件，提供标准化接口与集成机制，允许第三方机器人和外部服务接入 Matrix 生态系统。主要应用场景包括：

- **跨平台桥接**：IRC、Slack、Discord、Telegram 等平台的消息桥接
- **机器人服务**：自动化机器人、AI 助手
- **外部集成**：新闻推送、监控告警、数据分析

### 1.2 核心功能

| 功能 | 说明 |
|------|------|
| 服务注册与管理 | 动态注册、更新、删除应用服务 |
| 虚拟用户管理 | 为桥接服务创建虚拟用户 |
| 命名空间管理 | 用户/房间别名的正则匹配规则 |
| 事件推送 | 向应用服务推送 Matrix 事件 |
| 事务处理 | 可靠的事件传递与重试机制 |
| 健康监控 | 服务状态与健康检查 |

---

## 2. API 端点

### 2.1 Matrix 标准 API

遵循 Matrix App Service 规范 (MSC2409)：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/_matrix/app/v1/ping` | POST | 服务心跳检测 |
| `/_matrix/app/v1/transactions/{as_id}/{txn_id}` | PUT | 接收事件事务 |
| `/_matrix/app/v1/users/{user_id}` | GET | 用户命名空间查询 |
| `/_matrix/app/v1/rooms/{alias}` | GET | 房间别名查询 |

### 2.2 管理端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/_synapse/admin/v1/appservices` | GET | 列出所有应用服务 |
| `/_synapse/admin/v1/appservices` | POST | 注册新应用服务 |
| `/_synapse/admin/v1/appservices/{as_id}` | GET | 获取服务详情 |
| `/_synapse/admin/v1/appservices/{as_id}` | PUT | 更新服务配置 |
| `/_synapse/admin/v1/appservices/{as_id}` | DELETE | 删除服务 |
| `/_synapse/admin/v1/appservices/{as_id}/ping` | POST | Ping 服务 |
| `/_synapse/admin/v1/appservices/{as_id}/state` | GET/POST | 服务状态管理 |
| `/_synapse/admin/v1/appservices/{as_id}/users` | GET/POST | 虚拟用户管理 |
| `/_synapse/admin/v1/appservices/{as_id}/namespaces` | GET | 获取命名空间 |
| `/_synapse/admin/v1/appservices/{as_id}/events` | GET/POST | 事件管理 |
| `/_synapse/admin/v1/appservices/statistics` | GET | 获取统计信息 |

### 2.3 外部服务集成端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/_synapse/admin/v1/external_services` | POST | 注册外部服务 |
| `/_synapse/admin/v1/external_services/{type}` | GET | 按类型列出服务 |
| `/_synapse/admin/v1/external_services/{as_id}/health` | GET | 获取健康状态 |
| `/_synapse/admin/v1/external_services/{as_id}/health/check` | POST | 执行健康检查 |
| `/_synapse/admin/v1/external_services/{as_id}` | DELETE | 注销服务 |
| `/_synapse/external/trendradar/{service_id}/webhook` | POST | TrendRadar Webhook |
| `/_synapse/external/openclaw/{service_id}/webhook` | POST | OpenClaw Webhook |
| `/_synapse/external/webhook/{service_id}` | POST | 通用 Webhook |

---

## 3. 外部服务集成

### 3.1 支持的服务类型

| 类型 | 标识 | 说明 |
|------|------|------|
| TrendRadar | `trendradar` | 新闻热点聚合推送 |
| OpenClaw | `openclaw` | AI Agent 集成 |
| Generic Webhook | `generic_webhook` | 通用 Webhook 集成 |
| IRC Bridge | `irc_bridge` | IRC 桥接 |
| Slack Bridge | `slack_bridge` | Slack 桥接 |
| Discord Bridge | `discord_bridge` | Discord 桥接 |
| Custom | `custom` | 自定义服务 |

### 3.2 TrendRadar 集成

TrendRadar 是一个新闻热点聚合平台，支持将热点新闻推送到 Matrix 房间。

#### 注册 TrendRadar 服务

```bash
curl -X POST "https://your-server/_synapse/admin/v1/external_services" \
  -H "Authorization: Bearer <admin_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "service_type": "trendradar",
    "service_id": "news-bot",
    "display_name": "News Bot",
    "webhook_url": "https://trendradar.example.com/webhook",
    "config": {
      "topic": "tech-news",
      "include_rss": true,
      "include_hotlist": true,
      "keywords": ["AI", "区块链", "科技"],
      "max_items": 20
    }
  }'
```

#### Webhook Payload 格式

```json
{
  "title": "ChatGPT-5 正式发布",
  "content": "OpenAI 今日宣布 ChatGPT-5 正式发布...",
  "source": "今日头条",
  "timestamp": 1710691200000,
  "url": "https://example.com/news/123",
  "keywords": ["AI", "ChatGPT"],
  "metadata": {
    "rank": 1,
    "platform": "toutiao"
  }
}
```

#### 接收推送

TrendRadar 会将新闻推送到 Matrix 房间：

```bash
curl -X POST "https://your-server/_synapse/external/trendradar/news-bot/webhook" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "重要新闻标题",
    "content": "新闻内容详情...",
    "source": "微博热搜",
    "timestamp": 1710691200000,
    "url": "https://weibo.com/...",
    "keywords": ["关键词1", "关键词2"]
  }'
```

### 3.3 OpenClaw 集成

OpenClaw 是一个 AI Agent 框架，支持自动化任务和智能响应。

#### 注册 OpenClaw 服务

```bash
curl -X POST "https://your-server/_synapse/admin/v1/external_services" \
  -H "Authorization: Bearer <admin_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "service_type": "openclaw",
    "service_id": "ai-assistant",
    "display_name": "AI Assistant",
    "webhook_url": "http://localhost:8080/api",
    "config": {
      "agent_id": "agent-001",
      "api_endpoint": "http://localhost:8080",
      "capabilities": ["message", "reaction", "summary"],
      "auto_respond": true
    }
  }'
```

#### Webhook Payload 格式

```json
{
  "action": "message",
  "room_id": "!room:example.com",
  "event_id": "$event_id:example.com",
  "content": {
    "text": "AI 生成的回复内容"
  },
  "context": {
    "trigger_event": "m.room.message",
    "user_id": "@user:example.com"
  }
}
```

### 3.4 通用 Webhook 集成

适用于任何支持 HTTP Webhook 的服务。

#### 注册通用 Webhook

```bash
curl -X POST "https://your-server/_synapse/admin/v1/external_services" \
  -H "Authorization: Bearer <admin_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "service_type": "generic_webhook",
    "service_id": "custom-service",
    "display_name": "Custom Service",
    "webhook_url": "https://your-service.com/webhook",
    "config": {
      "secret": "your-webhook-secret",
      "events": ["m.room.message", "m.reaction"]
    }
  }'
```

#### Webhook Payload 格式

```json
{
  "event_type": "m.room.message",
  "timestamp": 1710691200000,
  "data": {
    "room_id": "!room:example.com",
    "sender": "@user:example.com",
    "content": {
      "msgtype": "m.text",
      "body": "消息内容"
    }
  },
  "signature": "sha256=..."
}
```

---

## 4. 命名空间配置

### 4.1 命名空间结构

```json
{
  "users": [
    {
      "exclusive": true,
      "regex": "@irc_.*:example\\.com",
      "group_id": "group:example.com"
    }
  ],
  "aliases": [
    {
      "exclusive": true,
      "regex": "#irc_.*:example\\.com"
    }
  ],
  "rooms": []
}
```

### 4.2 命名空间规则

| 字段 | 类型 | 说明 |
|------|------|------|
| `exclusive` | bool | 是否独占（阻止其他服务匹配同一用户） |
| `regex` | string | 正则表达式匹配规则 |
| `group_id` | string? | 可选的群组 ID |

### 4.3 预定义命名空间

系统会为不同类型的服务自动生成命名空间：

| 服务类型 | 用户命名空间 | 别名命名空间 |
|----------|--------------|--------------|
| TrendRadar | `@trendradar_.*:server` | `#trendradar_.*:server` |
| OpenClaw | `@openclaw_.*:server` | - |
| IRC Bridge | `@irc_.*:server` | `#irc_.*:server` |
| Slack Bridge | `@slack_.*:server` | `#slack_.*:server` |

---

## 5. 健康监控

### 5.1 健康状态结构

```json
{
  "service_id": "trendradar_news-bot",
  "service_type": "trendradar",
  "is_healthy": true,
  "last_check_ts": 1710691200000,
  "last_success_ts": 1710691200000,
  "last_error": null,
  "consecutive_failures": 0
}
```

### 5.2 健康检查机制

- **自动检查**：每次 Webhook 调用后更新状态
- **手动检查**：通过 API 触发健康检查
- **失败阈值**：连续 3 次失败后标记为不健康

### 5.3 健康检查 API

```bash
# 获取所有服务健康状态
curl "https://your-server/_synapse/admin/v1/external_services/health"

# 获取单个服务健康状态
curl "https://your-server/_synapse/admin/v1/external_services/trendradar_news-bot/health"

# 执行健康检查
curl -X POST "https://your-server/_synapse/admin/v1/external_services/trendradar_news-bot/health/check"
```

---

## 6. 错误处理

### 6.1 错误类型

| 错误码 | 说明 |
|--------|------|
| `M_FORBIDDEN` | 权限不足 |
| `M_UNAUTHORIZED` | Token 无效 |
| `M_NOT_FOUND` | 服务不存在 |
| `M_BAD_REQUEST` | 请求参数错误 |
| `M_UNKNOWN` | 未知错误 |

### 6.2 重试机制

事务处理支持自动重试：

1. 首次发送失败后记录错误
2. 增加重试计数
3. 连续失败 3 次后标记服务不健康
4. 可通过管理 API 手动重试

### 6.3 日志记录

所有操作都会记录详细日志：

```
INFO  Registering application service: as_id=irc-bridge
INFO  Application service registered successfully: as_id=irc-bridge
ERROR Failed to send transaction: HTTP 500
WARN  Health check failed: Connection timeout
```

---

## 7. 配置示例

### 7.1 IRC 桥接配置

```json
{
  "id": "irc-bridge",
  "url": "http://irc-bridge:9999",
  "as_token": "irc_as_token_here",
  "hs_token": "irc_hs_token_here",
  "sender": "@irc-bot:example.com",
  "namespaces": {
    "users": [
      {
        "exclusive": true,
        "regex": "@irc_.*:example\\.com"
      }
    ],
    "aliases": [
      {
        "exclusive": true,
        "regex": "#irc_.*:example\\.com"
      }
    ],
    "rooms": []
  },
  "protocols": ["irc"],
  "rate_limited": false
}
```

### 7.2 Slack 桥接配置

```json
{
  "id": "slack-bridge",
  "url": "http://slack-bridge:9999",
  "as_token": "slack_as_token_here",
  "hs_token": "slack_hs_token_here",
  "sender": "@slack-bot:example.com",
  "namespaces": {
    "users": [
      {
        "exclusive": true,
        "regex": "@slack_.*:example\\.com"
      }
    ],
    "aliases": [
      {
        "exclusive": true,
        "regex": "#slack_.*:example\\.com"
      }
    ],
    "rooms": []
  },
  "protocols": ["slack"],
  "rate_limited": false
}
```

---

## 8. 数据库表结构

### 8.1 application_services

```sql
CREATE TABLE application_services (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    as_token TEXT NOT NULL,
    hs_token TEXT NOT NULL,
    sender TEXT NOT NULL,
    name TEXT,
    description TEXT,
    rate_limited BOOLEAN DEFAULT FALSE,
    protocols JSONB DEFAULT '[]',
    namespaces JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_seen_ts BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE
);
```

### 8.2 application_service_users

```sql
CREATE TABLE application_service_users (
    as_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    displayname TEXT,
    avatar_url TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (as_id, user_id)
);
```

### 8.3 application_service_events

```sql
CREATE TABLE application_service_events (
    event_id TEXT NOT NULL PRIMARY KEY,
    as_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    transaction_id TEXT
);
```

### 8.4 application_service_transactions

```sql
CREATE TABLE application_service_transactions (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    events JSONB NOT NULL,
    sent_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    retry_count INTEGER DEFAULT 0,
    last_error TEXT
);
```

---

## 9. 最佳实践

### 9.1 安全建议

1. **Token 安全**：使用强随机 Token，定期轮换
2. **HTTPS**：生产环境必须使用 HTTPS
3. **网络隔离**：桥接服务部署在内网
4. **权限最小化**：只授予必要的命名空间

### 9.2 性能优化

1. **批量处理**：使用事务批量发送事件
2. **异步处理**：事件推送使用异步队列
3. **连接池**：复用 HTTP 连接
4. **缓存**：缓存服务配置和命名空间

### 9.3 监控告警

1. 监控服务健康状态
2. 设置失败阈值告警
3. 记录详细的错误日志
4. 定期检查事务积压

---

## 10. 常见问题

### Q1: 如何调试 Webhook 问题？

检查服务健康状态和错误日志：

```bash
curl "https://your-server/_synapse/admin/v1/external_services/{as_id}/health"
```

### Q2: 如何处理事务失败？

查看待处理事件：

```bash
curl "https://your-server/_synapse/admin/v1/appservices/{as_id}/events"
```

### Q3: 如何更新服务配置？

使用 PUT 请求更新：

```bash
curl -X PUT "https://your-server/_synapse/admin/v1/appservices/{as_id}" \
  -H "Authorization: Bearer <token>" \
  -d '{"url": "http://new-url:8080"}'
```

---

## 11. 相关资源

- [Matrix App Service 规范](https://spec.matrix.org/v1.9/application-service-api/)
- [MSC2409: App Service API](https://github.com/matrix-org/matrix-spec-proposals/pull/2409)
- [TrendRadar 项目](https://github.com/sansan0/TrendRadar)
- [synapse-rust 项目规则](/.trae/rules/project_rules.md)
