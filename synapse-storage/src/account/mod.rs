//! Account storage domain group.
//!
//! Re-exports account-related storage modules (`account_data`, `qr_login`,
//! `rendezvous`) under a single namespace so that new account storage modules
//! can be added here without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::account::AccountDataStorage` over
//! the flat `synapse_storage::AccountDataStorage`.

pub use crate::account_data::{AccountDataRecord, AccountDataStorage, AccountDataStoreApi};
pub use crate::qr_login::{QrLoginStorage, QrLoginStoreApi, QrTransaction};
pub use crate::rendezvous::{
    CreateRendezvousSessionParams, RendezvousCode, RendezvousIntent, RendezvousLoginFinish, RendezvousLoginStart,
    RendezvousLoginUser, RendezvousMessage, RendezvousMessageStorage, RendezvousMessageStoreApi, RendezvousSession,
    RendezvousStorage, RendezvousStoreApi, RendezvousTransport, StoredRendezvousMessage,
};
