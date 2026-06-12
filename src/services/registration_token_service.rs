pub use synapse_services::registration_token_service::*;
pub use crate::storage::registration_token::decode_registration_token_cursor;

#[cfg(test)]
mod tests {
    use super::{decode_registration_token_cursor, RegistrationTokenService};
    use crate::storage::registration_token::{RegistrationTokenCursor, RegistrationTokenStorage};
    use std::sync::Arc;

    #[test]
    fn root_registration_token_service_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<RegistrationTokenStorage>) -> RegistrationTokenService = RegistrationTokenService::new;
    }

    #[test]
    fn root_registration_token_service_keeps_cursor_helper_path() {
        let _helper: fn(Option<&str>) -> Option<RegistrationTokenCursor> = decode_registration_token_cursor;
    }
}
