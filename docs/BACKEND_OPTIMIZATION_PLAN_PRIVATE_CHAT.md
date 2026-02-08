# Synapse Backend Private Chat Optimization & Privacy Enhancement Plan

## 1. Executive Summary
This plan outlines the strategy to modernize the HuLa backend (`synapse`) by:
1.  **Removing Redundancy**: Deprecating and removing the custom `private_chat` module in favor of standard Matrix 1-on-1 Direct Rooms.
2.  **Enhancing Privacy**: Implementing backend support for "Burn-after-reading" (Ephemerality) and "Anti-Screenshot" signaling.

## 2. Redundancy Removal (Migration to Standard Matrix)

### 2.1 Context
The current backend maintains two parallel chat systems:
-   **Standard Matrix Rooms**: Supports Federation, E2EE, Multi-device sync.
-   **Custom `private_chat`**: Supports only local 1-on-1, no Federation.

### 2.2 Action Plan
We will decommission the `private_chat` module entirely.

#### 2.2.1 Code Removal
The following files and directories will be removed:
-   `src/services/private_chat_service.rs`
-   `src/web/routes/private_chat.rs`
-   `src/storage/private_chat.rs`
-   `tests/integration/test_private_chat_api.py` (and related tests)

#### 2.2.2 Code Cleanup
-   Remove module registrations in `src/lib.rs`, `src/services/mod.rs`, `src/web/routes/mod.rs`, `src/storage/mod.rs`.
-   Remove `private_chat` related tables (`private_sessions`, `private_messages`) in a future migration (or creating a drop migration now).

#### 2.2.3 Feature Parity Verification
-   **1-on-1 Chat**: Handled by `RoomService::create_room` with `is_direct: true` and `preset: trusted_private_chat`.
-   **Encryption**: Handled by existing `E2eeService`.

## 3. Privacy Feature Implementation

### 3.1 Burn-After-Reading (30s Auto-Delete)

**Challenge**: "Read" status is client-side. Server only receives `m.read` receipts.
**Solution**: Server-Side Enforcement Triggered by Receipts.

#### Architecture
1.  **Message Tagging**: Messages to be burnt must carry metadata (e.g., `content.burn_after_read: 30000` ms).
2.  **Trigger**: The `SyncService` or `RoomService` listens for incoming `m.read` receipts.
3.  **Enforcement**:
    -   When User B sends a Read Receipt for Event E.
    -   Server checks if Event E (or older unread events) has `burn_after_read`.
    -   If yes, Server schedules a **Redaction Job** for `Now + 30s`.
    -   **Physical Deletion**: The redaction must physically scrub the `content` field in the database to ensure "burning".

#### Backend Changes Required
-   **`src/services/room_service.rs`**: Add logic to intercept `m.read` receipts.
-   **`src/common/task_queue.rs`**: Ensure delayed tasks can be scheduled.

### 3.2 Anti-Screenshot Protection

**Challenge**: The backend cannot control the client OS hardware.
**Solution**: Signaling & Policy Enforcement.

#### Architecture
1.  **Room State**: Use a custom state event `com.hula.privacy` in the room.
    ```json
    {
      "type": "com.hula.privacy",
      "state_key": "",
      "content": {
        "screenshot_prevention": true
      }
    }
    ```
2.  **Client Enforcement**: Clients (Android/iOS) listen for this state. If `true`, they enable `FLAG_SECURE` (Android) or blur screens (iOS).
3.  **Backend Role**:
    -   Allow `createRoom` to accept this initial state.
    -   (Optional) Reject media downloads if the client is detected as "insecure" (hard to verify without Attestation).

## 4. Execution Steps (Immediate)

1.  **Delete** `private_chat` source files.
2.  **Clean** module references in `lib.rs` and `mod.rs`.
3.  **Verify** build passes.

## 5. Future Work (Phase 2)
-   Implement the `RedactionJob` for burn-after-reading in `RoomService`.
-   Add `com.hula.privacy` support to default room presets.

---
*Plan created by Trae AI*
