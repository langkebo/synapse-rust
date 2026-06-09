use serde::Deserialize;

pub const DEFAULT_PAGE_LIMIT: i64 = 20;
pub const MAX_PAGE_LIMIT: i64 = 100;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pagination {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: Option<i64>,
    #[serde(default)]
    pub dir: Option<String>,
}

#[allow(clippy::unnecessary_wraps)]
fn default_limit() -> Option<i64> {
    Some(DEFAULT_PAGE_LIMIT)
}

impl Pagination {
    pub fn new(limit: Option<i64>) -> Self {
        Self { limit, ..Default::default() }
    }

    pub fn with_direction(mut self, dir: String) -> Self {
        self.dir = Some(dir);
        self
    }

    pub fn is_forward(&self) -> bool {
        self.dir.as_deref() != Some("b")
    }

    pub fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::{Pagination, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

    #[test]
    fn pagination_defaults_to_forward_direction() {
        let pagination = Pagination::default();

        assert!(pagination.is_forward());
        assert_eq!(pagination.effective_limit(), DEFAULT_PAGE_LIMIT);
    }

    #[test]
    fn pagination_treats_backward_direction_as_non_forward() {
        let pagination = Pagination::new(Some(25)).with_direction("b".to_string());

        assert!(!pagination.is_forward());
        assert_eq!(pagination.effective_limit(), 25);
    }

    #[test]
    fn pagination_clamps_limit_into_supported_range() {
        assert_eq!(Pagination::new(Some(0)).effective_limit(), 1);
        assert_eq!(Pagination::new(Some(MAX_PAGE_LIMIT + 50)).effective_limit(), MAX_PAGE_LIMIT);
    }

    #[test]
    fn pagination_deserializes_default_limit_when_missing() {
        let pagination: Pagination = serde_json::from_value(serde_json::json!({
            "from": "s1",
            "to": "s2"
        }))
        .unwrap();

        assert_eq!(pagination.from.as_deref(), Some("s1"));
        assert_eq!(pagination.to.as_deref(), Some("s2"));
        assert_eq!(pagination.limit, Some(DEFAULT_PAGE_LIMIT));
        assert!(pagination.is_forward());
    }
}
