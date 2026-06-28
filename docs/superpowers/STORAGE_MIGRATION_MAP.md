# Storage Module Ownership Ledger

Audit date: 2026-06-28  
Repo: synapse-rust  
Branch: main  
Commit: 5133355 (HEAD)

## Classification legend

| Class | Meaning | Action |
|-------|---------|--------|
| **Pure facade** | Canon exists in `synapse-storage`, local file has only `pub use` re-exports (no business logic). | Delete local facade, update `mod.rs` to re-export directly from `synapse_storage`. |
| **Local-only** | No canon exists OR local has substantial code (>= 5 body lines of real logic). | Keep in `src/storage/`. |
| **Mixed** | Canon exists AND local has non-trivial code beyond re-exports. | Needs manual inspection before migration. |

## Audit methodology

- Automated first pass: count non-comment, non-blank, non-use, non-`pub use` body lines in each `src/storage/<module>.rs`.
- Manual second pass: inspect every module with body_lines >= 5 to determine whether the body is real logic or just multi-line `pub use { ... }` continuation lines.
- Directory modules (`event/`, `media/`, `room/`) were inspected manually via their `mod.rs` files.

## Summary

| Classification | Count |
|----------------|-------|
| Pure facade    | 55    |
| Local-only     | 0     |
| Mixed          | 0     |
| **Total**      | **55** |

All 55 storage modules are pure facades. Every one has a canonical implementation in `synapse-storage/src/<name>.rs` (or `<name>/mod.rs`). The local files contain zero business logic — only `pub use` re-exports and occasional comments noting that tests were moved.

## Complete ledger

### L0 — Core Matrix storage modules (unconditional)

| # | Module | Local path | Canon path | Lines (canon) | Class | Notes |
|---|--------|-----------|------------|---------------|-------|-------|
| 1 | admin_media | `src/storage/admin_media.rs` | `synapse-storage/src/admin_media.rs` | 229 | Pure facade | |
| 2 | application_service | `src/storage/application_service.rs` | `synapse-storage/src/application_service.rs` | 1109 | Pure facade | |
| 3 | audit | `src/storage/audit.rs` | `synapse-storage/src/audit.rs` | 249 | Pure facade | |
| 4 | background_update | `src/storage/background_update.rs` | `synapse-storage/src/background_update.rs` | 611 | Pure facade | |
| 5 | dehydrated_device | `src/storage/dehydrated_device.rs` | `synapse-storage/src/dehydrated_device.rs` | 311 | Pure facade | |
| 6 | device | `src/storage/device.rs` | `synapse-storage/src/device.rs` | 1253 | Pure facade | |
| 7 | e2ee_audit | `src/storage/e2ee_audit.rs` | `synapse-storage/src/e2ee_audit.rs` | 169 | Pure facade | |
| 8 | email_verification | `src/storage/email_verification.rs` | `synapse-storage/src/email_verification.rs` | 358 | Pure facade | |
| 9 | **event** | `src/storage/event/mod.rs` | `synapse-storage/src/event/` | dir | Pure facade | Directory module |
| 10 | event_report | `src/storage/event_report.rs` | `synapse-storage/src/event_report.rs` | 650 | Pure facade | |
| 11 | feature_flags | `src/storage/feature_flags.rs` | `synapse-storage/src/feature_flags.rs` | 563 | Pure facade | |
| 12 | federation_blacklist | `src/storage/federation_blacklist.rs` | `synapse-storage/src/federation_blacklist.rs` | 627 | Pure facade | |
| 13 | federation_queue | `src/storage/federation_queue.rs` | `synapse-storage/src/federation_queue.rs` | 154 | Pure facade | |
| 14 | invite_blocklist | `src/storage/invite_blocklist.rs` | `synapse-storage/src/invite_blocklist.rs` | 217 | Pure facade | |
| 15 | maintenance | `src/storage/maintenance.rs` | `synapse-storage/src/maintenance.rs` | 272 | Pure facade | |
| 16 | **media** | `src/storage/media/mod.rs` | `synapse-storage/src/media/` | dir | Pure facade | Directory module; re-exports sub-modules too |
| 17 | media_quota | `src/storage/media_quota.rs` | `synapse-storage/src/media_quota.rs` | 688 | Pure facade | |
| 18 | membership | `src/storage/membership.rs` | `synapse-storage/src/membership.rs` | 967 | Pure facade | |
| 19 | moderation | `src/storage/moderation.rs` | `synapse-storage/src/moderation.rs` | 489 | Pure facade | |
| 20 | module | `src/storage/module.rs` | `synapse-storage/src/module.rs` | 957 | Pure facade | Multi-line `pub use`; auto-count = 5 |
| 21 | monitoring | `src/storage/monitoring.rs` | `synapse-storage/src/monitoring.rs` | 260 | Pure facade | |
| 22 | oidc_user_mapping | `src/storage/oidc_user_mapping.rs` | `synapse-storage/src/oidc_user_mapping.rs` | 55 | Pure facade | |
| 23 | performance | `src/storage/performance.rs` | `synapse-storage/src/performance.rs` | 292 | Pure facade | |
| 24 | presence | `src/storage/presence.rs` | `synapse-storage/src/presence.rs` | 540 | Pure facade | |
| 25 | refresh_token | `src/storage/refresh_token.rs` | `synapse-storage/src/refresh_token.rs` | 784 | Pure facade | |
| 26 | registration_token | `src/storage/registration_token.rs` | `synapse-storage/src/registration_token.rs` | 1162 | Pure facade | Multi-line `pub use`; auto-count = 5 |
| 27 | relations | `src/storage/relations.rs` | `synapse-storage/src/relations.rs` | 443 | Pure facade | |
| 28 | retention | `src/storage/retention.rs` | `synapse-storage/src/retention.rs` | 414 | Pure facade | |
| 29 | **room** | `src/storage/room/mod.rs` | `synapse-storage/src/room/` | dir | Pure facade | Directory module |
| 30 | room_tag | `src/storage/room_tag.rs` | `synapse-storage/src/room_tag.rs` | 74 | Pure facade | |
| 31 | schema_health_check | `src/storage/schema_health_check.rs` | `synapse-storage/src/schema_health_check.rs` | 473 | Pure facade | |
| 32 | schema_validator | `src/storage/schema_validator.rs` | `synapse-storage/src/schema_validator.rs` | 213 | Pure facade | |
| 33 | search_index | `src/storage/search_index.rs` | `synapse-storage/src/search_index.rs` | 268 | Pure facade | |
| 34 | sliding_sync | `src/storage/sliding_sync.rs` | `synapse-storage/src/sliding_sync.rs` | 1209 | Pure facade | |
| 35 | space | `src/storage/space.rs` | `synapse-storage/src/space.rs` | 1402 | Pure facade | |
| 36 | state_groups | `src/storage/state_groups.rs` | `synapse-storage/src/state_groups.rs` | 379 | Pure facade | |
| 37 | sticky_event | `src/storage/sticky_event.rs` | `synapse-storage/src/sticky_event.rs` | 198 | Pure facade | |
| 38 | thread | `src/storage/thread.rs` | `synapse-storage/src/thread.rs` | 1196 | Pure facade | |
| 39 | threepid | `src/storage/threepid.rs` | `synapse-storage/src/threepid.rs` | 504 | Pure facade | |
| 40 | token | `src/storage/token.rs` | `synapse-storage/src/token.rs` | 438 | Pure facade | |
| 41 | user | `src/storage/user.rs` | `synapse-storage/src/user.rs` | 1586 | Pure facade | |
| 42 | captcha | `src/storage/captcha.rs` | `synapse-storage/src/captcha.rs` | 423 | Pure facade | Declared after feature-gated block in mod.rs |

### L3 — Feature-gated storage modules

| # | Module | Local path | Canon path | Lines (canon) | Class | Feature gate | Notes |
|---|--------|-----------|------------|---------------|-------|-------------|-------|
| 43 | ai_connection | `src/storage/ai_connection.rs` | `synapse-storage/src/ai_connection.rs` | 128 | Pure facade | `openclaw-routes` | |
| 44 | openclaw | `src/storage/openclaw.rs` | `synapse-storage/src/openclaw.rs` | 929 | Pure facade | `openclaw-routes` | |
| 45 | friend_room | `src/storage/friend_room.rs` | `synapse-storage/src/friend_room.rs` | 943 | Pure facade | `friends` | |
| 46 | voice | `src/storage/voice.rs` | `synapse-storage/src/voice.rs` | 223 | Pure facade | `voice-extended` | |
| 47 | saml | `src/storage/saml.rs` | `synapse-storage/src/saml.rs` | 847 | Pure facade | `saml-sso` | |
| 48 | cas | `src/storage/cas.rs` | `synapse-storage/src/cas.rs` | 727 | Pure facade | `cas-sso` | |
| 49 | beacon | `src/storage/beacon.rs` | `synapse-storage/src/beacon.rs` | 574 | Pure facade | `beacons` | |
| 50 | call_session | `src/storage/call_session.rs` | `synapse-storage/src/call_session.rs` | 201 | Pure facade | `voip-tracking` | |
| 51 | matrixrtc | `src/storage/matrixrtc.rs` | `synapse-storage/src/matrixrtc.rs` | 362 | Pure facade | `voip-tracking` | |
| 52 | widget | `src/storage/widget.rs` | `synapse-storage/src/widget.rs` | 428 | Pure facade | `widgets` | |
| 53 | server_notification | `src/storage/server_notification.rs` | `synapse-storage/src/server_notification.rs` | 1274 | Pure facade | `server-notifications` | Multi-line `pub use`; auto-count = 5 |
| 54 | privacy | `src/storage/privacy.rs` | `synapse-storage/src/privacy.rs` | 294 | Pure facade | `privacy-ext` | |
| 55 | burn_after_read | `src/storage/burn_after_read.rs` | `synapse-storage/src/burn_after_read.rs` | 337 | Pure facade | `burn-after-read` | |

## Direct re-exports (no local facade needed)

These canonical `synapse-storage` modules are already re-exported directly from `src/storage/mod.rs` via `pub use synapse_storage::<module>::{...}` — no local facade file exists and none is needed:

| Module | Re-export line in `mod.rs` |
|--------|---------------------------|
| account_data | 184 |
| filter | 185 |
| openid_token | 186 |
| push | 187 |
| push_notification | 188 |
| qr_login | 192 |
| rate_limit | 193 |
| rendezvous | 194 |
| room_account_data | 199 |
| room_summary | 200 |

## Canonical modules without local reference

These `synapse-storage` modules exist but are NOT declared or re-exported from `src/storage/`:

| Module | Notes |
|--------|-------|
| admin_federation | May be used via direct `synapse_storage::` import elsewhere |
| oauth_client_storage | OAuth-specific storage |
| oidc_session_storage | OIDC session storage |
| pruning | Database pruning |
| test_utils | Test helpers only |
| trigram_ranking | Search ranking (new) |
| url_preview_storage | URL preview |
| user_store_fake | Test fake |
| worker | Worker queue storage |

## Special notes

1. **Directory modules**: `event`, `media`, and `room` use the older `mod.rs`-in-a-directory convention. All three are pure facades. When deleting, the entire directory should be removed.

2. **Multi-line re-export false positives**: `module`, `registration_token`, and `server_notification` were flagged by the automated script as having >= 5 body lines. Manual inspection confirmed they contain only multi-line `pub use { ... }` blocks and a comment — zero business logic. They are pure facades.

3. **Feature gates**: 13 modules (rows 43–55) have `#[cfg(feature = "...")]` guards. When deleting facades, the corresponding `pub use self::<module>::{...}` blocks in mod.rs must retain their feature gates, only changing from `self::` to `synapse_storage::`.

4. **No local-only or mixed modules were found**. This is a clean codebase where every `src/storage/<module>.rs` (or directory) is a thin re-export shim over the canonical `synapse-storage` crate.

5. **`src/storage/mod.rs` also re-exports `Database` and `initialize_database`** directly from `synapse_storage` on lines 267, plus `UserThreepid` from `synapse_storage::threepid` on line 93. These are not separate facade modules and do not need migration.
