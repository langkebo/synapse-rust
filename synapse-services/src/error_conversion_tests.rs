//! One-way domain-error → `ApiError` conversion tests (A9 / B3-5).
//!
//! Each `impl From<XError> for ApiError` in this crate is pinned here with a
//! golden assertion on `(kind, code, message)`. When a new domain error gains
//! a `From` conversion, add a matching case so the HTTP mapping cannot drift.

#[cfg(test)]
mod tests {
    use crate::common::error::{ApiError, ApiErrorKind, MatrixErrorCode};
    use crate::room::state::tags::TagsError;

    /// Helper: pull the public golden surface out of an ApiError without a live
    /// HTTP renderer. PartialEq compares kind + code + message (cause is
    /// intentionally skipped), so this is stable and does not depend on axum.
    fn golden(err: &ApiError) -> (&ApiErrorKind, &MatrixErrorCode, &str) {
        (&err.kind, &err.code, err.message.as_str())
    }

    #[test]
    fn tags_error_not_found_maps_to_404_not_found() {
        let api: ApiError = TagsError::NotFound.into();
        let (kind, code, message) = golden(&api);
        assert_eq!(*kind, ApiErrorKind::NotFound);
        assert_eq!(*code, MatrixErrorCode::NotFound);
        assert!(message.contains("Tag not found"), "domain Display text must survive the conversion, got: {message}");
    }

    #[test]
    fn tags_error_duplicate_maps_to_409_user_in_use() {
        let api: ApiError = TagsError::Duplicate.into();
        let (kind, code, message) = golden(&api);
        assert_eq!(*kind, ApiErrorKind::Conflict);
        assert_eq!(*code, MatrixErrorCode::UserInUse);
        assert!(
            message.contains("Tag already exists"),
            "domain Display text must survive the conversion, got: {message}"
        );
    }
}
