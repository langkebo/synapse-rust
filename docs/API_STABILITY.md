# API Stability Levels

This document defines the stability levels for API endpoints in synapse-rust.

## Stability Levels

| Level | Badge | Description |
|-------|-------|-------------|
| **Stable** | ✅ | Long-term support, backward compatible, production ready |
| **Experimental** | 🔬 | May change, subject to removal, test with caution |
| **Internal** | 🔒 | Not for public use, internal implementation detail |
| **Deprecated** | ⚠️ | Will be removed in future versions, migrate to alternatives |

---

## Stable APIs (✅)

### Core Matrix APIs
- `/_matrix/client/v3/profile/*` - User profile management
- `/_matrix/client/v3/rooms/*` - Room operations
- `/_matrix/client/v3/sync` - Synchronization
- `/_matrix/client/v3/keys/*` - E2EE key management
- `/_matrix/client/v3/devices/*` - Device management
- `/_matrix/client/v3/account/*` - Account management

### E2EE
- `/_matrix/client/v1/room_keys/*` - Secure backup
- `/_matrix/client/v1/keys/*` - Key backup and cross-signing

### Federation
- `/_matrix/federation/v1/*` - Federation protocol

### Widgets (MSC4261)
- `/_matrix/client/v1/widgets/*` - Widget API
- `/_matrix/client/v3/rooms/{room_id}/widgets/*` - Room widgets

---

## Experimental APIs (🔬)

### AI Integration
- `/_matrix/client/v1/ai/*` - AI connection endpoints
- `/_matrix/client/v1/mcp/*` - MCP tools integration

### Voice
- `/_matrix/client/v1/voice/transcription` - Voice transcription

### Extended Features
- `/_matrix/client/v3/friends/*` - Friend system (non-standard)
- `/_matrix/client/v1/friends/groups/*` - Friend groups

---

## Internal APIs (🔒)

### Admin
- `/_synapse/admin/*` - Admin endpoints (require admin privileges)

### Health & Metrics
- `/_health` - Health check
- `/_metrics` - Prometheus metrics

---

## Deprecated APIs (⚠️)

### r0 Compatibility
- `/_matrix/client/r0/*` - Legacy r0 endpoints (use v3 instead)
- `/_matrix/client/r0/friendships` - Use `/_matrix/client/v3/friends`
- `/_matrix/client/r0/friends/*` - Use `/_matrix/client/v1/friends/*`

### Legacy Aliases
- `/_matrix/client/v1/friends/requests/incoming` - Use `/_matrix/client/v1/friends/request/received`
- `/_matrix/client/v3/account/profile/*` - Use `/_matrix/client/v3/profile/*`

---

## Error Code Standards

| Error Code | HTTP Status | Use Case |
|------------|-------------|----------|
| `M_NOT_FOUND` | 404 | Resource does not exist |
| `M_FORBIDDEN` | 403 | No permission to access |
| `M_UNSUPPORTED` | 400 | Feature not enabled/supported |
| `M_INVALID_PARAM` | 400 | Invalid request parameter |
| `M_UNRECOGNIZED` | 404 | Unknown endpoint (deprecated, use M_NOT_FOUND) |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-04-12 | Initial API stability classification |
