# Synapse Rust API Reference

## Overview

This document provides comprehensive API documentation for the Synapse Rust Matrix Homeserver implementation.

## Architecture

### Core Components

- **Web Layer**: HTTP API handlers (Axum framework)
- **Service Layer**: Business logic and coordination
- **Storage Layer**: Database operations (PostgreSQL)
- **Cache Layer**: Multi-tier caching (Local + Redis)

### Data Flow

```
Client Request → Authentication → Web Layer → Service Layer → Storage → Cache → Response
```

---

## Authentication API

### Login

**Endpoint**: `POST /_matrix/client/r0/login`

**Request**:
```json
{
  "type": "m.login.password",
  "user": "username",
  "password": "password"
}
```

**Response**:
```json
{
  "access_token": "...",
  "user_id": "@user:server.name",
  "device_id": "DEVICE123"
}
```

### Logout

**Endpoint**: `POST /_matrix/client/r0/logout`

**Response**: `200 OK` on success

---

## User API

### Get User Profile

**Endpoint**: `GET /_matrix/client/r0/profile/{user_id}`

**Response**:
```json
{
  "user_id": "@user:server.name",
  "displayname": "Display Name",
  "avatar_url": "mxc://server/avatar"
}
```

### Set User Profile

**Endpoint**: `PUT /_matrix/client/r0/profile/{user_id}/{field}`

---

## Room API

### Create Room

**Endpoint**: `POST /_matrix/client/r0/createRoom`

**Request**:
```json
{
  "visibility": "private",
  "name": "Room Name",
  "topic": "Room Topic"
}
```

**Response**:
```json
{
  "room_id": "!roomid:server.name"
}
```

### Get Room State

**Endpoint**: `GET /_matrix/client/r0/rooms/{room_id}/state`

### Send Event

**Endpoint**: `POST /_matrix/client/r0/rooms/{room_id}/send/{event_type}`

---

## Messaging API

### Send Message

**Endpoint**: `POST /_matrix/client/r0/rooms/{room_id}/send/m.room.message`

**Request**:
```json
{
  "msgtype": "m.text",
  "body": "Hello, World!"
}
```

### Sync

**Endpoint**: `GET /_matrix/client/r0/sync`

**Query Parameters**:
- `timeout`: Polling timeout in ms
- `since`: Token from last sync
- `filter`: Filter ID or definition

**Response**:
```json
{
  "next_batch": "...",
  "rooms": {
    "join": {},
    "invite": {},
    "leave": {}
  },
  "presence": {}
}
```

---

## Search API

### User Directory Search

**Endpoint**: `POST /_matrix/client/r0/user_directory/search`

**Request**:
```json
{
  "search_term": "alice",
  "limit": 10
}
```

**Response**:
```json
{
  "results": [
    {
      "user_id": "@alice:server.name",
      "display_name": "Alice",
      "avatar_url": "mxc://..."
    }
  ],
  "count": 1
}
```

### Elasticsearch Message Search (Optional)

**Endpoint**: `GET /_synapse/search/messages`

**Query Parameters**:
- `query`: Search query
- `limit`: Max results
- `sender_id`: Filter by sender

**Response**:
```json
{
  "results": [...],
  "count": 10
}
```

---

## Content Moderation API

### Report Event

**Endpoint**: `POST /_matrix/client/r0/rooms/{room_id}/report/{event_id}`

**Request**:
```json
{
  "reason": "Spam or abusive content",
  "score": -100
}
```

**Response**:
```json
{
  "report_id": 12345
}
```

### Update Report Score

**Endpoint**: `PUT /_matrix/client/r0/rooms/{room_id}/report/{event_id}/score`

**Request**:
```json
{
  "score": -50
}
```

---

## Admin API

### Get User Reputation

**Endpoint**: `GET /_synapse/admin/reputation/{user_id}`

**Response**:
```json
{
  "user_id": "@user:server.name",
  "reputation_score": 45,
  "total_reports": 5,
  "is_banned": false
}
```

### Ban User

**Endpoint**: `PUT /_synapse/admin/ban/{user_id}`

**Request**:
```json
{
  "reason": "Violation of terms",
  "expires_at": null
}
```

### Get Cache Stats

**Endpoint**: `GET /_synapse/admin/cache/stats`

**Response**:
```json
{
  "hits": 1000,
  "misses": 100,
  "hit_rate": 0.91,
  "total_entries": 500
}
}

---

## Configuration

### Homeserver Configuration

**File**: `homeserver.yaml`

```yaml
server:
  name: "server.name"
  port: 8008

database:
  host: "localhost"
  port: 5432
  name: "synapse"
  user: "synapse"

redis:
  host: "localhost"
  port: 6379

search:
  elasticsearch_url: "http://localhost:9200"
  enabled: false

cache:
  max_capacity: 10000
  time_to_live: 3600
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `SYNAPSE_CONFIG_PATH` | Path to configuration file |
| `SYNAPSE_DB_HOST` | Database host (overrides config) |
| `SYNAPSE_REDIS_HOST` | Redis host (overrides config) |

---

## Rate Limiting

### Default Limits

| Endpoint Type | Requests | Window |
|---------------|----------|--------|
| Auth endpoints | 5 | 10 seconds |
| Message sending | 10 | 10 seconds |
| General API | 30 | 10 seconds |

---

## Error Codes

| Code | Description |
|------|-------------|
| `M_FORBIDDEN` | Forbidden access |
| `M_UNKNOWN_TOKEN` | Invalid access token |
| `M_NOT_FOUND` | Resource not found |
| `M_LIMIT_EXCEEDED` | Rate limit exceeded |
| `M_INTERNAL_ERROR` | Server error |

---

## WebSocket API

### Sync Stream

**Endpoint**: `GET /_matrix/client/r0/sync`

**Usage**: Set `Accept: text/event-stream` header for streaming.

---

## Best Practices

### 1. Connection Pooling

Use connection pooling for database operations:

```rust
let pool = sqlx::PgPool::connect(&config.database_url).await?;
```

### 2. Caching Strategy

Cache frequently accessed data:

```rust
cache.set_user(&user_id, &user_data, 600).await;
```

### 3. Rate Limiting

Implement rate limiting for client endpoints:

```rust
let decision = cache_manager.rate_limit_token_bucket_take(
    &key, 
    rate_per_second, 
    burst_size
).await?;
```

### 4. Error Handling

Always handle API errors appropriately:

```rust
match service.method().await {
    Ok(result) => Ok(Json(result)),
    Err(ApiError::NotFound(msg)) => Err(ApiError::not_found(msg)),
    Err(e) => Err(e),
}
```

---

## SDK Examples

### Rust Client

```rust
use reqwest;

let client = reqwest::Client::new();
let response = client
    .post("http://server:8008/_matrix/client/r0/login")
    .json(&serde_json::json!({
        "type": "m.login.password",
        "user": "username",
        "password": "password"
    }))
    .send()
    .await?;
```

### Python Client

```python
import requests

session = requests.Session()

# Login
response = session.post(
    "http://server:8008/_matrix/client/r0/login",
    json={
        "type": "m.login.password",
        "user": "username",
        "password": "password"
    }
)
token = response.json()["access_token"]

# Sync
response = session.get(
    "http://server:8008/_matrix/client/r0/sync",
    params={"timeout": 30000},
    headers={"Authorization": f"Bearer {token}"}
)
```

### JavaScript Client

```javascript
const fetch = require('node-fetch');

const client = {
  baseUrl: 'http://server:8008',
  
  async login(username, password) {
    const response = await fetch(`${this.baseUrl}/_matrix/client/r0/login`, {
      method: 'POST',
      body: JSON.stringify({
        type: 'm.login.password',
        user: username,
        password: password
      })
    });
    return response.json();
  },
  
  async sync(accessToken, timeout = 30000) {
    const response = await fetch(
      `${this.baseUrl}/_matrix/client/r0/sync?timeout=${timeout}`,
      {
        headers: { 'Authorization': `Bearer ${accessToken}` }
      }
    );
    return response.json();
  }
};
```

---

## Performance Optimization Tips

### 1. Batch Operations

```rust
// Instead of individual inserts
for user in users {
    insert_user(&user).await?;
}

// Use batch operations
insert_users_batch(&users).await?;
```

### 2. Pagination

Always use pagination for large datasets:

```rust
let page = query.get("page").unwrap_or(0);
let limit = query.get("limit").unwrap_or(50);
let offset = page * limit;
```

### 3. Async/Await

Leverage Rust's async runtime:

```rust
tokio::spawn(async {
    process_events().await;
});
```

---

## Security Considerations

### Token Management

- Store tokens securely
- Implement token refresh
- Use short-lived tokens

### Input Validation

- Validate all user inputs
- Sanitize database queries
- Use parameterized statements

### CORS Configuration

Configure CORS for web clients:

```yaml
cors:
  allowed_origins:
    - "https://client.example.com"
```

---

## Monitoring and Debugging

### Metrics Endpoint

**Endpoint**: `GET /_synapse/admin/metrics`

### Health Check

**Endpoint**: `GET /_health`

### Debug Endpoints

- `GET /_synapse/admin/debug/threads`
- `GET /_synapse/admin/debug/locks`

---

## Version Compatibility

| Synapse Rust Version | Matrix Spec Version |
|---------------------|---------------------|
| 0.1.0 | v1.0 - v1.11 |

---

## Support

- **GitHub**: https://github.com/your-org/synapse-rust
- **Documentation**: https://docs.synapse-rust.dev
- **Matrix Room**: #synapse-rust:matrix.org
