//! RTC storage domain group.
//!
//! Re-exports RTC-related storage modules (`call_session`, `matrixrtc`) under a
//! single namespace. Feature-gated behind `voip-tracking`.
//!
//! Consumers should prefer `synapse_storage::rtc::CallSessionStorage` over the
//! flat `synapse_storage::CallSessionStorage`.

pub use crate::call_session::{CallCandidate, CallSession, CallSessionStorage, CreateCallSessionParams};
pub use crate::matrixrtc::{
    CreateMembershipParams, CreateSessionParams, MatrixRTCStorage, RTCEncryptionKey, RTCMembership, RTCSession,
    SessionWithMemberships,
};
