use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoIpResult {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub isp: Option<String>,
    pub org: Option<String>,
    pub is_datacenter: bool,
    pub is_vpn: bool,
    pub is_proxy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoIpConfig {
    pub enabled: bool,
    pub provider: GeoIpProvider,
    pub maxmind_database_path: Option<String>,
    pub api_key: Option<String>,
    pub allow_datacenters: bool,
    pub allow_vpn: bool,
    pub allow_proxy: bool,
    pub default_country: Option<String>,
    pub blocked_countries: Vec<String>,
    pub allowed_countries: Vec<String>,
}

impl Default for GeoIpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: GeoIpProvider::Disabled,
            maxmind_database_path: None,
            api_key: None,
            allow_datacenters: false,
            allow_vpn: false,
            allow_proxy: false,
            default_country: None,
            blocked_countries: vec![],
            allowed_countries: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GeoIpProvider {
    MaxMind,
    IpApi,
    IpStack,
    #[default]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryAccessRule {
    pub country: String,
    pub allow: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAccessRule {
    pub id: i64,
    pub ip_pattern: String,
    pub allow: bool,
    pub reason: Option<String>,
    pub priority: i32,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginLocation {
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub timestamp: i64,
    pub geo_info: Option<GeoIpResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedLoginAttempt {
    pub user_id: String,
    pub ip_address: String,
    pub timestamp: i64,
    pub reason: String,
    pub country: Option<String>,
}
