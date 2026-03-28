//! 分页参数提取器

use serde::Deserialize;

/// 分页参数
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pagination {
    /// 开始位置 (from token)
    #[serde(default)]
    pub from: Option<String>,

    /// 结束位置 (to token)
    #[serde(default)]
    pub to: Option<String>,

    /// 每页数量
    #[serde(default = "default_limit")]
    pub limit: Option<i64>,

    /// 方向: "f" (forward) 或 "b" (backward)
    #[serde(default)]
    pub dir: Option<String>,
}

fn default_limit() -> Option<i64> {
    Some(20)
}

impl Pagination {
    /// 创建新的分页参数
    pub fn new(limit: Option<i64>) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    /// 设置方向
    pub fn with_direction(mut self, dir: String) -> Self {
        self.dir = Some(dir);
        self
    }

    /// 是否向前翻页
    pub fn is_forward(&self) -> bool {
        self.dir.as_deref() != Some("b")
    }

    /// 获取有效 limit
    pub fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }
}
