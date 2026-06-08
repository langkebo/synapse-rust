/// Unified parser for Matrix `mxc://` URIs.
///
/// Matrix media is addressed via `mxc://server_name/media_id` URIs.
/// This struct provides a single place to parse and construct those URIs,
/// replacing the 3+ ad-hoc implementations that were scattered across
/// voice_service, media_service, and oidc routes.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLocator {
    pub server_name: String,
    pub media_id: String,
}

impl MediaLocator {
    /// Parse an `mxc://server_name/media_id` URI into its components.
    ///
    /// Returns `Err` if the URI does not start with `mxc://` or is missing
    /// either the server_name or media_id segment.
    pub fn parse(mxc_url: &str) -> Result<Self, String> {
        let rest = mxc_url
            .strip_prefix("mxc://")
            .ok_or_else(|| format!("Invalid mxc URI (missing mxc:// prefix): {mxc_url}"))?;

        let (server_name, media_id) = rest
            .split_once('/')
            .ok_or_else(|| format!("Invalid mxc URI (expected mxc://server_name/media_id): {mxc_url}"))?;

        if server_name.is_empty() {
            return Err(format!("Invalid mxc URI (empty server_name): {mxc_url}"));
        }
        if media_id.is_empty() {
            return Err(format!("Invalid mxc URI (empty media_id): {mxc_url}"));
        }

        Ok(Self {
            server_name: server_name.to_string(),
            media_id: media_id.to_string(),
        })
    }

    /// Reconstruct the `mxc://server_name/media_id` URI.
    pub fn to_mxc_url(&self) -> String {
        format!("mxc://{}/{}", self.server_name, self.media_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let loc = MediaLocator::parse("mxc://example.com/abc123").unwrap();
        assert_eq!(loc.server_name, "example.com");
        assert_eq!(loc.media_id, "abc123");
    }

    #[test]
    fn test_parse_roundtrip() {
        let loc = MediaLocator::parse("mxc://example.com/abc123").unwrap();
        assert_eq!(loc.to_mxc_url(), "mxc://example.com/abc123");
    }

    #[test]
    fn test_parse_missing_prefix() {
        assert!(MediaLocator::parse("https://example.com/abc").is_err());
    }

    #[test]
    fn test_parse_no_slash() {
        assert!(MediaLocator::parse("mxc://example.com").is_err());
    }

    #[test]
    fn test_parse_empty_server_name() {
        assert!(MediaLocator::parse("mxc:///abc").is_err());
    }

    #[test]
    fn test_parse_empty_media_id() {
        assert!(MediaLocator::parse("mxc://example.com/").is_err());
    }
}
