//! Outbound federation membership flows: make_join/send_join, make_leave/
//! send_leave, and invite.
//!
//! These methods handle the case where a local user needs to interact with a
//! room or user on a remote homeserver.  The flows follow the Matrix
//! federation specification:
//!
//! - **Join**: `GET /_matrix/federation/v1/make_join` → sign locally →
//!   `PUT /_matrix/federation/v2/send_join` → persist returned state.
//! - **Leave**: `GET /_matrix/federation/v1/make_leave` → sign locally →
//!   `PUT /_matrix/federation/v2/send_leave`.
//! - **Invite**: build invite event → sign locally →
//!   `PUT /_matrix/federation/v2/invite` → persist returned event.
//!
//! Reference: element-hq/synapse
//! `synapse/handlers/federation.py::FederationHandler.do_invite_join` and
//! `synapse/handlers/federation.py::FederationHandler.do_remotely_reject_invite`

use crate::common::error::{ApiError, ApiResult};
use serde_json::{json, Value};
use synapse_common::generate_event_id;
use synapse_federation::signing::sign_and_hash_event;
use synapse_storage::CreateEventParams;

use super::MembershipService;

impl MembershipService {
    // =========================================================================
    // Outbound federation join
    // =========================================================================

    /// Join a room on a remote homeserver via the make_join / send_join flow.
    ///
    /// 1. Call `make_join` on `destination` to get a template PDU.
    /// 2. Sign the template PDU locally.
    /// 3. Call `send_join` on `destination` with the signed PDU.
    /// 4. Create the room locally if it doesn't exist.
    /// 5. Persist the returned state events and auth chain.
    /// 6. Add the user as a joined member.
    pub async fn join_room_via_federation(&self, destination: &str, room_id: &str, user_id: &str) -> ApiResult<()> {
        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            user_id = %user_id,
            "Joining room via federation"
        );

        // Check room ACL before contacting the remote server
        self.check_outbound_server_acl(room_id, destination).await?;

        let federation_client = self.require_federation_client().await?;

        // 1. make_join: get the template event from the remote server.
        let make_join_response = federation_client.make_join(destination, room_id, user_id).await.map_err(|e| {
            ::tracing::warn!(error = %e, destination = %destination, "make_join failed");
            ApiError::bad_request(format!("Remote server rejected make_join: {e}"))
        })?;

        let room_version = make_join_response.room_version.unwrap_or_else(|| "10".to_string());
        let mut event_template = make_join_response.event;

        // 2. Sign the template event locally.
        let signing_key = self.require_signing_key().await?;
        sign_and_hash_event(&self.server_name, &signing_key.key_id, &signing_key.secret_key, &mut event_template)
            .map_err(|e| ApiError::internal(format!("Failed to sign join event: {e}")))?;

        let event_id = event_template
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| generate_event_id(&self.server_name));

        // 3. send_join: send the signed event to the remote server.
        let send_join_response =
            federation_client.send_join(destination, room_id, &event_id, &event_template).await.map_err(|e| {
                ::tracing::warn!(error = %e, destination = %destination, "send_join failed");
                ApiError::bad_request(format!("Remote server rejected send_join: {e}"))
            })?;

        // 4. Create the room locally if it doesn't exist.
        let room_exists = self
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check room existence", &e))?;

        if !room_exists {
            // Derive join_rule and visibility from the returned state events.
            let join_rule = send_join_response
                .state
                .iter()
                .find(|e| {
                    e.get("type").and_then(|v| v.as_str()) == Some("m.room.join_rules")
                        && e.get("state_key").and_then(|v| v.as_str()) == Some("")
                })
                .and_then(|e| e.get("content"))
                .and_then(|c| c.get("join_rule"))
                .and_then(|v| v.as_str())
                .unwrap_or("invite")
                .to_string();

            let is_public = join_rule == "public";

            self.room_storage
                .create_room(room_id, user_id, &join_rule, &room_version, is_public)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to create federated room", &e))?;

            ::tracing::info!(
                room_id = %room_id,
                room_version = %room_version,
                join_rule = %join_rule,
                "Created local record for federated room"
            );
        }

        // 5. Persist the returned state events and auth chain.
        //    We use create_event_with_graph so that event_edges is populated.
        let mut persisted_event_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        for state_event in &send_join_response.state {
            if let Some(event_id) = state_event.get("event_id").and_then(|v| v.as_str()) {
                if persisted_event_ids.contains(event_id) {
                    continue;
                }
                persisted_event_ids.insert(event_id.to_string());

                let prev_events: Vec<String> = state_event
                    .get("prev_events")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                e.as_array()
                                    .and_then(|inner| inner.first())
                                    .and_then(|id| id.as_str())
                                    .map(|s| s.to_string())
                                    .or_else(|| e.as_str().map(|s| s.to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let auth_events: Vec<String> = state_event
                    .get("auth_events")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                e.as_array()
                                    .and_then(|inner| inner.first())
                                    .and_then(|id| id.as_str())
                                    .map(|s| s.to_string())
                                    .or_else(|| e.as_str().map(|s| s.to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let depth = state_event.get("depth").and_then(|v| v.as_i64()).unwrap_or(0);

                let event_type = state_event.get("type").and_then(|v| v.as_str()).unwrap_or("m.unknown").to_string();

                let sender = state_event.get("sender").and_then(|v| v.as_str()).unwrap_or("").to_string();

                let content = state_event.get("content").cloned().unwrap_or(Value::Object(serde_json::Map::new()));

                let state_key = state_event.get("state_key").and_then(|v| v.as_str()).map(|s| s.to_string());

                let origin_server_ts = state_event
                    .get("origin_server_ts")
                    .and_then(|v| v.as_i64())
                    .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

                let redacts = state_event.get("redacts").and_then(|v| v.as_str()).map(|s| s.to_string());

                // Best-effort persistence — skip on error to avoid blocking the join.
                if let Err(e) = self
                    .event_storage
                    .create_event_with_graph(
                        CreateEventParams {
                            event_id: event_id.to_string(),
                            room_id: room_id.to_string(),
                            user_id: sender,
                            event_type,
                            content,
                            state_key,
                            origin_server_ts,
                            redacts,
                        },
                        &prev_events,
                        &auth_events,
                        depth,
                        None,
                    )
                    .await
                {
                    ::tracing::warn!(
                        event_id = %event_id,
                        error = %e,
                        "Failed to persist federated state event during join"
                    );
                }
            }
        }

        // 6. Add the user as a joined member.
        self.member_storage
            .add_member(room_id, user_id, "join", None, None, None, None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add member after federation join", &e))?;

        self.room_storage
            .increment_member_count(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member count after federation join", &e))?;

        // Persist the join event itself.
        let join_event_id = event_template.get("event_id").and_then(|v| v.as_str()).unwrap_or(&event_id).to_string();

        let join_sender = event_template.get("sender").and_then(|v| v.as_str()).unwrap_or(user_id).to_string();

        let join_content = event_template.get("content").cloned().unwrap_or(json!({ "membership": "join" }));

        let join_ts = event_template
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        if let Err(e) = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: join_event_id,
                    room_id: room_id.to_string(),
                    user_id: join_sender,
                    event_type: "m.room.member".to_string(),
                    content: join_content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: join_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist join event after federation join");
        }

        Ok(())
    }

    // =========================================================================
    // Outbound federation leave
    // =========================================================================

    /// Leave a federated room via the make_leave / send_leave flow.
    ///
    /// 1. Call `make_leave` on `destination` to get a template PDU.
    /// 2. Sign the template PDU locally.
    /// 3. Call `send_leave` on `destination` with the signed PDU.
    /// 4. Update local membership to "leave".
    pub async fn leave_room_via_federation(&self, destination: &str, room_id: &str, user_id: &str) -> ApiResult<()> {
        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            user_id = %user_id,
            "Leaving room via federation"
        );

        // Check room ACL before contacting the remote server
        self.check_outbound_server_acl(room_id, destination).await?;

        let federation_client = self.require_federation_client().await?;

        // 1. make_leave: get the template event from the remote server.
        let make_leave_response = federation_client.make_leave(destination, room_id, user_id).await.map_err(|e| {
            ::tracing::warn!(error = %e, destination = %destination, "make_leave failed");
            ApiError::bad_request(format!("Remote server rejected make_leave: {e}"))
        })?;

        let mut event_template = make_leave_response.event;

        // 2. Sign the template event locally.
        let signing_key = self.require_signing_key().await?;
        sign_and_hash_event(&self.server_name, &signing_key.key_id, &signing_key.secret_key, &mut event_template)
            .map_err(|e| ApiError::internal(format!("Failed to sign leave event: {e}")))?;

        let event_id = event_template
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| generate_event_id(&self.server_name));

        // 3. send_leave: send the signed event to the remote server.
        federation_client.send_leave(destination, room_id, &event_id, &event_template).await.map_err(|e| {
            ::tracing::warn!(error = %e, destination = %destination, "send_leave failed");
            ApiError::bad_request(format!("Remote server rejected send_leave: {e}"))
        })?;

        // 4. Update local membership.
        let existing_member = self
            .member_storage
            .get_room_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership before federation leave", &e))?;

        self.member_storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to leave federated room", &e))?;

        if existing_member.as_ref().is_some_and(|member| member.membership == "join") {
            self.room_storage
                .decrement_member_count(room_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update member count after federation leave", &e))?;
        }

        // Persist the leave event locally.
        let leave_sender = event_template.get("sender").and_then(|v| v.as_str()).unwrap_or(user_id).to_string();

        let leave_content = event_template.get("content").cloned().unwrap_or(json!({ "membership": "leave" }));

        let leave_ts = event_template
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        if let Err(e) = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: leave_sender,
                    event_type: "m.room.member".to_string(),
                    content: leave_content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: leave_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist leave event after federation leave");
        }

        Ok(())
    }

    // =========================================================================
    // Outbound federation invite
    // =========================================================================

    /// Invite a remote user to a room via the federation invite flow.
    ///
    /// 1. Build an `m.room.member` invite event.
    /// 2. Sign the event locally.
    /// 3. Call `invite` on the invitee's home server.
    /// 4. The remote server signs the event and returns it.
    /// 5. Persist the signed event locally.
    pub async fn invite_user_via_federation(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        let destination = Self::server_name_from_id(invitee_id)
            .ok_or_else(|| ApiError::bad_request("Invalid invitee ID: missing server name".to_string()))?
            .to_string();

        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            inviter_id = %inviter_id,
            invitee_id = %invitee_id,
            "Inviting remote user via federation"
        );

        // Check room ACL before contacting the remote server
        self.check_outbound_server_acl(room_id, &destination).await?;

        let federation_client = self.require_federation_client().await?;

        // 1. Build the invite event.
        let event_id = generate_event_id(&self.server_name);
        let now = chrono::Utc::now().timestamp_millis();

        let mut invite_event = json!({
            "event_id": event_id,
            "room_id": room_id,
            "sender": inviter_id,
            "user_id": inviter_id,
            "type": "m.room.member",
            "content": {
                "membership": "invite",
                "displayname": invitee_id
                    .trim_start_matches('@')
                    .split(':')
                    .next()
                    .unwrap_or(invitee_id),
            },
            "state_key": invitee_id,
            "origin_server_ts": now,
            "origin": self.server_name,
            "prev_events": [],
            "auth_events": [],
            "depth": 0,
        });

        // 2. Sign the event locally.
        let signing_key = self.require_signing_key().await?;
        sign_and_hash_event(&self.server_name, &signing_key.key_id, &signing_key.secret_key, &mut invite_event)
            .map_err(|e| ApiError::internal(format!("Failed to sign invite event: {e}")))?;

        // 3. Call invite on the remote server.
        let invite_response =
            federation_client.invite(&destination, room_id, &event_id, &invite_event).await.map_err(|e| {
                ::tracing::warn!(error = %e, destination = %destination, "federation invite failed");
                ApiError::bad_request(format!("Remote server rejected invite: {e}"))
            })?;

        // 4. Add the invitee as an invited member locally.
        self.member_storage
            .add_member(room_id, invitee_id, "invite", None, None, Some(inviter_id), None)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to record invite after federation invite", &e))?;

        // 5. Persist the signed event returned by the remote server.
        let final_event = invite_response.event;

        let persisted_event_id = final_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(&event_id).to_string();

        let persisted_sender = final_event.get("sender").and_then(|v| v.as_str()).unwrap_or(inviter_id).to_string();

        let persisted_content = final_event.get("content").cloned().unwrap_or(json!({ "membership": "invite" }));

        let persisted_ts = final_event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(now);

        if let Err(e) = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: persisted_event_id,
                    room_id: room_id.to_string(),
                    user_id: persisted_sender,
                    event_type: "m.room.member".to_string(),
                    content: persisted_content,
                    state_key: Some(invitee_id.to_string()),
                    origin_server_ts: persisted_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist invite event after federation invite");
        }

        Ok(())
    }

    // =========================================================================
    // Outbound exchange_third_party_invite
    // =========================================================================

    /// Exchange a third-party invite on a remote homeserver.
    ///
    /// When a local user wants to join a room via a third-party invite (e.g.
    /// email invite) on a remote server, we send the signed token to the
    /// room's home server.  The home server verifies the token against the
    /// `m.room.third_party_invite` state event, signs the `m.room.member`
    /// invite event, and returns it.  We then persist the signed event
    /// locally.
    ///
    /// Reference: element-hq/synapse
    /// `synapse/handlers/federation.py::FederationHandler.exchange_third_party_invite`
    pub async fn exchange_third_party_invite_via_federation(
        &self,
        destination: &str,
        room_id: &str,
        invite_event: &Value,
    ) -> ApiResult<Value> {
        ::tracing::info!(
            destination = %destination,
            room_id = %room_id,
            "Exchanging third-party invite via federation"
        );

        let federation_client = self.require_federation_client().await?;

        let signed_event =
            federation_client.exchange_third_party_invite(destination, room_id, invite_event).await.map_err(|e| {
                ::tracing::warn!(error = %e, destination = %destination, "exchange_third_party_invite failed");
                ApiError::bad_request(format!("Remote server rejected exchange_third_party_invite: {e}"))
            })?;

        // Persist the signed event locally.
        let event_id = signed_event
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ApiError::internal("Remote server returned event without event_id".to_string()))?;

        let sender = signed_event.get("sender").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let state_key = signed_event.get("state_key").and_then(|v| v.as_str()).map(|s| s.to_string());

        let content = signed_event.get("content").cloned().unwrap_or(json!({ "membership": "invite" }));

        let origin_server_ts = signed_event
            .get("origin_server_ts")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        // Add the invitee as an invited member.
        if let Some(ref invitee_id) = state_key {
            self.member_storage
                .add_member(room_id, invitee_id, "invite", None, None, Some(&sender), None)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to record invite after third-party exchange", &e))?;
        }

        // Persist the event.
        if let Err(e) = self
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id,
                    room_id: room_id.to_string(),
                    user_id: sender,
                    event_type: "m.room.member".to_string(),
                    content,
                    state_key,
                    origin_server_ts,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(error = %e, "Failed to persist third-party invite event");
        }

        Ok(signed_event)
    }
}
