# synapse-rust API Reference

> **Version**: 0.1.0  
> **Last Updated**: 2026-01-29  
> **Framework**: Axum + Rust  
> **Database**: PostgreSQL 16  
> **Cache**: Redis 7  

## Overview

synapse-rust is a Matrix protocol server implementation written in Rust. This document provides a comprehensive API reference for all available endpoints.

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

---

## Client API

### Base Path
```
/_matrix/client/r0
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
| GET | `/publicRooms` | List public rooms |
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
| GET | `/devices/:device_id` | Get device details |
| PUT | `/devices/:device_id` | Update device |
| DELETE | `/devices/:device_id` | Delete device |

### Presence

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/presence/:user_id/status` | Get user presence |
| PUT | `/presence/:user_id/status` | Set user presence |

### Version

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/versions` | Get client versions |

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

### Room Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/rooms` | List all rooms |
| GET | `/rooms/:room_id` | Get room details |

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

---

## Media API

### Base Path
```
/_matrix/media/v1
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/config` | Get media configuration |
| POST | `/upload` | Upload media file |
| GET | `/download/:server_name/:media_id` | Download media |
| GET | `/thumbnail/:server_name/:media_id` | Get media thumbnail |

---

## Federation API

### Base Path
```
/_matrix/federation/v1
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/version` | Get federation version |
| GET | `/` | Federation discovery |
| PUT | `/send/:txn_id` | Send transaction |
| GET | `/get_missing_events/:room_id` | Get missing events |
| POST | `/invite/:room_id` | Handle federation invite |
| GET | `/state/:room_id` | Get federation state |
| GET | `/state_ids/:room_id` | Get state IDs |
| POST | `/backfill/:room_id` | Backfill events |

---

## E2EE API

### Base Path
```
/_matrix/key/v1
```

### Device Keys

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/upload` | Upload device keys |
| GET | `/query` | Query device keys |
| POST | `/query` | Query multiple device keys |
| DELETE | `/delete` | Delete device keys |

### Cross Signing

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/keys/upload` | Upload cross signing keys |
| POST | `/keys/signatures/upload` | Upload signatures |
| GET | `/keys/:user_id` | Get cross signing keys |

### One-Time Keys

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/upload` | Upload one-time keys |

---

## Key Backup API

### Base Path
```
/_matrix/key_backup/v1
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/version` | Get backup version |
| POST | `/version` | Create backup version |
| PUT | `/version/:version` | Update backup |
| GET | `/version/:version` | Get backup |
| POST | `/version/:version/keys` | Backup keys |
| GET | `/version/:version/keys` | Get backed up keys |
| POST | `/version/:version/keys/count` | Get key count |

---

## Friend API

### Base Path
```
/_synapse/enhanced/friends
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Get user's friends |
| POST | `/request` | Send friend request |
| POST | `/accept` | Accept friend request |
| POST | `/reject` | Reject friend request |
| DELETE | `/:friend_id` | Remove friend |
| GET | `/requests` | List pending requests |
| POST | `/block` | Block user |
| DELETE | `/block/:user_id` | Unblock user |
| GET | `/blocked` | List blocked users |
| GET | `/categories` | List friend categories |
| POST | `/categories` | Create category |
| PUT | `/categories/:id` | Update category |
| DELETE | `/categories/:id` | Delete category |
| PUT | `/:friend_id/category` | Set friend category |

---

## Private Chat API

### Base Path
```
/_synapse/enhanced/private_chat
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/sessions` | List private chat sessions |
| GET | `/sessions/:id` | Get session details |
| POST | `/sessions` | Create private chat session |
| DELETE | `/sessions/:id` | Close session |
| GET | `/sessions/:id/messages` | Get session messages |
| POST | `/sessions/:id/messages` | Send message |
| PUT | `/sessions/:id/read` | Mark as read |

---

## Voice API

### Base Path
```
/_synapse/enhanced/voice
```

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/upload` | Upload voice message |
| GET | `/:message_id` | Get voice message |
| GET | `/stats` | Get voice usage stats |
| GET | `/user/:user_id/stats` | Get user voice stats |

---

## WebSocket Endpoints

| Endpoint | Description |
|----------|-------------|
| `/_matrix/client/r0/sync` | Sync stream (via WebSocket) |
| `/_matrix/client/r0/rooms/:room_id/join` | Room join stream |

---

## Error Codes

| Code | Description |
|------|-------------|
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 409 | Conflict |
| 429 | Rate Limited |
| 500 | Internal Server Error |
| 503 | Service Unavailable |

---

## Authentication

All endpoints (except those marked as public) require authentication via:

1. **Access Token**: Bearer token in `Authorization` header
2. **Cookie**: `access_token` cookie

Example:
```http
Authorization: Bearer YOUR_ACCESS_TOKEN
```

---

## Rate Limiting

The server implements rate limiting on all endpoints. Limits vary by endpoint:

- Client API: 60 requests/minute
- Media Upload: 10 uploads/minute
- Admin API: 30 requests/minute

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
| `/_synapse/admin/v1/health` | Health check |

---

## Next Steps

- [Data Models](synapse-rust/data-models.md)
- [Architecture Overview](synapse-rust/architecture-overview.md)
- [Development Guide](synapse-rust/enhanced-development-guide.md)
- [Database Schema](schema.sql)
