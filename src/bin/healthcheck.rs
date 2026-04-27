use reqwest::Client;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let base_url = std::env::var("SYNAPSE_HEALTHCHECK_BASE_URL").unwrap_or_else(|_| {
        let host =
            std::env::var("SYNAPSE_HEALTHCHECK_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port =
            std::env::var("SYNAPSE_HEALTHCHECK_PORT").unwrap_or_else(|_| "8008".to_string());
        format!("http://{host}:{port}")
    });

    let client = match Client::builder().timeout(Duration::from_secs(5)).build() {
        Ok(client) => client,
        Err(_) => std::process::exit(1),
    };

    let paths = [
        "/health",
        "/_matrix/client/versions",
        "/_matrix/federation/v1/version",
    ];

    for path in paths {
        match client.get(format!("{base_url}{path}")).send().await {
            Ok(response) if response.status().is_success() => std::process::exit(0),
            _ => {}
        }
    }

    std::process::exit(1);
}
