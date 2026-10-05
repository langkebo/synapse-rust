//! Configuration loading and environment variable resolution.
//!
//! Handles loading `Config` from file + environment, and resolving
//! `${ENV_VAR}` placeholders in config values.

use config::Config as ConfigBuilder;
use regex::Regex;
use std::path::PathBuf;

use super::Config;

impl Config {
    /// Load configuration from file (`SYNAPSE_CONFIG_PATH`) and environment overrides.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = std::env::var("SYNAPSE_CONFIG_PATH").unwrap_or_else(|_| "./homeserver.yaml".to_string());

        tracing::info!("Loading configuration from: {}", config_path);

        let config = ConfigBuilder::builder()
            .add_source(config::File::with_name(&config_path))
            .add_source(config::Environment::with_prefix("SYNAPSE").separator("__"))
            .build()?;

        let mut config_values: Self = config.try_deserialize()?;

        tracing::info!("Configuration loaded, resolving environment variables...");
        tracing::debug!(
            "Before resolution - federation.signing_key: [REDACTED] ({} chars)",
            config_values.federation.signing_key.as_ref().map_or(0, |k| k.len())
        );
        tracing::debug!(
            "Before resolution - security.secret: [REDACTED] ({} chars)",
            config_values.security.secret.len()
        );

        config_values.resolve_env_variables().map_err(|e| format!("Failed to resolve environment variables: {e}"))?;

        config_values.validate().map_err(|e| format!("Configuration validation failed: {e}"))?;

        tracing::info!("Environment variables resolved successfully");
        tracing::debug!(
            "After resolution - federation.signing_key: [REDACTED] ({} chars)",
            config_values.federation.signing_key.as_ref().map_or(0, |k| k.len())
        );
        tracing::debug!(
            "After resolution - security.secret: [REDACTED] ({} chars)",
            config_values.security.secret.len()
        );

        Ok(config_values)
    }

    /// Resolve `${ENV_VAR}`, `${ENV_VAR:-default}`, `${ENV_VAR:=assign}`,
    /// and `${ENV_VAR:?error}` placeholders in all config fields.
    pub fn resolve_env_variables(&mut self) -> Result<(), String> {
        self.server.name = resolve_env_in_string(&self.server.name)?;
        self.server.host = resolve_env_in_string(&self.server.host)?;
        self.server.public_baseurl =
            self.server.public_baseurl.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.signing_key_path =
            self.server.signing_key_path.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.macaroon_secret_key =
            self.server.macaroon_secret_key.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.form_secret = self.server.form_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.server_name = self.server.server_name.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.registration_shared_secret =
            self.server.registration_shared_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.admin_contact = self.server.admin_contact.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.user_agent_suffix =
            self.server.user_agent_suffix.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.web_client_location =
            self.server.web_client_location.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.server.map_style_url = self.server.map_style_url.take().map(|v| resolve_env_in_string(&v)).transpose()?;

        self.database.host = resolve_env_in_string(&self.database.host)?;
        self.database.username = resolve_env_in_string(&self.database.username)?;
        self.database.password = resolve_env_in_string(&self.database.password)?;
        self.database.name = resolve_env_in_string(&self.database.name)?;

        self.redis.host = resolve_env_in_string(&self.redis.host)?;
        // `username` 与 `password` 走同一条插值路径：两者的取值约束是绑定的
        // （username 必须配 password，见 `Config::validate`），只解析其一会让
        // 运维只能把用户名写死在配置文件里。
        self.redis.username = self.redis.username.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.redis.password = self.redis.password.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.redis.key_prefix = resolve_env_in_string(&self.redis.key_prefix)?;

        self.logging.level = resolve_env_in_string(&self.logging.level)?;
        self.logging.format = resolve_env_in_string(&self.logging.format)?;
        self.logging.log_file = self.logging.log_file.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.logging.log_dir = self.logging.log_dir.take().map(|v| resolve_env_in_string(&v)).transpose()?;

        self.federation.server_name = resolve_env_in_string(&self.federation.server_name)?;
        self.federation.signing_key =
            self.federation.signing_key.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.federation.key_id = self.federation.key_id.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        self.federation.signing_key_master_key =
            self.federation.signing_key_master_key.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        if self.federation.signing_key_master_key.as_deref().is_some_and(|key| key.trim().is_empty()) {
            // `${FEDERATION_MASTER_KEY:-}` with the variable unset resolves to the
            // **empty string**, not to "absent". Left as `Some("")` it takes the
            // *encrypt* branch in `KeyRotationManager::resolve_stored_secret_key`
            // with an empty HKDF input — i.e. the federation signing key is written
            // with an `enc:` prefix under a key that contains no secret at all,
            // while the fail-closed "no master key configured" path is skipped.
            // Normalise at this single boundary where config text becomes typed
            // config, so the rest of the code only ever sees `Some(real key)`/`None`.
            tracing::info!(
                "federation.signing_key_master_key resolved to an empty value — treating it as not configured"
            );
            self.federation.signing_key_master_key = None;
        }
        self.federation.ca_file = self
            .federation
            .ca_file
            .take()
            .map(|v| resolve_env_in_string(&v.to_string_lossy()).map(PathBuf::from))
            .transpose()?;
        self.federation.client_ca_file = self
            .federation
            .client_ca_file
            .take()
            .map(|v| resolve_env_in_string(&v.to_string_lossy()).map(PathBuf::from))
            .transpose()?;

        for server in &mut self.federation.trusted_key_servers {
            server.server_name = resolve_env_in_string(&server.server_name)?;
        }

        self.security.secret = resolve_env_in_string(&self.security.secret)?;
        self.security.admin_mfa_shared_secret = resolve_env_in_string(&self.security.admin_mfa_shared_secret)?;

        self.search.elasticsearch_url = resolve_env_in_string(&self.search.elasticsearch_url)?;

        if self.smtp.enabled {
            self.smtp.host = resolve_env_in_string(&self.smtp.host)?;
            self.smtp.username = resolve_env_in_string(&self.smtp.username)?;
            self.smtp.password = resolve_env_in_string(&self.smtp.password)?;
            self.smtp.from = resolve_env_in_string(&self.smtp.from)?;
        }

        if self.oidc.enabled {
            self.oidc.issuer = resolve_env_in_string(&self.oidc.issuer)?;
            self.oidc.client_id = resolve_env_in_string(&self.oidc.client_id)?;
            self.oidc.client_secret = self.oidc.client_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        }

        if self.saml.enabled {
            self.saml.metadata_url = self.saml.metadata_url.take().map(|v| resolve_env_in_string(&v)).transpose()?;
            self.saml.sp_entity_id = resolve_env_in_string(&self.saml.sp_entity_id)?;
        }

        self.admin_registration.shared_secret = resolve_env_in_string(&self.admin_registration.shared_secret)?;
        self.admin_registration.ip_whitelist = self
            .admin_registration
            .ip_whitelist
            .iter()
            .map(|value| resolve_env_in_string(value))
            .collect::<Result<Vec<_>, _>>()?;
        self.admin_registration.approval_tokens = self
            .admin_registration
            .approval_tokens
            .iter()
            .map(|value| resolve_env_in_string(value))
            .collect::<Result<Vec<_>, _>>()?;

        if self.voip.is_enabled() {
            self.voip.turn_shared_secret =
                self.voip.turn_shared_secret.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        }

        if self.push.is_enabled() {
            self.push.push_gateway_url =
                self.push.push_gateway_url.take().map(|v| resolve_env_in_string(&v)).transpose()?;
        }

        self.worker.instance_name = resolve_env_in_string(&self.worker.instance_name)?;
        for instance in self.worker.instance_map.values_mut() {
            instance.host = resolve_env_in_string(&instance.host)?;
        }
        self.worker.replication.server_name = resolve_env_in_string(&self.worker.replication.server_name)?;
        self.worker.replication.http.host = resolve_env_in_string(&self.worker.replication.http.host)?;
        self.worker.replication.http.secret =
            self.worker.replication.http.secret.take().map(|value| resolve_env_in_string(&value)).transpose()?;
        self.worker.replication.http.secret_path =
            self.worker.replication.http.secret_path.take().map(|value| resolve_env_in_string(&value)).transpose()?;

        Ok(())
    }
}

/// Resolve `${ENV_VAR}` placeholders in a string value.
///
/// Supports four syntaxes:
/// - `${VAR}` — replace with env var value, or empty string if unset
/// - `${VAR:-default}` — use default if env var is unset
/// - `${VAR:=assign}` — use default if env var is unset (deprecated, warns)
/// - `${VAR:?error}` — error if env var is unset
#[allow(clippy::expect_used)]
fn resolve_env_in_string(value: &str) -> Result<String, String> {
    resolve_env_in_string_with(value, &|var| std::env::var(var))
}

#[allow(clippy::expect_used)]
pub(crate) fn resolve_env_in_string_with<F>(value: &str, lookup: &F) -> Result<String, String>
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").expect("static regex is valid"));
    let mut result = value.to_string();

    for cap in re.captures_iter(value) {
        let full_match = cap.get(0).expect("capture group 0 always exists in captures_iter").as_str();
        let inner = cap.get(1).expect("capture group 1 always exists for this regex").as_str();

        let replacement = if inner.contains(":-") {
            let parts: Vec<&str> = inner.splitn(2, ":-").collect();
            let var_name = parts[0];
            let default_value = parts[1];

            let resolved = lookup(var_name).unwrap_or_else(|_| default_value.to_string());
            tracing::debug!("Resolved env var {} (with default): {} -> {}", var_name, full_match, resolved);
            resolved
        } else if inner.contains(":=") {
            let parts: Vec<&str> = inner.splitn(2, ":=").collect();
            let var_name = parts[0];
            let default_value = parts[1];

            let val = lookup(var_name).unwrap_or_else(|_| default_value.to_string());
            tracing::warn!(
                "Config uses ':=' (assign) syntax for env var {} - this is a security risk and will be removed in a future version. Use ':-' (default) instead.",
                var_name
            );
            val
        } else if inner.contains(":?") {
            let parts: Vec<&str> = inner.splitn(2, ":?").collect();
            let var_name = parts[0];
            let error_msg = parts[1];

            let val = match lookup(var_name) {
                Ok(v) => v,
                Err(_) => {
                    return Err(format!("Environment variable {var_name} is required: {error_msg}"));
                }
            };
            tracing::debug!("Resolved required env var {}: {} -> {}", var_name, full_match, val);
            val
        } else {
            let resolved = lookup(inner).unwrap_or_else(|_| "".to_string());
            tracing::debug!("Resolved env var {}: {} -> {}", inner, full_match, resolved);
            resolved
        };

        result = result.replace(full_match, &replacement);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::resolve_env_in_string_with;
    use std::collections::HashMap;
    use std::env::VarError;

    fn lookup_from<'a>(map: &'a HashMap<&str, &str>) -> impl Fn(&str) -> Result<String, VarError> + 'a {
        |var: &str| map.get(var).map(|v| v.to_string()).ok_or(VarError::NotPresent)
    }

    // ── 环境变量**覆盖**的拼写（README 曾写错）──────────────────────

    /// §8.7 待核实项：README「环境变量（覆盖配置）」一节写的是
    /// `SYNAPSE_REDIS__HOST` / `SYNAPSE_DATABASE__HOST`（`SYNAPSE` 后**单**下划线），
    /// 而 `Config::load()` 用的是
    /// `config::Environment::with_prefix("SYNAPSE").separator("__")`，
    /// 代码注释里也一致写作 `SYNAPSE__REDIS__HOST`（**双**下划线）。
    ///
    /// 两种拼写只能有一个生效；按 README 配而实际不生效 = 运维静默地被骗。
    /// 本用例把事实钉死（只断言 config 层的键映射，不反序列化整份 `Config`，
    /// 避免被无关字段的必填性干扰）。
    #[test]
    fn env_override_needs_a_double_underscore_after_the_prefix() {
        let yaml = "redis:\n  host: from-file\n  port: 6379\n";
        let build = || {
            config::Config::builder()
                .add_source(config::File::from_str(yaml, config::FileFormat::Yaml))
                .add_source(config::Environment::with_prefix("SYNAPSE").separator("__"))
                .build()
                .expect("config build")
        };

        // 场景 A：双下划线（与 loader.rs 一致）—— 必须生效。
        crate::config::test_env::set("SYNAPSE__REDIS__HOST", "double-underscore");
        assert_eq!(
            build().get_string("redis.host").expect("redis.host"),
            "double-underscore",
            "`SYNAPSE__REDIS__HOST` 必须能覆盖配置文件里的 redis.host"
        );
        crate::config::test_env::remove("SYNAPSE__REDIS__HOST");

        // 场景 B：单下划线（README 的写法）—— 不生效，值仍是配置文件里的。
        crate::config::test_env::set("SYNAPSE_REDIS__HOST", "single-underscore");
        assert_eq!(
            build().get_string("redis.host").expect("redis.host"),
            "from-file",
            "`SYNAPSE_REDIS__HOST`（单下划线）**不生效** —— README 必须改成双下划线拼写"
        );
        crate::config::test_env::remove("SYNAPSE_REDIS__HOST");
    }

    // ── C9: 部署侧用 env 覆盖 bool（管理员 MFA）──────────────────────

    /// C9 的支点：`security.admin_mfa_required` 是 **bool**，`homeserver.yaml`
    /// 的 `${VAR}` 插值只作用于 String 字段，无法用它开启 MFA。部署侧只能靠
    /// `SYNAPSE__SECURITY__ADMIN_MFA_REQUIRED=true` 覆盖 —— 依赖 config crate
    /// 把环境变量里的字符串 "true" 强制转成 bool。钉死这条行为，否则 config
    /// 升级后可能静默失效，管理员 MFA 会在无人察觉的情况下被关掉。
    #[test]
    fn env_override_coerces_string_to_bool_for_admin_mfa() {
        let yaml = "security:\n  admin_mfa_required: false\n";
        let build = || {
            config::Config::builder()
                .add_source(config::File::from_str(yaml, config::FileFormat::Yaml))
                .add_source(config::Environment::with_prefix("SYNAPSE").separator("__"))
                .build()
                .expect("config build")
        };

        crate::config::test_env::set("SYNAPSE__SECURITY__ADMIN_MFA_REQUIRED", "true");
        assert!(
            build().get_bool("security.admin_mfa_required").expect("admin_mfa_required"),
            "`SYNAPSE__SECURITY__ADMIN_MFA_REQUIRED=true` 必须把字符串强制转成 bool true"
        );
        crate::config::test_env::remove("SYNAPSE__SECURITY__ADMIN_MFA_REQUIRED");
    }

    // ── ${VAR} simple substitution ─────────────────────────────────

    #[test]
    fn resolves_env_var() {
        let vars = HashMap::from([("HOST", "localhost")]);
        let result = resolve_env_in_string_with("${HOST}:8080", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "localhost:8080");
    }

    #[test]
    fn unresolved_var_becomes_empty() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("host=${MISSING}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "host=");
    }

    #[test]
    fn no_placeholders_returns_unchanged() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("plain text", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn resolves_multiple_vars() {
        let vars = HashMap::from([("HOST", "example.com"), ("PORT", "443")]);
        let result = resolve_env_in_string_with("https://${HOST}:${PORT}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "https://example.com:443");
    }

    // ── ${VAR:-default} syntax ─────────────────────────────────────

    #[test]
    fn default_syntax_uses_var_when_set() {
        let vars = HashMap::from([("HOST", "prod.example.com")]);
        let result = resolve_env_in_string_with("${HOST:-localhost}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "prod.example.com");
    }

    #[test]
    fn default_syntax_falls_back_to_default() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("${HOST:-localhost}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "localhost");
    }

    #[test]
    fn default_syntax_with_empty_default() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("${HOST:-}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "");
    }

    // ── ${VAR:=assign} syntax ──────────────────────────────────────

    #[test]
    fn assign_syntax_uses_var_when_set() {
        let vars = HashMap::from([("TOKEN", "secret123")]);
        let result = resolve_env_in_string_with("${TOKEN:=fallback}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "secret123");
    }

    #[test]
    fn assign_syntax_falls_back_to_default() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("${TOKEN:=fallback}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "fallback");
    }

    // ── ${VAR:?error} syntax ───────────────────────────────────────

    #[test]
    fn required_syntax_returns_value_when_set() {
        let vars = HashMap::from([("SECRET", "key123")]);
        let result = resolve_env_in_string_with("${SECRET:?Must set SECRET}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "key123");
    }

    #[test]
    fn required_syntax_errors_when_unset() {
        let vars = HashMap::new();
        let err = resolve_env_in_string_with("${SECRET:?Must set SECRET}", &lookup_from(&vars)).unwrap_err();
        assert!(err.contains("SECRET is required"));
        assert!(err.contains("Must set SECRET"));
    }

    // ── Edge cases ─────────────────────────────────────────────────

    #[test]
    fn empty_string_input() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn adjacent_placeholders() {
        let vars = HashMap::from([("A", "hello"), ("B", "world")]);
        let result = resolve_env_in_string_with("${A}${B}", &lookup_from(&vars)).unwrap();
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn nested_braces_in_default_value() {
        let vars = HashMap::new();
        let result = resolve_env_in_string_with("${VAR:-default{with}braces}", &lookup_from(&vars)).unwrap();
        // Regex \$\{([^}]+)\} stops at the first }, so it captures
        // "VAR:-default{with" and "braces}" remains as literal text.
        assert_eq!(result, "default{withbraces}");
    }
}
