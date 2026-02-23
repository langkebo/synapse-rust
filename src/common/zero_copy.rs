use std::borrow::Cow;

pub struct StaticString(Cow<'static, str>);

impl StaticString {
    pub const fn from_static(s: &'static str) -> Self {
        StaticString(Cow::Borrowed(s))
    }

    pub fn from_owned(s: String) -> Self {
        StaticString(Cow::Owned(s))
    }

    pub fn from_ref(s: &str) -> Self {
        if s.is_empty() {
            StaticString(Cow::Borrowed(""))
        } else {
            StaticString(Cow::Owned(s.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub fn into_string(self) -> String {
        self.0.into_owned()
    }

    pub fn is_borrowed(&self) -> bool {
        matches!(self.0, Cow::Borrowed(_))
    }

    pub fn is_owned(&self) -> bool {
        matches!(self.0, Cow::Owned(_))
    }
}

impl Clone for StaticString {
    fn clone(&self) -> Self {
        StaticString(self.0.clone())
    }
}

impl std::fmt::Display for StaticString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for StaticString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StaticString({:?}, borrowed={})", self.0, self.is_borrowed())
    }
}

impl AsRef<str> for StaticString {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<&'static str> for StaticString {
    fn from(s: &'static str) -> Self {
        StaticString::from_static(s)
    }
}

impl From<String> for StaticString {
    fn from(s: String) -> Self {
        StaticString::from_owned(s)
    }
}

impl From<&String> for StaticString {
    fn from(s: &String) -> Self {
        StaticString::from_ref(s.as_str())
    }
}

impl std::ops::Deref for StaticString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl PartialEq for StaticString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for StaticString {}

impl std::hash::Hash for StaticString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

pub struct ErrorMessage {
    message: StaticString,
    code: StaticString,
}

impl ErrorMessage {
    pub const fn static_msg(message: &'static str, code: &'static str) -> Self {
        Self {
            message: StaticString::from_static(message),
            code: StaticString::from_static(code),
        }
    }

    pub fn dynamic_msg(message: String, code: &'static str) -> Self {
        Self {
            message: StaticString::from_owned(message),
            code: StaticString::from_static(code),
        }
    }

    pub fn full_dynamic(message: String, code: String) -> Self {
        Self {
            message: StaticString::from_owned(message),
            code: StaticString::from_owned(code),
        }
    }

    pub fn message(&self) -> &str {
        self.message.as_str()
    }

    pub fn code(&self) -> &str {
        self.code.as_str()
    }
}

pub mod matrix_errors {
    use super::*;

    pub const M_NOT_FOUND: ErrorMessage = ErrorMessage::static_msg("Resource not found", "M_NOT_FOUND");
    pub const M_FORBIDDEN: ErrorMessage = ErrorMessage::static_msg("Access denied", "M_FORBIDDEN");
    pub const M_UNAUTHORIZED: ErrorMessage = ErrorMessage::static_msg("Unauthorized access", "M_UNAUTHORIZED");
    pub const M_UNKNOWN_TOKEN: ErrorMessage = ErrorMessage::static_msg("Unknown or expired token", "M_UNKNOWN_TOKEN");
    pub const M_MISSING_TOKEN: ErrorMessage = ErrorMessage::static_msg("Access token required", "M_MISSING_TOKEN");
    pub const M_BAD_JSON: ErrorMessage = ErrorMessage::static_msg("Invalid JSON data", "M_BAD_JSON");
    pub const M_NOT_JSON: ErrorMessage = ErrorMessage::static_msg("Request body is not valid JSON", "M_NOT_JSON");
    pub const M_INVALID_PARAM: ErrorMessage = ErrorMessage::static_msg("Invalid parameter value", "M_INVALID_PARAM");
    pub const M_USER_IN_USE: ErrorMessage = ErrorMessage::static_msg("Username already taken", "M_USER_IN_USE");
    pub const M_ROOM_IN_USE: ErrorMessage = ErrorMessage::static_msg("Room alias already taken", "M_ROOM_IN_USE");
    pub const M_LIMIT_EXCEEDED: ErrorMessage = ErrorMessage::static_msg("Rate limit exceeded", "M_LIMIT_EXCEEDED");
    pub const M_INTERNAL_ERROR: ErrorMessage = ErrorMessage::static_msg("Internal server error", "M_INTERNAL_ERROR");
    pub const M_USER_DEACTIVATED: ErrorMessage = ErrorMessage::static_msg("User account has been deactivated", "M_USER_DEACTIVATED");
    pub const M_GUEST_ACCESS_FORBIDDEN: ErrorMessage = ErrorMessage::static_msg("Guest access not allowed", "M_GUEST_ACCESS_FORBIDDEN");
    pub const M_CONSENT_NOT_GIVEN: ErrorMessage = ErrorMessage::static_msg("User consent required", "M_CONSENT_NOT_GIVEN");
    pub const M_THREEPID_IN_USE: ErrorMessage = ErrorMessage::static_msg("Third-party ID already in use", "M_THREEPID_IN_USE");
    pub const M_THREEPID_NOT_FOUND: ErrorMessage = ErrorMessage::static_msg("Third-party ID not found", "M_THREEPID_NOT_FOUND");
    pub const M_THREEPID_AUTH_FAILED: ErrorMessage = ErrorMessage::static_msg("Third-party authentication failed", "M_THREEPID_AUTH_FAILED");
    pub const M_INVALID_USERNAME: ErrorMessage = ErrorMessage::static_msg("Invalid username format", "M_INVALID_USERNAME");
    pub const M_INVALID_PASSWORD: ErrorMessage = ErrorMessage::static_msg("Invalid password format", "M_INVALID_PASSWORD");
    pub const M_PASSWORD_TOO_SHORT: ErrorMessage = ErrorMessage::static_msg("Password is too short", "M_PASSWORD_TOO_SHORT");
    pub const M_PASSWORD_NO_DIGIT: ErrorMessage = ErrorMessage::static_msg("Password must contain a digit", "M_PASSWORD_NO_DIGIT");
    pub const M_PASSWORD_NO_UPPERCASE: ErrorMessage = ErrorMessage::static_msg("Password must contain an uppercase letter", "M_PASSWORD_NO_UPPERCASE");
    pub const M_PASSWORD_NO_LOWERCASE: ErrorMessage = ErrorMessage::static_msg("Password must contain a lowercase letter", "M_PASSWORD_NO_LOWERCASE");
    pub const M_PASSWORD_NO_SYMBOL: ErrorMessage = ErrorMessage::static_msg("Password must contain a symbol", "M_PASSWORD_NO_SYMBOL");
    pub const M_WEAK_PASSWORD: ErrorMessage = ErrorMessage::static_msg("Password is too weak", "M_WEAK_PASSWORD");
    pub const M_INCOMPATIBLE_ROOM_VERSION: ErrorMessage = ErrorMessage::static_msg("Incompatible room version", "M_INCOMPATIBLE_ROOM_VERSION");
    pub const M_UNSUPPORTED_ROOM_VERSION: ErrorMessage = ErrorMessage::static_msg("Unsupported room version", "M_UNSUPPORTED_ROOM_VERSION");
    pub const M_EXCLUSIVE: ErrorMessage = ErrorMessage::static_msg("Exclusive resource error", "M_EXCLUSIVE");
    pub const M_NOT_A_USER: ErrorMessage = ErrorMessage::static_msg("Not a valid user", "M_NOT_A_USER");
    pub const M_DEVICE_NOT_FOUND: ErrorMessage = ErrorMessage::static_msg("Device not found", "M_DEVICE_NOT_FOUND");
    pub const M_TOO_LARGE: ErrorMessage = ErrorMessage::static_msg("Request too large", "M_TOO_LARGE");
    pub const M_CANNOT_LEAVE_SERVER_NOTICE_ROOM: ErrorMessage = ErrorMessage::static_msg("Cannot leave server notice room", "M_CANNOT_LEAVE_SERVER_NOTICE_ROOM");
}

pub struct CowStringBuilder {
    parts: Vec<Cow<'static, str>>,
}

impl CowStringBuilder {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            parts: Vec::with_capacity(capacity),
        }
    }

    pub fn push_static(&mut self, s: &'static str) -> &mut Self {
        if !s.is_empty() {
            self.parts.push(Cow::Borrowed(s));
        }
        self
    }

    pub fn push_owned(&mut self, s: String) -> &mut Self {
        if !s.is_empty() {
            self.parts.push(Cow::Owned(s));
        }
        self
    }

    pub fn push_str(&mut self, s: &str) -> &mut Self {
        if !s.is_empty() {
            self.parts.push(Cow::Owned(s.to_string()));
        }
        self
    }

    pub fn push_static_if(&mut self, condition: bool, s: &'static str) -> &mut Self {
        if condition {
            self.push_static(s);
        }
        self
    }

    pub fn build(self) -> Cow<'static, str> {
        if self.parts.is_empty() {
            return Cow::Borrowed("");
        }

        if self.parts.len() == 1 {
            return self.parts.into_iter().next().unwrap();
        }

        let total_len: usize = self.parts.iter().map(|p| p.len()).sum();
        let mut result = String::with_capacity(total_len);
        for part in self.parts {
            result.push_str(&part);
        }
        Cow::Owned(result)
    }

    pub fn build_string(self) -> String {
        self.build().into_owned()
    }
}

impl Default for CowStringBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Write for CowStringBuilder {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_string_from_static() {
        let s = StaticString::from_static("hello");
        assert!(s.is_borrowed());
        assert_eq!(s.as_str(), "hello");
    }

    #[test]
    fn test_static_string_from_owned() {
        let s = StaticString::from_owned("hello".to_string());
        assert!(s.is_owned());
        assert_eq!(s.as_str(), "hello");
    }

    #[test]
    fn test_static_string_clone() {
        let s1 = StaticString::from_static("hello");
        let s2 = s1.clone();
        assert_eq!(s1.as_str(), s2.as_str());
        assert!(s1.is_borrowed());
        assert!(s2.is_borrowed());
    }

    #[test]
    fn test_static_string_into_string() {
        let s = StaticString::from_static("hello");
        let owned = s.into_string();
        assert_eq!(owned, "hello");
    }

    #[test]
    fn test_error_message_static() {
        let err = ErrorMessage::static_msg("Not found", "M_NOT_FOUND");
        assert_eq!(err.message(), "Not found");
        assert_eq!(err.code(), "M_NOT_FOUND");
    }

    #[test]
    fn test_error_message_dynamic() {
        let err = ErrorMessage::dynamic_msg("User 123 not found".to_string(), "M_NOT_FOUND");
        assert_eq!(err.message(), "User 123 not found");
        assert_eq!(err.code(), "M_NOT_FOUND");
    }

    #[test]
    fn test_matrix_errors() {
        assert_eq!(matrix_errors::M_NOT_FOUND.code(), "M_NOT_FOUND");
        assert_eq!(matrix_errors::M_FORBIDDEN.code(), "M_FORBIDDEN");
        assert_eq!(matrix_errors::M_UNAUTHORIZED.code(), "M_UNAUTHORIZED");
    }

    #[test]
    fn test_cow_string_builder_empty() {
        let builder = CowStringBuilder::new();
        let result = builder.build();
        assert_eq!(result, "");
    }

    #[test]
    fn test_cow_string_builder_single_static() {
        let mut builder = CowStringBuilder::new();
        builder.push_static("hello");
        let result = builder.build();
        assert_eq!(result, "hello");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn test_cow_string_builder_multiple() {
        let mut builder = CowStringBuilder::new();
        builder.push_static("hello");
        builder.push_static(" ");
        builder.push_static("world");
        let result = builder.build();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_cow_string_builder_mixed() {
        let mut builder = CowStringBuilder::new();
        builder.push_static("User ");
        builder.push_owned("123".to_string());
        builder.push_static(" not found");
        let result = builder.build();
        assert_eq!(result, "User 123 not found");
    }

    #[test]
    fn test_cow_string_builder_with_capacity() {
        let mut builder = CowStringBuilder::with_capacity(5);
        builder.push_static("hello");
        builder.push_static(" world");
        let result = builder.build_string();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_static_string_deref() {
        let s = StaticString::from_static("hello");
        assert!(s.starts_with("hel"));
        assert!(s.ends_with("llo"));
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_static_string_equality() {
        let s1 = StaticString::from_static("hello");
        let s2 = StaticString::from_static("hello");
        let s3 = StaticString::from_owned("hello".to_string());
        assert_eq!(s1, s2);
        assert_eq!(s1, s3);
    }

    #[test]
    fn test_static_string_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(StaticString::from_static("hello"));
        assert!(set.contains(&StaticString::from_static("hello")));
        assert!(set.contains(&StaticString::from_owned("hello".to_string())));
    }
}
