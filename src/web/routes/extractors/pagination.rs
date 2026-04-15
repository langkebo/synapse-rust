use serde::{Deserialize, Serialize};

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

fn default_limit() -> Option<i64> {
    Some(20)
}

impl Pagination {
    pub fn new(limit: Option<i64>) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    pub fn with_direction(mut self, dir: String) -> Self {
        self.dir = Some(dir);
        self
    }

    pub fn is_forward(&self) -> bool {
        self.dir.as_deref() != Some("b")
    }

    pub fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OffsetPagination {
    #[serde(default = "default_offset_limit")]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

fn default_offset_limit() -> Option<i64> {
    Some(20)
}

impl OffsetPagination {
    pub fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }

    pub fn effective_offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}
