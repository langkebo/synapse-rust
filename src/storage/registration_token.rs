pub use synapse_storage::registration_token::{
    decode_registration_token_cursor, encode_registration_token_cursor, CreateRegistrationTokenRequest,
    CreateRoomInviteRequest, RegistrationToken, RegistrationTokenBatch, RegistrationTokenCursor,
    RegistrationTokenStorage, RegistrationTokenUsage, RoomInvite, TokenValidationResult,
    UpdateRegistrationTokenRequest,
};

// NOTE: Tests moved to synapse-storage crate.
