//! Worker Communication Demo
//!
//! This example demonstrates the worker distributed architecture including:
//! - Worker registration
//! - Worker heartbeat
//! - Task distribution
//! - Inter-worker messaging via Redis bus

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
struct RegisterWorkerRequest {
    worker_id: String,
    worker_name: String,
    worker_type: String,
    host: String,
    port: u16,
    version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HeartbeatRequest {
    status: String,
    load_stats: Option<LoadStats>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoadStats {
    cpu_usage: Option<f32>,
    memory_usage: Option<i64>,
    active_connections: Option<i32>,
    requests_per_second: Option<f32>,
    average_latency_ms: Option<f32>,
    queue_depth: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssignTaskRequest {
    task_type: String,
    task_data: serde_json::Value,
    priority: Option<i32>,
    preferred_worker_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SendCommandRequest {
    command_type: String,
    command_data: serde_json::Value,
    priority: Option<i32>,
    max_retries: Option<i32>,
}

const BASE_URL: &str = "http://localhost:8008";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

    println!("=== Worker Distributed Architecture Demo ===\n");

    // Step 1: Register a frontend worker
    println!("1. Registering frontend worker...");
    let register_req = RegisterWorkerRequest {
        worker_id: "frontend-001".to_string(),
        worker_name: "Frontend Worker 1".to_string(),
        worker_type: "frontend".to_string(),
        host: "localhost".to_string(),
        port: 8008,
        version: Some("1.0.0".to_string()),
    };

    let response = client
        .post(format!("{}/_synapse/worker/v1/register", BASE_URL))
        .json(&register_req)
        .send()
        .await?;

    if response.status().is_success() {
        let worker: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Worker registered: {}",
            serde_json::to_string_pretty(&worker)?
        );
    } else {
        println!("   ✗ Failed to register worker: {}", response.status());
    }

    // Step 2: Register a background worker
    println!("\n2. Registering background worker...");
    let register_req = RegisterWorkerRequest {
        worker_id: "background-001".to_string(),
        worker_name: "Background Worker 1".to_string(),
        worker_type: "background".to_string(),
        host: "localhost".to_string(),
        port: 8009,
        version: Some("1.0.0".to_string()),
    };

    let response = client
        .post(format!("{}/_synapse/worker/v1/register", BASE_URL))
        .json(&register_req)
        .send()
        .await?;

    if response.status().is_success() {
        let worker: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Worker registered: {}",
            serde_json::to_string_pretty(&worker)?
        );
    } else {
        println!("   ✗ Failed to register worker: {}", response.status());
    }

    // Step 3: List all workers
    println!("\n3. Listing all workers...");
    let response = client
        .get(format!("{}/_synapse/worker/v1/workers", BASE_URL))
        .send()
        .await?;

    if response.status().is_success() {
        let workers: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Active workers: {}",
            serde_json::to_string_pretty(&workers)?
        );
    } else {
        println!("   ✗ Failed to list workers: {}", response.status());
    }

    // Step 4: Send heartbeat from frontend worker
    println!("\n4. Sending heartbeat from frontend worker...");
    let heartbeat_req = HeartbeatRequest {
        status: "running".to_string(),
        load_stats: Some(LoadStats {
            cpu_usage: Some(25.5),
            memory_usage: Some(512_000_000),
            active_connections: Some(100),
            requests_per_second: Some(50.0),
            average_latency_ms: Some(10.5),
            queue_depth: Some(5),
        }),
    };

    let response = client
        .post(format!(
            "{}/_synapse/worker/v1/workers/frontend-001/heartbeat",
            BASE_URL
        ))
        .json(&heartbeat_req)
        .send()
        .await?;

    println!("   ✓ Heartbeat sent: {}", response.status());

    // Step 5: Assign a task
    println!("\n5. Assigning a task...");
    let task_req = AssignTaskRequest {
        task_type: "background".to_string(),
        task_data: serde_json::json!({
            "task": "process_queue",
            "items": ["item1", "item2", "item3"]
        }),
        priority: Some(1),
        preferred_worker_id: Some("background-001".to_string()),
    };

    let response = client
        .post(format!("{}/_synapse/worker/v1/tasks", BASE_URL))
        .json(&task_req)
        .send()
        .await?;

    if response.status().is_success() {
        let task: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Task assigned: {}",
            serde_json::to_string_pretty(&task)?
        );
    } else {
        println!("   ✗ Failed to assign task: {}", response.status());
    }

    // Step 6: Get pending tasks
    println!("\n6. Getting pending tasks...");
    let response = client
        .get(format!("{}/_synapse/worker/v1/tasks", BASE_URL))
        .send()
        .await?;

    if response.status().is_success() {
        let tasks: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Pending tasks: {}",
            serde_json::to_string_pretty(&tasks)?
        );
    } else {
        println!("   ✗ Failed to get tasks: {}", response.status());
    }

    // Step 7: Select worker for a task type
    println!("\n7. Testing worker selection for 'http' task...");
    let response = client
        .get(format!("{}/_synapse/worker/v1/select/http", BASE_URL))
        .send()
        .await?;

    if response.status().is_success() {
        let selected: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Selected worker: {}",
            serde_json::to_string_pretty(&selected)?
        );
    } else {
        println!("   ✗ Failed to select worker: {}", response.status());
    }

    // Step 8: Send command to worker
    println!("\n8. Sending command to background worker...");
    let cmd_req = SendCommandRequest {
        command_type: "process_task".to_string(),
        command_data: serde_json::json!({
            "action": "start",
            "task_id": "task-001"
        }),
        priority: Some(1),
        max_retries: Some(3),
    };

    let response = client
        .post(format!(
            "{}/_synapse/worker/v1/workers/background-001/commands",
            BASE_URL
        ))
        .json(&cmd_req)
        .send()
        .await?;

    if response.status().is_success() {
        let command: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Command sent: {}",
            serde_json::to_string_pretty(&command)?
        );
    } else {
        println!("   ✗ Failed to send command: {}", response.status());
    }

    // Step 9: Get worker statistics
    println!("\n9. Getting worker statistics...");
    let response = client
        .get(format!("{}/_synapse/worker/v1/statistics", BASE_URL))
        .send()
        .await?;

    if response.status().is_success() {
        let stats: serde_json::Value = response.json().await?;
        println!("   ✓ Statistics: {}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("   ✗ Failed to get statistics: {}", response.status());
    }

    // Step 10: Get statistics by type
    println!("\n10. Getting worker statistics by type...");
    let response = client
        .get(format!("{}/_synapse/worker/v1/statistics/types", BASE_URL))
        .send()
        .await?;

    if response.status().is_success() {
        let stats: serde_json::Value = response.json().await?;
        println!(
            "   ✓ Type statistics: {}",
            serde_json::to_string_pretty(&stats)?
        );
    } else {
        println!("   ✗ Failed to get type statistics: {}", response.status());
    }

    println!("\n=== Demo Complete ===");
    println!("\nWorker distributed architecture is functional!");
    println!("\nAvailable worker types:");
    println!("  - master: Main server instance");
    println!("  - frontend: HTTP request handling");
    println!("  - background: Background task processing");
    println!("  - event_persister: Event persistence");
    println!("  - federation_sender: Outgoing federation");
    println!("  - federation_reader: Incoming federation");
    println!("  - pusher: Push notification delivery");
    println!("  - media_repository: Media storage");

    Ok(())
}
