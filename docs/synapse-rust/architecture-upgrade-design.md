# Backend Architecture Upgrade Design: Redis Integration & Message Queue

**Status**: Draft
**Version**: 1.0.0
**Date**: 2026-02-08

## 1. Introduction

This document outlines the design for Phase 4 of the HuLa backend architecture upgrade. The primary goals are to:
1.  **Introduce Redis** for distributed caching and rate limiting, replacing in-memory solutions to allow horizontal scaling.
2.  **Refactor Task Queue** to use Redis Streams (or Pub/Sub) instead of local Tokio channels, ensuring reliability and supporting multiple worker instances.

## 2. Redis Integration

### 2.1 Current State
- **Caching**: `CacheManager` (`src/cache/mod.rs`) uses `moka::sync::Cache` for in-memory caching. It supports an optional Redis backend (`deadpool-redis`), but usage is mixed and primarily local-first.
- **Rate Limiting**: In-memory token bucket implementation (`LocalRateLimitState`).
- **Session**: JWT tokens are validated against local cache or database.

### 2.2 Upgrade Design

#### 2.2.1 Distributed Cache Strategy
We will transition `CacheManager` to be Redis-primary with a local L1 cache (hybrid approach).

*   **L1 Cache**: `moka` (In-memory, short TTL, e.g., 5s). fast access for hot keys.
*   **L2 Cache**: Redis (Distributed, longer TTL, e.g., 1h). Source of truth for cached data.

**Key Schema:**
*   **User Sessions**: `session:{token}` -> `Claims` (JSON)
    *   TTL: Matches token expiration.
*   **User Active Status**: `user:active:{user_id}` -> `bool` (0/1)
    *   TTL: 5 minutes (heartbeat).
*   **Federation Keys**: `federation:keys:{server_name}:{key_id}` -> `KeyData`
    *   TTL: 24 hours.

#### 2.2.2 Distributed Rate Limiting
Replace `rate_limit_local` `Mutex<HashMap>` with Redis Lua scripts for atomic token bucket operations.

*   **Key**: `ratelimit:{endpoint}:{ip_or_user}`
*   **Algorithm**: Token Bucket (already partially implemented in `token_bucket_take` Lua script in `src/cache/mod.rs`).
*   **Action**: Ensure all rate limit checks call `CacheManager::rate_limit_token_bucket_take` instead of local logic.

### 2.3 Implementation Plan
1.  **Refactor `CacheManager`**:
    *   Remove `use_redis` flag logic that falls back to local storage for *persistent* data.
    *   Make Redis connection mandatory for production profile.
    *   Implement "Cache Aside" pattern properly: `get` -> check L1 -> check L2 -> fetch DB -> set L2 -> set L1.

## 3. Message Queue Refactoring

### 3.1 Current State
- **Task Queue**: `TaskQueue` (`src/common/task_queue.rs`) uses `tokio::sync::mpsc::unbounded_channel`.
- **Limitation**: If the server restarts, queued tasks are lost. Only scales vertically (single instance).

### 3.2 Upgrade Design: Redis Streams

We will replace the in-memory `mpsc` channel with **Redis Streams** for durable, distributed task processing.

#### 3.2.1 Stream Architecture
*   **Stream Key**: `mq:tasks:default`
*   **Consumer Group**: `synapse_workers`
*   **Consumers**: Unique ID per instance (e.g., `worker-{hostname}-{pid}`).

#### 3.2.2 Task Structure (Payload)
Tasks will be serialized as JSON/MsgPack stored in the stream entry.

```rust
struct TaskPayload {
    id: String,
    task_type: String, // e.g., "send_email", "push_notification", "media_process"
    payload: serde_json::Value,
    created_at: u64,
    retry_count: u32,
}
```

#### 3.2.3 Workflow
1.  **Producer** (`BackgroundTaskManager::submit`):
    *   Serialize task closure context (Note: Closures are hard to serialize. We must refactor `TaskHandler` to use **Data Objects** instead of Closures).
    *   `XADD mq:tasks:default * data {json_payload}`.
2.  **Consumer** (Background Worker):
    *   `XREADGROUP GROUP synapse_workers COUNT 10 BLOCK 2000 STREAMS mq:tasks:default >`.
    *   Deserialize `TaskPayload`.
    *   Route to specific handler based on `task_type`.
    *   Execute logic.
    *   **Ack**: `XACK mq:tasks:default synapse_workers {entry_id}`.

### 3.3 Refactoring `TaskHandler`
The current `TaskHandler` trait relies on `FnOnce` closures, which cannot be sent over network/Redis. We must convert this to a Command pattern.

**New Enum-based Task Definition:**
```rust
#[derive(Serialize, Deserialize)]
pub enum BackgroundJob {
    SendEmail { to: String, subject: String, body: String },
    ProcessMedia { file_id: String },
    FederationTransaction { txn_id: String, destination: String },
}
```

## 4. Migration Steps

1.  **Dependency**: Add `redis` crate features (streams).
2.  **Code Change**:
    *   Define `BackgroundJob` enum.
    *   Create `RedisTaskQueue` struct implementing `submit(job: BackgroundJob)`.
    *   Create `WorkerService` that polls Redis Stream.
3.  **Deployment**:
    *   Deploy Redis instance.
    *   Update `homeserver.yaml` with Redis config.

## 5. Future Considerations
*   **Kafka**: If throughput exceeds Redis capabilities (>100k msg/sec), migrate MQ interface to Kafka.
*   **Dead Letter Queue (DLQ)**: Implement `mq:tasks:dead` for failed tasks after N retries.
