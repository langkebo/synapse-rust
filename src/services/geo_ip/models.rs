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
    pub created_ts: i64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_ip_result_default() {
        let result = GeoIpResult::default();
        assert!(result.country.is_none());
        assert!(result.region.is_none());
        assert!(result.city.is_none());
        assert!(!result.is_datacenter);
        assert!(!result.is_vpn);
        assert!(!result.is_proxy);
    }

    #[test]
    fn test_geo_ip_result_with_data() {
        let result = GeoIpResult {
            country: Some("CN".to_string()),
            region: Some("Beijing".to_string()),
            city: Some("Beijing".to_string()),
            latitude: Some(39.9042),
            longitude: Some(116.4074),
            isp: Some("China Telecom".to_string()),
            org: Some("China Telecom".to_string()),
            is_datacenter: false,
            is_vpn: false,
            is_proxy: false,
        };
        assert_eq!(result.country.as_deref(), Some("CN"));
        assert_eq!(result.city.as_deref(), Some("Beijing"));
        assert!((result.latitude.unwrap() - 39.9042).abs() < 0.001);
    }

    #[test]
    fn test_geo_ip_config_default() {
        let config = GeoIpConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.provider, GeoIpProvider::Disabled);
        assert!(config.maxmind_database_path.is_none());
        assert!(config.api_key.is_none());
        assert!(!config.allow_datacenters);
        assert!(!config.allow_vpn);
        assert!(!config.allow_proxy);
        assert!(config.default_country.is_none());
        assert!(config.blocked_countries.is_empty());
        assert!(config.allowed_countries.is_empty());
    }

    #[test]
    fn test_geo_ip_config_with_blocked_countries() {
        let config = GeoIpConfig {
            enabled: true,
            blocked_countries: vec!["CN".to_string(), "RU".to_string()],
            allowed_countries: vec!["US".to_string()],
            ..Default::default()
        };
        assert!(config.enabled);
        assert_eq!(config.blocked_countries.len(), 2);
        assert_eq!(config.allowed_countries.len(), 1);
    }

    #[test]
    fn test_geo_ip_provider_default() {
        assert_eq!(GeoIpProvider::default(), GeoIpProvider::Disabled);
    }

    #[test]
    fn test_geo_ip_provider_serialization() {
        let disabled = GeoIpProvider::Disabled;
        let json = serde_json::to_string(&disabled).unwrap();
        assert_eq!(json, r#""disabled""#);

        let maxmind = GeoIpProvider::MaxMind;
        let json = serde_json::to_string(&maxmind).unwrap();
        assert_eq!(json, r#""max_mind""#);
    }

    #[test]
    fn test_geo_ip_provider_deserialization() {
        let provider: GeoIpProvider = serde_json::from_str(r#""max_mind""#).unwrap();
        assert_eq!(provider, GeoIpProvider::MaxMind);

        let provider: GeoIpProvider = serde_json::from_str(r#""ip_api""#).unwrap();
        assert_eq!(provider, GeoIpProvider::IpApi);

        let provider: GeoIpProvider = serde_json::from_str(r#""ip_stack""#).unwrap();
        assert_eq!(provider, GeoIpProvider::IpStack);

        let provider: GeoIpProvider = serde_json::from_str(r#""disabled""#).unwrap();
        assert_eq!(provider, GeoIpProvider::Disabled);
    }

    #[test]
    fn test_country_access_rule() {
        let rule = CountryAccessRule {
            country: "CN".to_string(),
            allow: false,
            reason: Some("Blocked for compliance".to_string()),
        };
        assert_eq!(rule.country, "CN");
        assert!(!rule.allow);
        assert_eq!(rule.reason.as_deref(), Some("Blocked for compliance"));
    }

    #[test]
    fn test_ip_access_rule() {
        let rule = IpAccessRule {
            id: 1,
            ip_pattern: "192.168.1.0/24".to_string(),
            allow: false,
            reason: Some("Internal network".to_string()),
            priority: 100,
            created_ts: 1700000000000,
        };
        assert_eq!(rule.id, 1);
        assert_eq!(rule.ip_pattern, "192.168.1.0/24");
        assert!(!rule.allow);
        assert_eq!(rule.priority, 100);
    }

    #[test]
    fn test_login_location() {
        let location = LoginLocation {
            user_id: "@user:example.com".to_string(),
            ip_address: "1.2.3.4".to_string(),
            user_agent: Some("Mozilla/5.0".to_string()),
            timestamp: 1700000000000,
            geo_info: Some(GeoIpResult {
                country: Some("US".to_string()),
                ..Default::default()
            }),
        };
        assert_eq!(location.user_id, "@user:example.com");
        assert_eq!(location.ip_address, "1.2.3.4");
        assert!(location.geo_info.is_some());
    }

    #[test]
    fn test_failed_login_attempt() {
        let attempt = FailedLoginAttempt {
            user_id: "@user:example.com".to_string(),
            ip_address: "5.6.7.8".to_string(),
            timestamp: 1700000000000,
            reason: "Invalid password".to_string(),
            country: Some("CN".to_string()),
        };
        assert_eq!(attempt.reason, "Invalid password");
        assert_eq!(attempt.country.as_deref(), Some("CN"));
    }
}
