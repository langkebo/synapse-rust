use super::models::*;
use crate::error::ApiError;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct GeoIpService {
    config: GeoIpConfig,
    http_client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, (GeoIpResult, Instant)>>>,
    rules: Arc<RwLock<Vec<IpAccessRule>>>,
    cache_ttl: Duration,
}

impl GeoIpService {
    pub fn new(config: GeoIpConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            rules: Arc::new(RwLock::new(Vec::new())),
            cache_ttl: Duration::from_secs(3600),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.provider != GeoIpProvider::Disabled
    }

    pub async fn lookup(&self, ip: &str) -> Result<GeoIpResult, ApiError> {
        if !self.is_enabled() {
            return Ok(GeoIpResult {
                country: self.config.default_country.clone(),
                region: None,
                city: None,
                latitude: None,
                longitude: None,
                isp: None,
                org: None,
                is_datacenter: false,
                is_vpn: false,
                is_proxy: false,
            });
        }

        {
            let cache = self.cache.read().await;
            if let Some((result, instant)) = cache.get(ip) {
                if instant.elapsed() < self.cache_ttl {
                    return Ok(result.clone());
                }
            }
        }

        let result = match self.config.provider {
            GeoIpProvider::MaxMind => self.lookup_maxmind(ip).await,
            GeoIpProvider::IpApi => self.lookup_ipapi(ip).await,
            GeoIpProvider::IpStack => self.lookup_ipstack(ip).await,
            GeoIpProvider::Disabled => Ok(GeoIpResult::default()),
        }?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(ip.to_string(), (result.clone(), Instant::now()));
        }

        Ok(result)
    }

    async fn lookup_maxmind(&self, _ip: &str) -> Result<GeoIpResult, ApiError> {
        Ok(GeoIpResult {
            country: self.config.default_country.clone(),
            region: None,
            city: None,
            latitude: None,
            longitude: None,
            isp: None,
            org: None,
            is_datacenter: false,
            is_vpn: false,
            is_proxy: false,
        })
    }

    async fn lookup_ipapi(&self, ip: &str) -> Result<GeoIpResult, ApiError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| ApiError::internal("IPAPI API key not configured".to_string()))?;

        let url = format!("http://api.ipapi.com/{}?access_key={}", ip, api_key);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("IPAPI request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "IPAPI returned error: {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse IPAPI response: {}", e)))?;

        Ok(GeoIpResult {
            country: json
                .get("country_code")
                .and_then(|v| v.as_str())
                .map(String::from),
            region: json
                .get("region_name")
                .and_then(|v| v.as_str())
                .map(String::from),
            city: json.get("city").and_then(|v| v.as_str()).map(String::from),
            latitude: json.get("latitude").and_then(|v| v.as_f64()),
            longitude: json.get("longitude").and_then(|v| v.as_f64()),
            isp: json
                .get("connection")
                .and_then(|v| v.as_str())
                .map(String::from),
            org: json.get("org").and_then(|v| v.as_str()).map(String::from),
            is_datacenter: false,
            is_vpn: false,
            is_proxy: false,
        })
    }

    async fn lookup_ipstack(&self, ip: &str) -> Result<GeoIpResult, ApiError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| ApiError::internal("IPStack API key not configured".to_string()))?;

        let url = format!("http://api.ipstack.com/{}?access_key={}", ip, api_key);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("IPStack request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "IPStack returned error: {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse IPStack response: {}", e)))?;

        Ok(GeoIpResult {
            country: json
                .get("country_code")
                .and_then(|v| v.as_str())
                .map(String::from),
            region: json
                .get("region_name")
                .and_then(|v| v.as_str())
                .map(String::from),
            city: json.get("city").and_then(|v| v.as_str()).map(String::from),
            latitude: json.get("latitude").and_then(|v| v.as_f64()),
            longitude: json.get("longitude").and_then(|v| v.as_f64()),
            isp: json.get("isp").and_then(|v| v.as_str()).map(String::from),
            org: json.get("org").and_then(|v| v.as_str()).map(String::from),
            is_datacenter: false,
            is_vpn: false,
            is_proxy: false,
        })
    }

    pub async fn check_access(&self, ip: &str) -> Result<bool, ApiError> {
        let _: IpAddr = ip
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid IP address".to_string()))?;

        let rules = self.rules.read().await;

        let mut matching_rules: Vec<&IpAccessRule> = rules
            .iter()
            .filter(|rule| self.matches_ip_pattern(ip, &rule.ip_pattern))
            .collect();

        matching_rules.sort_by_key(|r| r.priority);

        if let Some(rule) = matching_rules.first() {
            return Ok(rule.allow);
        }

        if !self.is_enabled() {
            return Ok(true);
        }

        let geo = self.lookup(ip).await?;

        if let Some(ref country) = geo.country {
            if !self.config.allowed_countries.is_empty() {
                if !self.config.allowed_countries.contains(country) {
                    return Ok(false);
                }
            }

            if self.config.blocked_countries.contains(country) {
                return Ok(false);
            }
        }

        if geo.is_datacenter && !self.config.allow_datacenters {
            return Ok(false);
        }

        if geo.is_vpn && !self.config.allow_vpn {
            return Ok(false);
        }

        if geo.is_proxy && !self.config.allow_proxy {
            return Ok(false);
        }

        Ok(true)
    }

    fn matches_ip_pattern(&self, ip: &str, pattern: &str) -> bool {
        if pattern.contains('/') {
            if let Some((network, prefix_len)) = pattern.split_once('/') {
                if let (Ok(ip1), Ok(ip2)) = (ip.parse::<IpAddr>(), network.parse::<IpAddr>()) {
                    return match (ip1, ip2) {
                        (IpAddr::V4(ip1), IpAddr::V4(ip2)) => {
                            let prefix: u8 = prefix_len.parse().unwrap_or(32);
                            let mask = !((1u32 << (32 - prefix)) - 1);
                            (u32::from(ip1) & mask) == (u32::from(ip2) & mask)
                        }
                        (IpAddr::V6(ip1), IpAddr::V6(ip2)) => {
                            let prefix: u8 = prefix_len.parse().unwrap_or(128);
                            let mask = !((1u128 << (128 - prefix)) - 1);
                            (u128::from(ip1) & mask) == (u128::from(ip2) & mask)
                        }
                        _ => false,
                    };
                }
            }
        }

        ip == pattern
    }

    pub async fn add_rule(&self, rule: IpAccessRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
    }

    pub async fn remove_rule(&self, rule_id: i64) {
        let mut rules = self.rules.write().await;
        rules.retain(|r| r.id != rule_id);
    }

    pub async fn get_rules(&self) -> Vec<IpAccessRule> {
        self.rules.read().await.clone()
    }
}
