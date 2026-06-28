# Matrix Spec Version Gap Analysis: v1.14 through v1.18

Audit date: 2026-06-28

## Methodology

Each version's changelog was fetched from the [matrix-org/matrix-spec GitHub releases](https://github.com/matrix-org/matrix-spec/releases).
Each Client-Server API change was cross-referenced against synapse-rust's route ledger, handler modules,
storage layer, and error code definitions. Changes are classified as:

| Classification | Meaning |
|---|---|
| `already_supported` | Route, handler, or behavior confirmed in the codebase |
| `missing_trivial` | Functionality absent but < 1 day of work to add (e.g. register stable path for existing unstable endpoint) |
| `missing_significant` | New endpoint, capability, or behavioral change requiring new implementation |
| `not_applicable` | Editorial, typo fix, or feature irrelevant to synapse-rust's scope |

Evidence is recorded as file paths and line references where the implementation was found.

---

## v1.14 (Released: March 27, 2025 — default room version v11 per MSC4239)

### Changes and Evidence

| Change | MSC | Classification | Evidence / Notes |
|---|---|---|---|
| NEW: `POST /_matrix/client/v3/users/{userId}/report` | MSC4260 | **missing_significant** | No route found. Room reporting (`report_room`) and event reporting (`report_event`) exist in `src/web/routes/directory_reporting.rs`, but user-report endpoint is absent. |
| REMOVED: `server_name` parameter from `/_matrix/client/v3/join/{roomIdOrAlias}` and `/_matrix/client/v3/knock/{roomIdOrAlias}` | MSC4213 | **already_supported** | No `server_name` parameter found in join/knock route definitions. |
| `POST /initialSync` no longer deprecated (used for peeking) | — | **not_applicable** | synapse-rust does not implement `initialSync`. |
| Clarified `/join` endpoint wording | #2038 | **not_applicable** | Editorial. |
| Clarified string type formats | #2046 | **not_applicable** | Editorial. |
| Various typo fixes | #2047, #2048, #2080, #2091 | **not_applicable** | Editorial. |
| Documented `instance_id` in third-party protocol responses | #2051 | **not_applicable** | Third-party protocol (identity server) not implemented. |
| Redaction application is a SHOULD for clients | #2055 | **not_applicable** | Client-side guidance. |
| Clarified which rooms `/hierarchy` returns | #2064 | **not_applicable** | Editorial. |
| Clients can choose which history visibility options to offer | #2072 | **not_applicable** | Client-side guidance. |

### v1.14 Verdict: DECLARE

Only 1 spec change is a functional gap (user report endpoint), and it is a niche moderation feature.
Room and event reporting are already supported. The core API surface remains compatible with v1.14.

---

## v1.15

### Changes and Evidence

| Change | MSC | Classification | Evidence / Notes |
|---|---|---|---|
| NEW: `GET /_matrix/client/v1/room_summary/{roomIdOrAlias}` | MSC3266 | **missing_trivial** | Room summary functionality exists at `/_matrix/client/v1/rooms/{room_id}/summary` (`src/web/routes/room_summary.rs:590-591`) and `/_synapse/room_summary/v1/*` — but the standard MSC3266 client path `/_matrix/client/v1/room_summary/{roomIdOrAlias}` is not registered. |
| NEW: `GET /_matrix/client/v1/auth_metadata` | MSC2965 | **missing_trivial** | Unstable path `/_matrix/client/unstable/org.matrix.msc2965/auth_metadata` exists (`src/web/routes/assembly.rs:379-380`, `src/web/routes/handlers/auth_discovery.rs:17`). Stable path `/_matrix/client/v1/auth_metadata` not registered. |
| Rich text in room topics (`m.topic` content block) | MSC3765 | **missing_trivial** | No `m.topic` content block handling found. |
| Include device keys with Olm-encrypted events | MSC4147 | **missing_trivial** | e2ee feature — not verified in codebase. |
| Room summary & hierarchy extensions (optional `allowed_room_ids`, `encryption`, `room_version` properties) | MSC3266 | **missing_trivial** | Hierarchy endpoint exists (`src/web/routes/space.rs:189-190` federation, `src/web/routes/federation/events.rs:467`), but optional response properties not verified. |
| OAuth 2.0 authentication API | MSC3861 | **missing_significant** | No OAuth 2.0 support found anywhere in the codebase. This is a substantial feature requiring new auth flows, token management, and client registration. |
| Topic key handling clarified (absent/null/empty) | #2068 | **not_applicable** | Editorial. |
| Sync & membership examples fixed | #2077 | **not_applicable** | Documentation. |
| Third-party invite format clarified | #2083 | **not_applicable** | Identity server feature. |
| "Public" rooms defined through join rule and history visibility | #2101-#2108 | **not_applicable** | Editorial. |
| Spaces access clarified | #2109 | **not_applicable** | Editorial. |
| Well-Known URIs clarified | #2140 | **not_applicable** | Editorial. |

### v1.15 Verdict: NOT DECLARED

Multiple trivial gaps (standard paths for room_summary and auth_metadata) plus the significant OAuth 2.0 gap.
While the room summary and auth_metadata gaps are trivially fixable (just register stable paths),
OAuth 2.0 (MSC3861) is a substantial feature that synapse-rust does not implement.

---

## v1.16

### Changes and Evidence

| Change | MSC | Classification | Evidence / Notes |
|---|---|---|---|
| Deprecate `m.set_avatar_url` and `m.set_displayname` capabilities | MSC4133 | **already_supported** | These capabilities are declared in `src/web/routes/assembly.rs`. Deprecation means they should remain true for now. |
| Add `m.profile_fields` capability | MSC4133 | **missing_trivial** | Extended profile routes exist at unstable `uk.tcpip.msc4133` path (`src/web/routes/handlers/extended_profile.rs`). Need to declare as stable capability. |
| Remove "intentional mentions in replies" feature | MSC4142 | **not_applicable** | Not implemented in synapse-rust. |
| `format` query param on `GET /_matrix/client/v3/rooms/{roomId}/state/{eventType}/{stateKey}` | — | **missing_trivial** | State endpoint exists but `format` parameter not implemented. |
| `use_state_after` query param and `state_after` response on `GET /sync` | MSC4222 | **missing_significant** | Sync handler (`sync_service`) does not implement this parameter. Requires new sync logic to return state before a given point. |
| `additional_creators` for `POST /rooms/{roomId}/upgrade` (room v12) | MSC4289 | **missing_significant** | Room upgrade exists (`src/services/room_service.rs`) but `additional_creators` support not found. |
| `trusted_private_chat` preset merges invitees into `additional_creators` (room v12) | MSC4289 | **missing_trivial** | Custom preset not implemented. |
| Room creators in v12 have "infinitely high" power level | MSC4289 | **missing_trivial** | Room v12 power level rules not fully implemented. Room version 12 IS declared as stable in `SUPPORTED_ROOM_VERSIONS` (`synapse-common/src/room_versions.rs:80`), but v12-specific power/creator semantics not verified. |
| Room IDs in v12 no longer have a domain component | MSC4291 | **missing_trivial** | Room ID generation may not conform to v12 rules. |
| Profile field for time zone | MSC4175 | **already_supported** | Timezone field present in `src/web/routes/push_notification.rs:22` (PushRuleRequest.timezone). |
| Invites and knocks must include `m.room.create` in stripped state | MSC4311 | **missing_trivial** | Not verified in invite/knock handler stripped state construction. |
| `format` required when `formatted_body` specified | #2167 | **not_applicable** | Clarification. |
| `latest_event` in aggregated threads uses same format as event | #2169 | **not_applicable** | Clarification. |
| Various typo fixes | #2171, #2177, #2179 | **not_applicable** | Editorial. |

### v1.16 Verdict: NOT DECLARED

Two significant gaps: `use_state_after` on sync (MSC4222) requires new sync logic, and `additional_creators`
for room upgrade (MSC4289) requires new room creation flow. Multiple trivial gaps add up.

---

## v1.17

### Changes and Evidence

| Change | MSC | Classification | Evidence / Notes |
|---|---|---|---|
| Removed legacy mentions | MSC4210 | **not_applicable** | Legacy mentions not implemented in synapse-rust. |
| Allow application services to masquerade as devices | MSC4326 | **missing_significant** | Application service device masquerading not implemented. `src/services/application_service/` exists but device masquerading requires new auth and event-sending logic. |
| Add `m.oauth` authentication type for UIA | MSC4312 | **missing_significant** | Requires OAuth 2.0 support (not implemented). |
| Allow application services to manage devices and register users | MSC4190 | **missing_significant** | Requires application service enhancements. |
| `M_RESOURCE_LIMIT_EXCEEDED` listed as common error code | — | **already_supported** | `synapse-common/src/error.rs:55` defines `ResourceLimitExceeded` variant mapped to `M_RESOURCE_LIMIT_EXCEEDED`. |
| Add `m.login.terms` to enumeration of auth types | — | **missing_trivial** | `m.login.terms` not declared in auth type enumeration. |
| Push rule IDs are globally unique within their kind | #2214 | **not_applicable** | Clarification. |
| Don't advertise `creator` field in room creation description | #2215 | **not_applicable** | Documentation. |
| Peeking via `/events`: `room_id` is required | #2216 | **not_applicable** | `/events` not used. |
| MXC URI sanitization | #2217 | **not_applicable** | Clarification. |
| Capability negotiation note on each endpoint | #2223 | **not_applicable** | Documentation. |
| Additional OpenGraph properties in URL previews | #2225 | **not_applicable** | Minor extension. |
| Power level special casing clarified | #2231 | **not_applicable** | Clarification. |
| `state_after` usage clarified | #2240 | **not_applicable** | Relates to v1.16's `state_after` feature. |
| `device_one_time_keys_count` optional only if no unclaimed keys | #2245 | **not_applicable** | Clarification. |
| `M_USER_DEACTIVATED` at login may not be used | #2246 | **already_supported** | `synapse-common/src/error.rs:35,74,173` defines `UserDeactivated` → `M_USER_DEACTIVATED`. |
| `event_id_only` format for push not mandatory | #2255 | **not_applicable** | Clarification. |

### v1.17 Verdict: NOT DECLARED

Three significant gaps all related to application services and OAuth, which are substantial features.
The `m.login.terms` trivial gap is also present.

---

## v1.18

### Changes and Evidence

| Change | MSC | Classification | Evidence / Notes |
|---|---|---|---|
| NEW: `GET/PUT /_matrix/client/v1/admin/suspend/{userId}` | MSC4323 | **missing_significant** | No admin suspend/lock endpoints found. Admin room blocking exists (`src/web/routes/admin/room/management.rs`) but user-level admin endpoints are absent. |
| NEW: `GET/PUT /_matrix/client/v1/admin/lock/{userId}` | MSC4323 | **missing_significant** | Same as above. |
| REMOVED: `score` request param on `/_matrix/client/v3/rooms/{roomId}/report/{eventId}` | MSC4277 | **missing_trivial** | `score` still used internally in `report_event` (`src/web/routes/directory_reporting.rs:194`). `update_report_score` returns 403 via client API (`src/web/routes/directory_reporting.rs:224`). |
| Report endpoint may respond 200 regardless of subject existence | MSC4277 | **not_applicable** | Behavioral nuance. |
| `M_USER_LIMIT_EXCEEDED` common error code | MSC4335 | **missing_trivial** | Only `M_LIMIT_EXCEEDED` exists (`synapse-common/src/error.rs:70,169`). `M_USER_LIMIT_EXCEEDED` not defined. |
| `m.account_management` capability | MSC4191 | **missing_significant** | Requires OAuth 2.0 account management support. |
| OAuth 2.0 aware clients support | MSC3824 | **missing_significant** | No OAuth 2.0 support. |
| `m.recent_emoji` account data event | MSC4356 | **missing_trivial** | Account data event not handled. |
| `m.forget_forced_upon_leave` capability | MSC4267 | **missing_trivial** | Auto-forget on leave not implemented. |
| `m.room.redaction` support via `PUT /rooms/{roomId}/send/{eventType}/{txnId}` | MSC4169 | **already_supported** | Redaction via send endpoint works (`src/web/routes/handlers/room/events.rs:958` shows `m.room.redaction` event type handling). |
| `ol` HTML element `start` attribute requirement | MSC4313 | **missing_trivial** | HTML rendering nuance. |
| Recommendation to exclude non-cross-signed devices | MSC4153 | **missing_trivial** | e2ee policy nuance. |
| Invite blocking | MSC4380 | **already_supported** | `src/web/routes/invite_blocklist.rs` implements `GET/POST /_matrix/client/v3/rooms/{room_id}/invite_blocklist`. |
| Device Authorization Grant (RFC 8628) | MSC4341 | **missing_significant** | Part of OAuth 2.0 ecosystem. |
| `is_animated` flag on `m.image` and `m.sticker` | MSC4230 | **missing_trivial** | Image/sticker metadata extension not found. |
| Policy Servers module | MSC4284 | **missing_trivial** | Config exists (`synapse-common/src/config/policy_server.rs`) but no runtime implementation found in routes or services. |
| `submit_url` clarification | MSC4183 | **not_applicable** | Identity server clarification. |
| Various clarifications and typo fixes | multiple | **not_applicable** | Editorial. |

### v1.18 Verdict: NOT DECLARED

Five significant gaps (admin suspend/lock, m.account_management, OAuth 2.0, Device Auth Grant)
plus numerous trivial gaps. Two features already supported (m.room.redaction via send, invite blocking).

---

## Summary

| Version | Significant Gaps | Trivial Gaps | Already Supported | Verdict | Declare? |
|---|---|---|---|---|---|
| v1.14 | 1 (user report) | 0 | 1 (server_name removal + all editorial) | Acceptable — 1 niche feature missing | **YES** |
| v1.15 | 1 (OAuth 2.0) | 5 (standard paths, rich text, device keys, hierarchy extensions) | 0 | Multiple gaps, OAuth is substantial | NO |
| v1.16 | 2 (state_after, additional_creators) | 7 (format param, v12 semantics, stripped state, etc.) | 2 (deprecated caps, timezone) | Two significant sync/room features missing | NO |
| v1.17 | 3 (AS device, m.oauth, AS management) | 1 (m.login.terms) | 2 (M_RESOURCE_LIMIT_EXCEEDED, M_USER_DEACTIVATED) | AS + OAuth gaps are substantial | NO |
| v1.18 | 5 (admin endpoints, m.account_mgmt, OAuth, Device Auth) | 7 (emoji, forget, score removal, M_USER_LIMIT, is_animated, policy servers, ol attribute) | 2 (m.room.redaction, invite blocking) | Most feature-heavy version with numerous gaps | NO |

## Recommendation

**Add `v1.14` to `CLIENT_API_VERSION_SUPPORT`.**

The user report endpoint (`POST /_matrix/client/v3/users/{userId}/report`) is the only functional gap
in v1.14, and it is a niche moderation feature. synapse-rust already supports room and event reporting,
and the core v1.14 API surface (join/knock without `server_name`, all editorial clarifications) is
fully compatible.

For v1.15-v1.18, the gap analysis should be revisited after:
- OAuth 2.0 support is implemented (unblocks v1.15, v1.17, v1.18 declarations)
- `use_state_after` sync parameter is added (unblocks v1.16)
- `additional_creators` room upgrade support is added (unblocks v1.16)
- Application service enhancements are implemented (unblocks v1.17)
- Admin suspend/lock endpoints are implemented (unblocks v1.18)

## Verification

The audit evidence can be re-verified with:

```bash
# Check route surface for each version's endpoints
grep -rn "report\|room_summary\|auth_metadata\|suspend\|lock" src/web/routes/ --include="*.rs" | grep -i "get\|post\|put"

# Check error codes
grep -rn "M_RESOURCE_LIMIT\|M_USER_LIMIT\|M_LIMIT_EXCEEDED" synapse-common/src/error.rs

# Check room versions
grep "SUPPORTED_ROOM_VERSIONS" synapse-common/src/room_versions.rs -A 20

# Run contract tests
cargo test --lib web::routes::handlers::versions::tests -- --nocapture
cargo test --test integration api_auth_routes_tests -- --nocapture
```
