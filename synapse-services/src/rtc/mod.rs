//! RTC (Real-Time Communication) 统一域
//!
//! 本模块将所有实时通信相关服务统一组织：
//!
//! - [`RtcInfraService`]: TURN/STUN 基础设施与凭证签发（原 VoipService）
//! - [`CallOrchestrationService`]: 1:1 通话信令 — invite/answer/candidates/hangup（原 CallService）
//! - [`RtcSessionService`]: MatrixRTC 会话/成员/加密密钥管理（原 MatrixRTCService）
//! - [`LivekitClient`]: SFU 集成客户端（原 LivekitClient）
//!
//! # 统一门面
//!
//! [`RtcDomainService`] 提供统一的 RTC 域入口，组合以上子服务，
//! 路由层和 ServiceContainer 应通过此门面访问 RTC 能力。
//!
//! # Feature Gates
//!
//! - 无 feature gate（L0 核心）: `RtcInfraService`
//! - `voip-tracking`: `CallOrchestrationService`, `RtcSessionService`, `LivekitClient`
//!
//! # VoiceService 不在此域
//!
//! 语音消息（VoiceService）属于异步媒体通信，不属于实时通信域，
//! 保留在 `services/voice_service.rs`。

pub mod infra;
pub mod metrics;

#[cfg(feature = "voip-tracking")]
pub mod call;
#[cfg(feature = "voip-tracking")]
pub mod session;
#[cfg(feature = "voip-tracking")]
pub mod sfu;

// Re-export new names
pub use infra::RtcInfraService;
pub use infra::RtcInfraSettings;
pub use infra::TurnCredentials;
pub use infra::VoipSettings;

#[cfg(feature = "voip-tracking")]
pub use synapse_common::config::LivekitConfig;
#[cfg(feature = "voip-tracking")]
pub use call::CallOrchestrationService;
#[cfg(feature = "voip-tracking")]
pub use call::{
    CallAnswer, CallAnswerEvent, CallCandidatesEvent, CallHangupEvent, CallInviteEvent, CallOffer, CallState,
    IceCandidate,
};
#[cfg(feature = "voip-tracking")]
pub use session::to_matrix_event;
#[cfg(feature = "voip-tracking")]
pub use session::RtcSessionService;
#[cfg(feature = "voip-tracking")]
pub use sfu::LivekitClient;
#[cfg(feature = "voip-tracking")]
pub use sfu::{
    CreateRoomRequest, CreateRoomResponse, JoinRoomRequest, JoinRoomResponse, LivekitCodec, LivekitError,
    LivekitParticipant, LivekitRoom, LivekitTrack, RoomParticipant, TrackInfo,
};

// Backward-compatible re-exports (old names → new types)
pub use infra::RtcInfraService as VoipService;

#[cfg(feature = "voip-tracking")]
pub use call::CallOrchestrationService as CallService;
#[cfg(feature = "voip-tracking")]
pub use session::RtcSessionService as MatrixRTCService;

use std::sync::Arc;

/// RTC 统一域门面
///
/// 组合所有 RTC 子服务，提供统一入口点。
/// ServiceContainer 和路由层应通过此类型访问 RTC 能力。
#[derive(Clone)]
pub struct RtcDomainService {
    pub infra: Arc<RtcInfraService>,
    #[cfg(feature = "voip-tracking")]
    pub call: Arc<CallOrchestrationService>,
    #[cfg(feature = "voip-tracking")]
    pub session: Arc<RtcSessionService>,
    #[cfg(feature = "voip-tracking")]
    pub sfu: Arc<LivekitClient>,
}

impl RtcDomainService {
    pub fn new(
        infra: Arc<RtcInfraService>,
        #[cfg(feature = "voip-tracking")] call: Arc<CallOrchestrationService>,
        #[cfg(feature = "voip-tracking")] session: Arc<RtcSessionService>,
        #[cfg(feature = "voip-tracking")] sfu: Arc<LivekitClient>,
    ) -> Self {
        Self {
            infra,
            #[cfg(feature = "voip-tracking")]
            call,
            #[cfg(feature = "voip-tracking")]
            session,
            #[cfg(feature = "voip-tracking")]
            sfu,
        }
    }

    pub fn infra(&self) -> &RtcInfraService {
        &self.infra
    }

    #[cfg(feature = "voip-tracking")]
    pub fn call(&self) -> &CallOrchestrationService {
        &self.call
    }

    #[cfg(feature = "voip-tracking")]
    pub fn session(&self) -> &RtcSessionService {
        &self.session
    }

    #[cfg(feature = "voip-tracking")]
    pub fn sfu(&self) -> &LivekitClient {
        &self.sfu
    }
}
