use serde::Deserialize;

// ============================================================================
// SECTION: Search Configuration
// ============================================================================

/// 搜索服务配置。
#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    /// Elasticsearch 服务器 URL
    pub elasticsearch_url: String,
    /// 是否启用搜索功能
    pub enabled: bool,
    /// 搜索服务类型: "elasticsearch" | "postgres"
    #[serde(default = "default_search_provider")]
    pub provider: String,
    /// PostgreSQL 全文搜索配置
    #[serde(default)]
    pub postgres_fts: PostgresFtsConfig,
}

fn default_search_provider() -> String {
    "postgres".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PostgresFtsConfig {
    /// 是否启用 PostgreSQL 全文搜索
    pub enabled: bool,
    /// 搜索权重配置
    pub weights: PostgresFtsWeights,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PostgresFtsWeights {
    /// 标题权重
    pub title: f32,
    /// 内容权重
    pub body: f32,
    /// 发送者权重
    pub sender: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            elasticsearch_url: String::new(),
            enabled: false,
            provider: default_search_provider(),
            postgres_fts: PostgresFtsConfig::default(),
        }
    }
}