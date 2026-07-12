use reqwest::Client;
use std::time::Duration;
use tokio::net::TcpStream;

const HEALTH_PATHS: [&str; 1] = ["/health"];

fn env_flag(name: &str) -> bool {
    std::env::var(name).map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")).unwrap_or(false)
}

#[tokio::main]
async fn main() {
    let host = std::env::var("SYNAPSE_HEALTHCHECK_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("SYNAPSE_HEALTHCHECK_PORT").unwrap_or_else(|_| "8008".to_string());
    let base_url = std::env::var("SYNAPSE_HEALTHCHECK_BASE_URL").unwrap_or_else(|_| format!("http://{host}:{port}"));
    let tcp_only = env_flag("SYNAPSE_HEALTHCHECK_TCP_ONLY");

    let client = match Client::builder().timeout(Duration::from_secs(5)).build() {
        Ok(client) => client,
        Err(_) => std::process::exit(1),
    };

    if !tcp_only {
        let paths = HEALTH_PATHS;

        for path in paths {
            match client.get(format!("{base_url}{path}")).send().await {
                Ok(response) if response.status().is_success() => std::process::exit(0),
                _ => {}
            }
        }
    }

    if let Ok(port) = port.parse::<u16>() {
        if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((host.as_str(), port))).await
        {
            std::process::exit(0);
        }
    }

    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_probe_must_not_trust_versions() {
        assert!(
            !HEALTH_PATHS.contains(&"/_matrix/client/versions"),
            "/versions returns 200 without DB; must not be a health signal"
        );
        assert!(
            !HEALTH_PATHS.contains(&"/_matrix/federation/v1/version"),
            "/federation version returns 200 without DB; must not be a health signal"
        );
    }
}
