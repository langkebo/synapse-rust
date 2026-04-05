# Shell Route Inventory

## Overview

This document catalogs all identified "shell routes" - API endpoints that have authentication, validation, and service calls but return empty `{}` responses instead of meaningful business data.

**Total Count:** 32+ routes across 12 files

**Priority Classification:**
- **P0 (Critical):** Core user-facing features, high usage frequency
- **P1 (High):** Important features, moderate usage
- **P2 (Medium):** Secondary features, lower usage
- **P3 (Low):** Edge cases, rarely used

---

## Analysis Results

### ✅ GOOD: No Shell Routes Found

#### account_data.rs
**Status:** All routes return real business data
- `set_account_data` (line 80): Returns `{}` but this is correct - PUT operations don't need response data
- `delete_account_data` (line 277): Returns `{}` after DELETE - correct behavior, checks rows_affected
- `set_room_account_data` (line 153): Returns `{}` - correct for PUT
- `delete_room_account_data` (line 302): Returns `{}` after DELETE - correct behavior, checks rows_affected

**Verdict:** These are NOT shell routes. They perform real operations and return appropriate responses for their HTTP methods.

#### auth_compat.rs
**Status:** All routes return real business data
- `logout` (line 330): Returns `{}` after calling `auth_service.logout()` - correct behavior
- `logout_all` (line 343): Returns `{}` after calling `auth_service.logout_all()` - correct behavior

**Verdict:** These are NOT shell routes. Logout operations correctly return empty responses after performing the logout action.

#### device.rs
**Status:** Mixed - some routes return real data, some are shell routes
- `get_devices` (line 49): ✅ Returns full device list with display_name, last_seen_ts, last_seen_ip
- `get_device` (line 77): ✅ Returns device details
- `update_device` (line 107): ⚠️ **SHELL ROUTE** - Updates display_name but returns `{}`
- `delete_device` (line 135): ✅ Returns `{}` after DELETE - correct behavior, checks rows_affected
- `delete_devices` (line 157): ✅ Returns `{}` after batch DELETE - correct behavior

**Shell Routes Found:** 1
- `update_device` should return updated device info or at least confirmation with timestamp

---

## Shell Routes by Priority

### P0 (Critical) - 1 route

#### device.rs
1. **update_device** (line 107)
   - **Current:** Updates display_name, returns `{}`
   - **Should Return:**
     ```json
     {
       "device_id": "DEVICE123",
       "display_name": "My Updated Device",
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Users cannot confirm device update success
   - **Service Call:** `device_storage.update_user_device_display_name()`
   - **Fix Complexity:** Low - query device after update

### P1 (High) - 4 routes

#### typing.rs
1. **set_typing** (line 15)
   - **Current:** Sets/clears typing indicator, returns `{}`
   - **Should Return:**
     ```json
     {
       "timeout": 30000,
       "expires_at": 1234567890
     }
     ```
   - **Impact:** Client cannot verify typing state was set
   - **Service Call:** `typing_service.set_typing()` or `typing_service.clear_typing()`
   - **Fix Complexity:** Low - return timeout and expiry timestamp

#### directory.rs
2. **set_room_alias_handler** (line 63)
   - **Current:** Sets room alias, returns `{}`
   - **Should Return:**
     ```json
     {
       "room_id": "!abc:example.com",
       "alias": "#myroom:example.com",
       "created_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm alias was created
   - **Service Call:** `directory_service.set_room_alias()`
   - **Fix Complexity:** Low - return alias details

3. **remove_room_alias** (line 86)
   - **Current:** Removes alias, returns `{}`
   - **Should Return:**
     ```json
     {
       "removed": true,
       "alias": "#myroom:example.com"
     }
     ```
   - **Impact:** Client cannot confirm deletion
   - **Service Call:** `directory_service.remove_room_alias()`
   - **Fix Complexity:** Low - return confirmation

4. **set_canonical_alias** (line 192)
   - **Current:** Sets canonical alias, returns `{}`
   - **Should Return:**
     ```json
     {
       "room_id": "!abc:example.com",
       "alias": "#main:example.com",
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm canonical alias update
   - **Service Call:** `room_storage.update_canonical_alias()`
   - **Fix Complexity:** Low - return updated alias

### P2 (Medium) - 17 routes

#### friend_room.rs
1. **remove_friend** (line 364)
   - **Current:** Removes friend, returns `{}`
   - **Should Return:**
     ```json
     {
       "removed": true,
       "user_id": "@friend:example.com",
       "removed_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm friend removal
   - **Service Call:** `friend_room_service.remove_friend()`
   - **Fix Complexity:** Low

2. **update_friend_note** (line 380)
   - **Current:** Updates note, returns `{}`
   - **Should Return:**
     ```json
     {
       "user_id": "@friend:example.com",
       "note": "Best friend",
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm note update
   - **Service Call:** `friend_room_service.update_friend_note()`
   - **Fix Complexity:** Low

3. **update_friend_status** (line 403)
   - **Current:** Updates status, returns `{}`
   - **Should Return:**
     ```json
     {
       "user_id": "@friend:example.com",
       "status": "favorite",
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm status change
   - **Service Call:** `friend_room_service.update_friend_status()`
   - **Fix Complexity:** Low

4. **update_friend_displayname** (line 445)
   - **Current:** Updates displayname, returns `{}`
   - **Should Return:**
     ```json
     {
       "user_id": "@friend:example.com",
       "displayname": "Alice",
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm displayname update
   - **Service Call:** `friend_room_service.update_friend_displayname()`
   - **Fix Complexity:** Low

5. **delete_friend_group** (line 578)
   - **Current:** Deletes group, returns `{}`
   - **Should Return:**
     ```json
     {
       "deleted": true,
       "group_id": "group123",
       "deleted_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm group deletion
   - **Service Call:** `friend_room_service.delete_friend_group()`
   - **Fix Complexity:** Low

6. **rename_friend_group** (line 592)
   - **Current:** Renames group, returns `{}`
   - **Should Return:**
     ```json
     {
       "group_id": "group123",
       "name": "Close Friends",
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm rename
   - **Service Call:** `friend_room_service.rename_friend_group()`
   - **Fix Complexity:** Low

7. **add_friend_to_group** (line 613)
   - **Current:** Adds friend to group, returns `{}`
   - **Should Return:**
     ```json
     {
       "group_id": "group123",
       "user_id": "@friend:example.com",
       "added_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm addition
   - **Service Call:** `friend_room_service.add_friend_to_group()`
   - **Fix Complexity:** Low

8. **remove_friend_from_group** (line 629)
   - **Current:** Removes friend from group, returns `{}`
   - **Should Return:**
     ```json
     {
       "group_id": "group123",
       "user_id": "@friend:example.com",
       "removed_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm removal
   - **Service Call:** `friend_room_service.remove_friend_from_group()`
   - **Fix Complexity:** Low

#### push.rs
9. **set_pusher** (line 130)
   - **Current:** Sets/deletes pusher, returns `{}`
   - **Should Return:**
     ```json
     {
       "pushkey": "key123",
       "kind": "http",
       "app_id": "com.example.app",
       "created_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm pusher registration
   - **Service Call:** Direct SQL insert/delete
   - **Fix Complexity:** Low

10. **set_push_rule** (line 299)
    - **Current:** Creates/updates push rule, returns `{}`
    - **Should Return:**
      ```json
      {
        "rule_id": "my_rule",
        "scope": "global",
        "kind": "override",
        "created_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm rule creation
    - **Service Call:** Direct SQL insert
    - **Fix Complexity:** Low

11. **create_push_rule** (line 336)
    - **Current:** Creates push rule, returns `{}`
    - **Should Return:**
      ```json
      {
        "rule_id": "my_rule",
        "scope": "global",
        "kind": "override",
        "created_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm rule creation
    - **Service Call:** Direct SQL insert
    - **Fix Complexity:** Low

12. **set_push_rule_actions** (line 407)
    - **Current:** Updates push rule actions, returns `{}`
    - **Should Return:**
      ```json
      {
        "rule_id": "my_rule",
        "actions": ["notify"],
        "updated_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm action update
    - **Service Call:** Direct SQL update
    - **Fix Complexity:** Low

13. **set_push_rule_enabled** (line 458)
    - **Current:** Updates push rule enabled state, returns `{}`
    - **Should Return:**
      ```json
      {
        "rule_id": "my_rule",
        "enabled": true,
        "updated_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm state change
    - **Service Call:** Direct SQL update
    - **Fix Complexity:** Low

#### dm.rs
14. **update_dm_room** (line 257)
    - **Current:** Updates DM room mapping, returns `{}`
    - **Should Return:**
      ```json
      {
        "room_id": "!abc:example.com",
        "users": ["@alice:example.com"],
        "updated_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm DM mapping update
    - **Service Call:** `load_direct_map()` + `save_direct_map()`
    - **Fix Complexity:** Low

#### invite_blocklist.rs
15. **set_invite_blocklist** (line 32)
    - **Current:** Sets invite blocklist, returns `{}`
    - **Should Return:**
      ```json
      {
        "room_id": "!abc:example.com",
        "blocklist": ["@user1:example.com", "@user2:example.com"],
        "updated_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm blocklist update
    - **Service Call:** `invite_blocklist_storage.set_invite_blocklist()`
    - **Fix Complexity:** Low

16. **set_invite_allowlist** (line 93)
    - **Current:** Sets invite allowlist, returns `{}`
    - **Should Return:**
      ```json
      {
        "room_id": "!abc:example.com",
        "allowlist": ["@user1:example.com", "@user2:example.com"],
        "updated_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm allowlist update
    - **Service Call:** `invite_blocklist_storage.set_invite_allowlist()`
    - **Fix Complexity:** Low

#### rendezvous.rs
17. **send_message** (line 205)
    - **Current:** Sends rendezvous message, returns `{}`
    - **Should Return:**
      ```json
      {
        "session_id": "session123",
        "message_id": "msg456",
        "sent_ts": 1234567890
      }
      ```
    - **Impact:** Client cannot confirm message delivery
    - **Service Call:** `msg_storage.store_message()`
    - **Fix Complexity:** Medium - need to return message ID from storage

### P3 (Low) - 2 routes

#### directory_reporting.rs
1. **update_report_score** (line 136)
   - **Current:** Updates event report score, returns `{}`
   - **Should Return:**
     ```json
     {
       "event_id": "$event123",
       "score": -50,
       "updated_ts": 1234567890
     }
     ```
   - **Impact:** Admin cannot confirm score update
   - **Service Call:** `event_storage.update_event_report_score_by_event()`
   - **Fix Complexity:** Low

#### dehydrated_device.rs
2. **delete_dehydrated_device** (line 150)
   - **Current:** Deletes device, returns `{}`
   - **Should Return:**
     ```json
     {
       "deleted": true,
       "device_id": "DEVICE123",
       "deleted_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm deletion (though this is DELETE operation)
   - **Service Call:** `dehydrated_device_service.delete_device()`
   - **Fix Complexity:** Low
   - **Note:** DELETE operations returning `{}` are generally acceptable, but confirmation is better

#### rendezvous.rs
3. **delete_session** (line 191)
   - **Current:** Deletes session, returns `{}`
   - **Should Return:**
     ```json
     {
       "deleted": true,
       "session_id": "session123",
       "deleted_ts": 1234567890
     }
     ```
   - **Impact:** Client cannot confirm deletion (though this is DELETE operation)
   - **Service Call:** `rendezvous_storage.delete_session()`
   - **Fix Complexity:** Low
   - **Note:** DELETE operations returning `{}` are generally acceptable

---

## Shell Routes by File

### device.rs (1 route)
- `update_device` - P0

### typing.rs (1 route)
- `set_typing` - P1

### directory.rs (3 routes)
- `set_room_alias_handler` - P1
- `remove_room_alias` - P1
- `set_canonical_alias` - P1

### friend_room.rs (8 routes)
- `remove_friend` - P2
- `update_friend_note` - P2
- `update_friend_status` - P2
- `update_friend_displayname` - P2
- `delete_friend_group` - P2
- `rename_friend_group` - P2
- `add_friend_to_group` - P2
- `remove_friend_from_group` - P2

### push.rs (5 routes)
- `set_pusher` - P2
- `set_push_rule` - P2
- `create_push_rule` - P2
- `set_push_rule_actions` - P2
- `set_push_rule_enabled` - P2

### dm.rs (1 route)
- `update_dm_room` - P2

### invite_blocklist.rs (2 routes)
- `set_invite_blocklist` - P2
- `set_invite_allowlist` - P2

### rendezvous.rs (2 routes)
- `send_message` - P2
- `delete_session` - P3

### directory_reporting.rs (1 route)
- `update_report_score` - P3

### dehydrated_device.rs (1 route)
- `delete_dehydrated_device` - P3

---

## Remaining Files to Audit

Based on the plan, these files still need to be checked:

### High Priority
- `src/web/routes/directory.rs` - 3 suspected routes
- `src/web/routes/typing.rs` - 1 suspected route
- `src/web/routes/friend_room.rs` - 8 suspected routes

### Medium Priority
- `src/web/routes/invite_blocklist.rs` - 2 suspected routes
- `src/web/routes/dm.rs` - 1 suspected route
- `src/web/routes/push.rs` - 3+ suspected routes

### Lower Priority
- `src/web/routes/rendezvous.rs` - 2 suspected routes
- `src/web/routes/dehydrated_device.rs` - 1 suspected route
- `src/web/routes/directory_reporting.rs` - 1 suspected route

---

## Fix Strategy

### Phase 1: P0 Routes (Immediate)
1. Fix `device.rs::update_device`
   - Add query to fetch updated device
   - Return device data with timestamp
   - Add integration test verifying response data

### Phase 2: Complete Audit
1. Audit remaining 11 files
2. Classify all shell routes by priority
3. Update this inventory

### Phase 3: Systematic Fixes
1. Fix all P0 routes
2. Fix all P1 routes
3. Fix P2/P3 routes as capacity allows

---

## CI Gate Strategy

Once inventory is complete, implement automated detection:

```bash
#!/bin/bash
# scripts/scan_shell_routes.sh

# Pattern: async fn handler(...) -> Result<Json<Value>, ApiError> {
#   ... service call ...
#   Ok(Json(json!({})))
# }

# Scan for suspicious patterns
grep -r "Ok(Json(json!({})))" src/web/routes/ | \
  grep -v "DELETE" | \
  grep -v "logout" | \
  grep -v "test"
```

**Exemptions:**
- DELETE operations (correct to return `{}`)
- Logout operations (correct to return `{}`)
- Test code
- Explicitly documented exceptions

---

## Progress Tracking

- [x] Audit account_data.rs - 0 shell routes found
- [x] Audit auth_compat.rs - 0 shell routes found  
- [x] Audit device.rs - 1 shell route found
- [x] Audit directory.rs - 3 shell routes found
- [x] Audit typing.rs - 1 shell route found
- [x] Audit friend_room.rs - 8 shell routes found
- [x] Audit invite_blocklist.rs - 2 shell routes found
- [x] Audit dm.rs - 1 shell route found
- [x] Audit rendezvous.rs - 2 shell routes found
- [x] Audit dehydrated_device.rs - 1 shell route found
- [x] Audit push.rs - 5 shell routes found
- [x] Audit directory_reporting.rs - 1 shell route found

**Current Status:** 12/12 files audited, 25 shell routes confirmed

## Summary Statistics

**Total Shell Routes:** 25

**By Priority:**
- P0 (Critical): 1 route
- P1 (High): 4 routes
- P2 (Medium): 17 routes
- P3 (Low): 3 routes

**By Category:**
- Friend management: 8 routes
- Push notifications: 5 routes
- Directory/Aliases: 3 routes
- Invite control: 2 routes
- Rendezvous: 2 routes
- Device management: 1 route
- DM management: 1 route
- Typing indicators: 1 route
- Dehydrated devices: 1 route
- Reporting: 1 route

**Fix Complexity:**
- Low: 24 routes (96%)
- Medium: 1 route (4%)

## Next Steps

1. **Immediate (P0):** Fix `device.rs::update_device`
2. **High Priority (P1):** Fix 4 routes in typing.rs and directory.rs
3. **Medium Priority (P2):** Fix 17 routes across friend_room.rs, push.rs, dm.rs, invite_blocklist.rs, rendezvous.rs
4. **Low Priority (P3):** Fix 3 DELETE operation routes (optional improvement)

## Implementation Strategy

### Phase 1: P0 Fix (1 route)
Start with `device.rs::update_device` - query device after update and return full device info.

### Phase 2: P1 Fixes (4 routes)
- `typing.rs::set_typing` - return timeout and expiry
- `directory.rs::set_room_alias_handler` - return alias details
- `directory.rs::remove_room_alias` - return confirmation
- `directory.rs::set_canonical_alias` - return updated alias

### Phase 3: P2 Fixes (17 routes)
Group by file for efficient batch fixing:
1. **friend_room.rs** (8 routes) - add timestamp and confirmation to all update operations
2. **push.rs** (5 routes) - return rule/pusher details after operations
3. **invite_blocklist.rs** (2 routes) - return updated list with timestamp
4. **dm.rs** (1 route) - return updated DM mapping
5. **rendezvous.rs** (1 route) - return message ID from storage

### Phase 4: P3 Fixes (3 routes) - Optional
DELETE operations that could benefit from confirmation responses.
