use crate::common::ApiError;
use crate::storage::call_session::{CallSession, CallSessionStorage, CreateCallSessionParams};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 呼叫状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallState {
    RingING,
    Connected,
    Held,
    Ended,
}

impl CallState {
    pub fn as_str(&self) -> &str {
        match self {
            CallState::RingING => "ringing",
            CallState::Connected => "connected",
            CallState::Held => "held",
            CallState::Ended => "ended",
        }
    }
}

/// 呼叫邀请事件内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInviteEvent {
    pub call_id: String,
    pub version: i32,
    pub offer: Option<CallOffer>,
    pub invitee: Option<String>,
    pub lifetime: Option<i64>,
}

/// 呼叫提议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallOffer {
    #[serde(rename = "type")]
    pub offer_type: String,
    pub sdp: String,
}

/// 呼叫候选人事件内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallCandidatesEvent {
    pub call_id: String,
    pub version: i32,
    pub candidates: Vec<IceCandidate>,
}

/// ICE 候选人
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(rename = "sdpMid")]
    pub sdp_mid: Option<String>,
    #[serde(rename = "sdpMLineIndex")]
    pub sdp_mline_index: Option<i32>,
    #[serde(rename = "type")]
    pub candidate_type: Option<String>,
}

/// 呼叫应答事件内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallAnswerEvent {
    pub call_id: String,
    pub version: i32,
    pub answer: CallAnswer,
}

/// 呼叫应答
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallAnswer {
    #[serde(rename = "type")]
    pub answer_type: String,
    pub sdp: String,
}

/// 呼叫挂断事件内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHangupEvent {
    pub call_id: String,
    pub version: i32,
}

pub struct CallService {
    storage: Arc<CallSessionStorage>,
}

impl CallService {
    pub fn new(storage: Arc<CallSessionStorage>) -> Self {
        Self { storage }
    }

    /// 处理呼叫邀请
    pub async fn handle_invite(
        &self,
        room_id: &str,
        sender_id: &str,
        content: CallInviteEvent,
    ) -> Result<CallSession, ApiError> {
        // 检查是否已存在会话
        if let Some(existing) = self
            .storage
            .get_session(&content.call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to check call session: {}", e)))?
        {
            if existing.state != "ended" {
                return Err(ApiError::conflict("Call session already exists"));
            }
        }

        // 创建新会话
        let params = CreateCallSessionParams {
            call_id: content.call_id.clone(),
            room_id: room_id.to_string(),
            caller_id: sender_id.to_string(),
            callee_id: content.invitee.clone(),
            offer_sdp: content.offer.as_ref().map(|o| o.sdp.clone()),
            lifetime: content.lifetime,
        };

        let session = self
            .storage
            .create_session(params)
            .await
            .map_err(|e| ApiError::database(format!("Failed to create call session: {}", e)))?;

        Ok(session)
    }

    /// 处理 ICE 候选人
    pub async fn handle_candidates(
        &self,
        room_id: &str,
        sender_id: &str,
        content: CallCandidatesEvent,
    ) -> Result<(), ApiError> {
        // 验证会话存在
        let session = self
            .storage
            .get_session(&content.call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get call session: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Call session not found"))?;

        // 验证发送者是呼叫的参与者
        if session.caller_id != sender_id && session.callee_id.as_deref() != Some(sender_id) {
            return Err(ApiError::forbidden(
                "Not authorized to send candidates for this call",
            ));
        }

        // 添加所有候选人
        for candidate in content.candidates {
            self.storage
                .add_candidate(
                    &content.call_id,
                    room_id,
                    sender_id,
                    serde_json::to_value(candidate).map_err(|e| {
                        ApiError::internal(format!("Failed to serialize candidate: {}", e))
                    })?,
                )
                .await
                .map_err(|e| ApiError::database(format!("Failed to add candidate: {}", e)))?;
        }

        Ok(())
    }

    /// 处理呼叫应答
    pub async fn handle_answer(
        &self,
        room_id: &str,
        sender_id: &str,
        content: CallAnswerEvent,
    ) -> Result<CallSession, ApiError> {
        // 验证会话存在
        let session = self
            .storage
            .get_session(&content.call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get call session: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Call session not found"))?;

        // 验证发送者是被邀请方
        if session.callee_id.as_deref() != Some(sender_id) {
            return Err(ApiError::forbidden("Not authorized to answer this call"));
        }

        // 更新会话状态
        self.storage
            .set_answer(&content.call_id, room_id, &content.answer.sdp)
            .await
            .map_err(|e| ApiError::database(format!("Failed to set answer: {}", e)))?;

        // 返回更新后的会话
        self.storage
            .get_session(&content.call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get updated session: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Call session not found after answer"))
    }

    /// 处理呼叫挂断
    pub async fn handle_hangup(
        &self,
        room_id: &str,
        sender_id: &str,
        content: CallHangupEvent,
    ) -> Result<(), ApiError> {
        // 验证会话存在
        let session = self
            .storage
            .get_session(&content.call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get call session: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Call session not found"))?;

        // 验证发送者是呼叫的参与者
        if session.caller_id != sender_id && session.callee_id.as_deref() != Some(sender_id) {
            return Err(ApiError::forbidden("Not authorized to end this call"));
        }

        // 结束会话
        self.storage
            .end_session(&content.call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to end call session: {}", e)))?;

        Ok(())
    }

    /// 获取呼叫会话
    pub async fn get_session(
        &self,
        call_id: &str,
        room_id: &str,
    ) -> Result<Option<CallSession>, ApiError> {
        self.storage
            .get_session(call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get call session: {}", e)))
    }

    /// 获取会话的候选人
    pub async fn get_candidates(
        &self,
        call_id: &str,
        room_id: &str,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let candidates = self
            .storage
            .get_candidates(call_id, room_id)
            .await
            .map_err(|e| ApiError::database(format!("Failed to get candidates: {}", e)))?;

        Ok(candidates.into_iter().map(|c| c.candidate).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_state_as_str() {
        assert_eq!(CallState::RingING.as_str(), "ringing");
        assert_eq!(CallState::Connected.as_str(), "connected");
        assert_eq!(CallState::Held.as_str(), "held");
        assert_eq!(CallState::Ended.as_str(), "ended");
    }

    #[test]
    fn test_call_state_serialization() {
        let state = CallState::Connected;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"connected\"");

        let parsed: CallState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, CallState::Connected);
    }

    #[test]
    fn test_call_invite_event_deserialization() {
        let json = r#"{
            "call_id": "call123",
            "version": 1,
            "offer": {
                "type": "offer",
                "sdp": "v=0..."
            },
            "invitee": "@alice:example.com",
            "lifetime": 60000
        }"#;

        let event: CallInviteEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.call_id, "call123");
        assert_eq!(event.version, 1);
        assert!(event.offer.is_some());
        let offer = event.offer.unwrap();
        assert_eq!(offer.offer_type, "offer");
        assert_eq!(offer.sdp, "v=0...");
        assert_eq!(event.invitee, Some("@alice:example.com".to_string()));
        assert_eq!(event.lifetime, Some(60000));
    }

    #[test]
    fn test_call_candidates_event() {
        let event = CallCandidatesEvent {
            call_id: "call123".to_string(),
            version: 1,
            candidates: vec![IceCandidate {
                candidate: "candidate:1".to_string(),
                sdp_mid: Some("0".to_string()),
                sdp_mline_index: Some(0),
                candidate_type: Some("host".to_string()),
            }],
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"call_id\":\"call123\""));
        assert!(json.contains("\"sdpMid\":\"0\""));
        assert!(json.contains("\"sdpMLineIndex\":0"));
    }

    #[test]
    fn test_call_answer_event() {
        let event = CallAnswerEvent {
            call_id: "call123".to_string(),
            version: 1,
            answer: CallAnswer {
                answer_type: "answer".to_string(),
                sdp: "v=0...answer".to_string(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: CallAnswerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.call_id, "call123");
        assert_eq!(parsed.answer.answer_type, "answer");
        assert_eq!(parsed.answer.sdp, "v=0...answer");
    }

    #[test]
    fn test_call_hangup_event() {
        let event = CallHangupEvent {
            call_id: "call123".to_string(),
            version: 1,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"call_id":"call123","version":1}"#);
    }

    #[test]
    fn test_ice_candidate_serialization() {
        let candidate = IceCandidate {
            candidate: "candidate:1 1 UDP 2122260223 192.168.1.1 54321 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
            candidate_type: Some("host".to_string()),
        };

        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains("\"candidate\":\"candidate:1"));
        assert!(json.contains("\"sdpMid\":\"0\""));
        assert!(json.contains("\"sdpMLineIndex\":0"));
        assert!(json.contains("\"type\":\"host\""));
    }

    #[test]
    fn test_call_offer_serialization() {
        let offer = CallOffer {
            offer_type: "offer".to_string(),
            sdp: "v=0\r\no=- 123456 2 IN IP4 127.0.0.1".to_string(),
        };

        let json = serde_json::to_string(&offer).unwrap();
        assert!(json.contains("\"type\":\"offer\""));
        assert!(json.contains("\"sdp\":\"v=0"));
    }

    #[test]
    fn test_call_answer_serialization() {
        let answer = CallAnswer {
            answer_type: "answer".to_string(),
            sdp: "v=0\r\no=- 654321 2 IN IP4 127.0.0.1".to_string(),
        };

        let json = serde_json::to_string(&answer).unwrap();
        assert!(json.contains("\"type\":\"answer\""));
        assert!(json.contains("\"sdp\":\"v=0"));
    }
}
