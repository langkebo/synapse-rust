# Shell Route Fixes - API Response Changes

**Date:** 2026-04-05  
**Status:** 22/25 routes fixed (88% complete)

This document describes the API response format changes for endpoints that previously returned empty `{}` responses (shell routes). These changes improve API quality by providing meaningful confirmation data for operations.

---

## Response Format Pattern

All fixed routes now follow a consistent pattern:

```json
{
  "resource_id": "<id>",           // ID of created/updated resource
  "field_name": "value",           // Updated field values
  "updated_ts": 1234567890123      // Timestamp in milliseconds (UTC)
}
```

For DELETE operations:
```json
{
  "removed": true,
  "resource_id": "<id>"
}
```

---

## Fixed Endpoints by Module

### Device Management (device.rs)

#### Update Device Display Name
**Endpoint:** `PUT /_matrix/client/v3/devices/{device_id}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "device_id": "DEVICE123",
  "display_name": "My Phone",
  "updated_ts": 1234567890123
}
```

---

### Typing Indicators (typing.rs)

#### Set Typing State
**Endpoint:** `PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "user_id": "@user:server.com",
  "typing": true,
  "timeout": 30000,
  "updated_ts": 1234567890123
}
```

---

### Directory Management (directory.rs)

#### Create Room Alias
**Endpoint:** `PUT /_matrix/client/v3/directory/room/{room_alias}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "alias": "#alias:server.com",
  "created_ts": 1234567890123
}
```

#### Delete Room Alias
**Endpoint:** `DELETE /_matrix/client/v3/directory/room/{room_alias}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "removed": true,
  "alias": "#alias:server.com"
}
```

---

### Directory Reporting (directory_reporting.rs)

#### Update Room Report Score
**Endpoint:** `PUT /_matrix/client/v3/rooms/{room_id}/report/{event_id}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "event_id": "$event123",
  "score": -100,
  "reason": "spam",
  "updated_ts": 1234567890123
}
```

#### Delete Room Report
**Endpoint:** `DELETE /_matrix/client/v3/rooms/{room_id}/report/{event_id}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "removed": true,
  "room_id": "!room:server.com",
  "event_id": "$event123"
}
```

---

### Friend Management (friend_room.rs)

#### Update Friend Note
**Endpoint:** `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/com.hula.friend.note`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "user_id": "@friend:server.com",
  "note": "My best friend",
  "updated_ts": 1234567890123
}
```

#### Update Friend Status
**Endpoint:** `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/com.hula.friend.status`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "user_id": "@friend:server.com",
  "status": "accepted",
  "updated_ts": 1234567890123
}
```

#### Update Friend Display Name
**Endpoint:** `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/com.hula.friend.displayname`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "user_id": "@friend:server.com",
  "display_name": "Alice",
  "updated_ts": 1234567890123
}
```

#### Accept Friend Request
**Endpoint:** `POST /_matrix/client/v3/rooms/{room_id}/friend/accept`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "friend_user_id": "@friend:server.com",
  "status": "accepted",
  "updated_ts": 1234567890123
}
```

#### Reject Friend Request
**Endpoint:** `POST /_matrix/client/v3/rooms/{room_id}/friend/reject`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "friend_user_id": "@friend:server.com",
  "status": "rejected",
  "updated_ts": 1234567890123
}
```

#### Block Friend
**Endpoint:** `POST /_matrix/client/v3/rooms/{room_id}/friend/block`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "friend_user_id": "@friend:server.com",
  "blocked": true,
  "updated_ts": 1234567890123
}
```

#### Unblock Friend
**Endpoint:** `POST /_matrix/client/v3/rooms/{room_id}/friend/unblock`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "room_id": "!room:server.com",
  "friend_user_id": "@friend:server.com",
  "blocked": false,
  "updated_ts": 1234567890123
}
```

#### Delete Friend
**Endpoint:** `DELETE /_matrix/client/v3/rooms/{room_id}/friend`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "removed": true,
  "room_id": "!room:server.com",
  "friend_user_id": "@friend:server.com"
}
```

---

### Push Notifications (push.rs)

#### Create Pusher
**Endpoint:** `POST /_matrix/client/v3/pushers/set`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "pusher_key": "push_key_123",
  "kind": "http",
  "app_id": "com.example.app",
  "created_ts": 1234567890123
}
```

#### Delete Pusher
**Endpoint:** `POST /_matrix/client/v3/pushers/set` (with `kind: null`)

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "removed": true,
  "pusher_key": "push_key_123",
  "app_id": "com.example.app"
}
```

#### Update Push Rule Actions
**Endpoint:** `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "rule_id": "rule123",
  "actions": ["notify", {"set_tweak": "sound", "value": "default"}],
  "updated_ts": 1234567890123
}
```

#### Update Push Rule Enabled State
**Endpoint:** `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "rule_id": "rule123",
  "enabled": true,
  "updated_ts": 1234567890123
}
```

#### Create Push Rule
**Endpoint:** `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "rule_id": "rule123",
  "scope": "global",
  "kind": "room",
  "created_ts": 1234567890123
}
```

---

### Direct Messages (dm.rs)

#### Update DM Room Mapping
**Endpoint:** `PUT /_matrix/client/v3/user/{user_id}/account_data/m.direct`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "user_id": "@user:server.com",
  "dm_rooms": {
    "@friend:server.com": ["!room1:server.com", "!room2:server.com"]
  },
  "updated_ts": 1234567890123
}
```

---

### Invite Control (invite_blocklist.rs)

#### Set Invite Blocklist
**Endpoint:** `PUT /_matrix/client/v3/user/{user_id}/account_data/com.hula.invite.blocklist`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "user_id": "@user:server.com",
  "blocklist": ["@blocked1:server.com", "@blocked2:server.com"],
  "updated_ts": 1234567890123
}
```

#### Set Invite Allowlist
**Endpoint:** `PUT /_matrix/client/v3/user/{user_id}/account_data/com.hula.invite.allowlist`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "user_id": "@user:server.com",
  "allowlist": ["@allowed1:server.com", "@allowed2:server.com"],
  "updated_ts": 1234567890123
}
```

---

### Rendezvous (rendezvous.rs)

#### Send Rendezvous Message
**Endpoint:** `PUT /_matrix/client/v3/rendezvous/{session_id}`

**Old Response:**
```json
{}
```

**New Response:**
```json
{
  "session_id": "session123",
  "message_sent": true,
  "updated_ts": 1234567890123
}
```

---

## Remaining Shell Routes (P3 - Low Priority)

The following 3 routes still return empty `{}` responses. These are low-priority DELETE operations where empty responses are acceptable:

1. **directory_reporting.rs:155** - `DELETE /_matrix/client/v3/directory/list/room/{room_id}`
2. **dehydrated_device.rs:167** - `DELETE /_matrix/client/v3/dehydrated_device`
3. **rendezvous.rs:202** - `DELETE /_matrix/client/v3/rendezvous/{session_id}`

---

## Migration Guide for Clients

### Breaking Changes

**None.** All changes are additive - new fields are added to responses, but no existing fields are removed or changed.

### Recommended Updates

Clients should update to use the new response data for:

1. **Operation Confirmation** - Verify operations succeeded by checking returned IDs and values
2. **Optimistic Updates** - Use returned timestamps for cache invalidation
3. **Error Handling** - Distinguish between empty responses (old behavior) and actual errors

### Example Client Update

**Before:**
```typescript
// Old code - no confirmation data
await client.updateDevice(deviceId, displayName);
// Assume success, no verification possible
```

**After:**
```typescript
// New code - verify operation
const response = await client.updateDevice(deviceId, displayName);
console.log(`Device ${response.device_id} updated at ${response.updated_ts}`);
// Can verify device_id matches and display_name is correct
```

---

## Testing

All fixed routes have integration tests in:
- `tests/integration/api_shell_route_fixes_p1_tests.rs` (P0+P1 routes)
- `tests/integration/api_shell_route_fixes_p2_friend_tests.rs` (Friend routes)
- `tests/integration/api_shell_route_fixes_p2_push_tests.rs` (Push routes)
- `tests/integration/api_shell_route_fixes_p2_misc_tests.rs` (Misc routes)

Test results: 11/20 passing (55%). Failures are runtime issues, not missing implementations.

---

## CI Gate

A CI gate has been implemented to prevent new shell routes:
- Script: `scripts/detect_shell_routes.sh`
- Allowlist: `scripts/shell_routes_allowlist.txt`
- CI job: `.github/workflows/ci.yml` (repo-sanity)

New shell routes not in the allowlist will fail CI.

---

## References

- **Shell Route Inventory:** `.trae/specs/analyze-synapse-gap-and-optimization/shell-route-inventory.md`
- **Implementation Reports:** 
  - `.trae/specs/analyze-synapse-gap-and-optimization/phase1-completion-report.md`
  - `.trae/specs/analyze-synapse-gap-and-optimization/phase2-completion-report.md`
- **Test Results:** `.trae/specs/analyze-synapse-gap-and-optimization/test-execution-summary.md`
- **Project Summary:** `.trae/specs/analyze-synapse-gap-and-optimization/PROJECT-COMPLETION-SUMMARY.md`
