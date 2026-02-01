# Synapse Rust API Reference

> **Version**: 0.1.0  
> **Last Updated**: 2026-02-01  
> **Framework**: Axum + Rust  
> **Database**: PostgreSQL 15  
> **Cache**: Redis 7  

## Overview

synapse-rust is a Matrix protocol server implementation written in Rust. This document provides a comprehensive API reference for all available endpoints.

---

## Test Environment & Compatibility

**Test Environment**
- Deployment: Docker
- Server Image: synapse_rust:0.1.0
- Base URL: http://localhost:8008
- Database: PostgreSQL 15
- Cache: Redis 7

**Compatibility**
- Matrix Client API: r0.x (r0.0.1 ~ r0.6.0)
- E2EE endpoints: r0 + v3 (keys/changes, sendToDevice)
- Federation API: /_matrix/federation + /_matrix/federation/v2 + /_matrix/key/v2

**Support**
- Issues: https://github.com/your-org/synapse-rust-sdk/issues
- Discussions: https://github.com/your-org/synapse-rust-sdk/discussions
- Email: support@example.com

---

## Table of Contents

1. [Client API](#client-api)
2. [Admin API](#admin-api)
3. [Media API](#media-api)
4. [Federation API](#federation-api)
5. [E2EE API](#e2ee-api)
6. [Key Backup API](#key-backup-api)
7. [Friend API](#friend-api)
8. [Private Chat API](#private-chat-api)
9. [Voice API](#voice-api)
10. [Error Codes](#error-codes)
11. [Authentication](#authentication)
12. [Version Compatibility](#version-compatibility)

---

## Client API

### Base Path
```
/_matrix/client/r0 (primary)  
/_matrix/client/v3 (selected endpoints)
```

### Authentication Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/register` | Register a new user |
| GET | `/register/available` | Check username availability |
| POST | `/login` | User login |
| POST | `/logout` | Logout current session |
| POST | `/logout/all` | Logout from all sessions |
| POST | `/refresh` | Refresh access token |
| GET | `/account/whoami` | Get current user info |

### Account Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/account/profile/:user_id` | Get user profile |
| PUT | `/account/profile/:user_id/displayname` | Update display name |
| PUT | `/account/profile/:user_id/avatar_url` | Update avatar URL |
| POST | `/account/password` | Change password |
| POST | `/account/deactivate` | Deactivate account |

### Room Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/createRoom` | Create a new room |
| GET | `/rooms/:room_id/messages` | Get room messages |
| POST | `/rooms/:room_id/send/:event_type` | Send message to room |
| POST | `/rooms/:room_id/join` | Join a room |
| POST | `/rooms/:room_id/leave` | Leave a room |
| POST | `/rooms/:room_id/invite` | Invite user to room |
| GET | `/rooms/:room_id/state` | Get room state |
| GET | `/rooms/:room_id/state/:event_type` | Get state by type |
| GET | `/rooms/:room_id/state/:event_type/:state_key` | Get state event |
| PUT | `/rooms/:room_id/redact/:event_id` | Redact event |
| POST | `/rooms/:room_id/kick` | Kick user from room |
| POST | `/rooms/:room_id/ban` | Ban user from room |
| POST | `/rooms/:room_id/unban` | Unban user from room |

### Room Directory

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/directory/room/:room_id` | Get room details |
| DELETE | `/directory/room/:room_id` | Delete room |
| GET | `/publicRooms` | List public rooms (auth required) |
| POST | `/publicRooms` | Create public room |

### User Rooms

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/user/:user_id/rooms` | Get user's joined rooms |

### Sync

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/sync` | Sync updates |

### Devices

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/devices` | List user devices |
| POST | `/delete_devices` | Bulk delete devices |
| GET | `/devices/:device_id` | Get device details |
| PUT | `/devices/:device_id` | Update device |
| DELETE | `/devices/:device_id` | Delete device |

### Presence

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/presence/:user_id/status` | Get user presence |
| PUT | `/presence/:user_id/status` | Set user presence |

### General

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Server info |
| GET | `/_matrix/client/versions` | Get client versions |

---

## Admin API

### Base Path
```
/_synapse/admin/v1
```

### Server Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/server_version` | Get server version |

### User Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/users` | List all users |
| GET | `/users/:user_id` | Get user details |
| PUT | `/users/:user_id/admin` | Set admin status |
| POST | `/users/:user_id/deactivate` | Deactivate user |
| GET | `/users/:user_id/rooms` | List user rooms |

### Room Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/rooms` | List all rooms |
| GET | `/rooms/:room_id` | Get room details |
| POST | `/rooms/:room_id/delete` | Delete room |

### History Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/purge_history` | Purge room history |
| POST | `/shutdown_room` | Shutdown room |

### Security

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/security/ip/blocks` | List IP blocks |
| POST | `/security/ip/block` | Block IP address |
| POST | `/security/ip/unblock` | Unblock IP address |
| GET | `/security/ip/reputation/:ip` | Get IP reputation |
| GET | `/security/events` | List security events |
| GET | `/status` | Server status |

---

## Media API

### Base Path
```
/_matrix/media/v1  
/_matrix/media/v3
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/_matrix/media/v1/config` | Get media configuration |
| POST | `/_matrix/media/v1/upload` | Upload media file |
| POST | `/_matrix/media/v3/upload` | Upload media file |
| POST | `/_matrix/media/v3/upload/:server_name/:media_id` | Upload media file |
| GET | `/_matrix/media/v1/download/:server_name/:media_id` | Download media |
| GET | `/_matrix/media/r1/download/:server_name/:media_id` | Download media |
| GET | `/_matrix/media/v3/download/:server_name/:media_id` | Download media |
| GET | `/_matrix/media/v3/thumbnail/:server_name/:media_id` | Get media thumbnail |

---

## Federation API

### Base Path
```
/_matrix/federation  
/_matrix/federation/v2  
/_matrix/key/v2
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/_matrix/federation/version` | Get federation version |
| GET | `/_matrix/federation` | Federation discovery |
| PUT | `/_matrix/federation/send/:txn_id` | Send transaction |
| GET | `/_matrix/federation/make_join/:room_id/:user_id` | Make join |
| GET | `/_matrix/federation/make_leave/:room_id/:user_id` | Make leave |
| PUT | `/_matrix/federation/send_join/:room_id/:event_id` | Send join |
| PUT | `/_matrix/federation/send_leave/:room_id/:event_id` | Send leave |
| PUT | `/_matrix/federation/invite/:room_id/:event_id` | Handle federation invite |
| POST | `/_matrix/federation/get_missing_events/:room_id` | Get missing events |
| GET | `/_matrix/federation/get_event_auth/:room_id/:event_id` | Get event auth |
| GET | `/_matrix/federation/state/:room_id` | Get federation state |
| GET | `/_matrix/federation/event/:event_id` | Get event |
| GET | `/_matrix/federation/state_ids/:room_id` | Get state IDs |
| GET | `/_matrix/federation/query/directory/room/:room_id` | Query directory |
| GET | `/_matrix/federation/query/profile/:user_id` | Query profile |
| GET | `/_matrix/federation/backfill/:room_id` | Backfill events |
| POST | `/_matrix/federation/keys/claim` | Claim keys |
| POST | `/_matrix/federation/keys/upload` | Upload keys |
| GET | `/_matrix/federation/v2/server` | Server keys |
| GET | `/_matrix/key/v2/server` | Server keys (key) |
| GET | `/_matrix/federation/v2/query/:server_name/:key_id` | Query server keys |
| GET | `/_matrix/key/v2/query/:server_name/:key_id` | Query server keys (key) |
| POST | `/_matrix/federation/v2/key/clone` | Clone keys |
| POST | `/_matrix/federation/v2/user/keys/query` | Query user keys |

---

## E2EE API

### Base Path
```
/_matrix/client/r0  
/_matrix/client/v3
```

### Device Keys

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/_matrix/client/r0/keys/upload/:device_id` | Upload device keys |
| POST | `/_matrix/client/r0/keys/query` | Query device keys |
| POST | `/_matrix/client/r0/keys/claim` | Claim one-time keys |
| GET | `/_matrix/client/v3/keys/changes` | Key changes |
| GET | `/_matrix/client/r0/rooms/:room_id/keys/distribution` | Room key distribution |
| PUT | `/_matrix/client/v3/sendToDevice/:event_type/:transaction_id` | Send to device |

### Cross Signing

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/_matrix/client/r0/keys/device_signing/upload` | Upload cross signing keys |
| POST | `/_matrix/client/r0/keys/signatures/upload` | Upload signatures |

### One-Time Keys

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/_matrix/client/r0/keys/upload/:device_id` | Upload one-time keys |

---

## Key Backup API

### Base Path
```
/_matrix/client/r0/room_keys
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/version` | Create backup version |
| GET | `/version/:version` | Get backup version |
| PUT | `/version/:version` | Update backup version |
| DELETE | `/version/:version` | Delete backup version |
| GET | `/:version` | Get room keys |
| PUT | `/:version` | Upload room keys |
| POST | `/:version/keys` | Upload room keys (multi) |
| GET | `/:version/keys/:room_id` | Get room keys (room) |
| GET | `/:version/keys/:room_id/:session_id` | Get room keys (session) |

---

## Friend API

### Base Path
```
/_synapse/enhanced
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/friends/search` | Search users |
| GET | `/friends` | Get user's friends |
| POST | `/friend/request` | Send friend request |
| GET | `/friend/requests` | List pending requests |
| POST | `/friend/request/:request_id/accept` | Accept friend request |
| POST | `/friend/request/:request_id/decline` | Decline friend request |
| GET | `/friend/blocks/:user_id` | List blocked users |
| POST | `/friend/blocks/:user_id` | Block user |
| DELETE | `/friend/blocks/:user_id/:blocked_user_id` | Unblock user |
| GET | `/friend/categories/:user_id` | List friend categories |
| POST | `/friend/categories/:user_id` | Create category |
| PUT | `/friend/categories/:user_id/:category_name` | Update category |
| DELETE | `/friend/categories/:user_id/:category_name` | Delete category |
| GET | `/friend/recommendations/:user_id` | Friend recommendations |

---

## Private Chat API

### Base Path
```
/_matrix/client/r0  
/_synapse/enhanced/private
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/_matrix/client/r0/dm` | List DM rooms |
| POST | `/_matrix/client/r0/createDM` | Create DM |
| GET | `/_matrix/client/r0/rooms/:room_id/dm` | DM room details |
| GET | `/_matrix/client/r0/rooms/:room_id/unread` | Unread notifications |
| GET | `/_synapse/enhanced/private/sessions` | List sessions |
| POST | `/_synapse/enhanced/private/sessions` | Create session |
| GET | `/_synapse/enhanced/private/sessions/:session_id` | Session details |
| DELETE | `/_synapse/enhanced/private/sessions/:session_id` | Delete session |
| GET | `/_synapse/enhanced/private/sessions/:session_id/messages` | Session messages |
| POST | `/_synapse/enhanced/private/sessions/:session_id/messages` | Send message |
| DELETE | `/_synapse/enhanced/private/messages/:message_id` | Delete message |
| POST | `/_synapse/enhanced/private/messages/:message_id/read` | Mark read |
| GET | `/_synapse/enhanced/private/unread-count` | Unread count |
| POST | `/_synapse/enhanced/private/search` | Search messages |

---

## Voice API

### Base Path
```
/_matrix/client/r0/voice
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/upload` | Upload voice message |
| GET | `/:message_id` | Get voice message |
| DELETE | `/:message_id` | Delete voice message |
| GET | `/user/:user_id` | Get user voice messages |
| GET | `/room/:room_id` | Get room voice messages |
| GET | `/user/:user_id/stats` | Get user voice stats |

---

## Error Codes

| Code | Description |
|------|-------------|
| M_BAD_JSON | 400 | Invalid JSON or input |
| M_INVALID_PARAM | 400 | Invalid parameter |
| M_UNAUTHORIZED | 401 | Unauthorized |
| M_UNKNOWN_TOKEN | 401 | Unknown or expired token |
| M_FORBIDDEN | 403 | Forbidden |
| M_NOT_FOUND | 404 | Not found |
| M_USER_IN_USE | 409 | Conflict |
| M_LIMIT_EXCEEDED | 429 | Rate limited |
| M_INTERNAL_ERROR | 500 | Internal error |
| M_DB_ERROR | 500 | Database error |
| M_CACHE_ERROR | 500 | Cache error |
| M_AUTH_FAILED | 401 | Authentication failed |
| M_VALIDATION_FAILED | 400 | Validation failed |
| M_INVALID_INPUT | 400 | Invalid input |
| M_DECRYPTION_FAILED | 401 | Decryption failed |
| M_ENCRYPTION_FAILED | 500 | Encryption failed |
| M_CRYPTO_ERROR | 500 | Crypto error |

---

## Authentication

All endpoints (except those marked as public) require authentication via:

1. **Access Token**: Bearer token in `Authorization` header

Example:
```http
Authorization: Bearer YOUR_ACCESS_TOKEN
```

## Federation Signing Requirement

Federation endpoints require `federation.signing_key` (base64 32-byte seed). If not configured, server_key-related endpoints return internal error and federation is unavailable.

---

## Rate Limiting

The server applies rate limiting via middleware. Expect HTTP 429 with `M_LIMIT_EXCEEDED` when exceeded.

---

## Version Compatibility

| Client Version | Status |
|----------------|--------|
| r0.0.1 | Supported |
| r0.1.0 | Supported |
| r0.2.0 | Supported |
| r0.3.0 | Supported |
| r0.4.0 | Supported |
| r0.5.0 | Supported |
| r0.6.0 | Supported |

---

## Response Format

All responses follow the Matrix specification format:

### Success Response
```json
{
  "event_id": "$...",
  "room_id": "!...",
  "type": "m.room.message",
  "content": { ... }
}
```

### Error Response
```json
{
  "errcode": "M_FORBIDDEN",
  "error": "You are not invited to this room"
}
```

---

## Document Changes

| Date | Changes |
|------|---------|
| 2026-02-01 | Aligned endpoints and behaviors with latest test results |

---

## Database Tables

The following tables are used by the API:

### Core Tables
- `users` - User accounts
- `devices` - User devices
- `access_tokens` - Access tokens
- `refresh_tokens` - Refresh tokens
- `rooms` - Room data
- `events` - Room events
- `room_memberships` - Room memberships
- `presence` - User presence

### Enhanced Tables
- `friends` - Friend relationships
- `friend_requests` - Friend requests
- `friend_categories` - Friend categories
- `private_sessions` - Private chat sessions
- `private_messages` - Private messages
- `voice_messages` - Voice messages

### E2EE Tables
- `device_keys` - Device keys
- `cross_signing_keys` - Cross signing keys
- `megolm_sessions` - Megolm sessions
- `inbound_megolm_sessions` - Inbound Megolm sessions
- `key_backups` - Key backups
- `backup_keys` - Backup keys

---

## Monitoring Endpoints

| Endpoint | Description |
|----------|-------------|
| `/_matrix/client/versions` | API version info |
| `/_synapse/admin/v1/server_version` | Server version |
| `/_synapse/admin/v1/status` | Server status |
