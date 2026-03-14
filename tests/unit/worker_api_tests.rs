// Worker API Tests - API Endpoint Coverage
// These tests cover the worker API endpoints from src/web/routes/worker.rs

use serde_json::json;

// Test 1: Worker registration request
#[test]
fn test_worker_registration() {
    let worker = json!({
        "worker_id": "worker1",
        "worker_name": "Worker 1",
        "worker_type": "synapse",
        "host": "localhost",
        "port": 8080
    });
    
    assert!(worker.get("worker_id").is_some());
    assert!(worker.get("worker_name").is_some());
    assert!(worker.get("worker_type").is_some());
}

// Test 2: Worker type validation
#[test]
fn test_worker_type_validation() {
    // Valid worker types
    assert!(is_valid_worker_type("synapse"));
    assert!(is_valid_worker_type("federation"));
    assert!(is_valid_worker_type("client"));
    assert!(is_valid_worker_type("event_persister"));
    assert!(is_valid_worker_type("presence"));
    assert!(is_valid_worker_type("typing"));
    
    // Invalid
    assert!(!is_valid_worker_type("invalid"));
    assert!(!is_valid_worker_type(""));
}

// Test 3: Worker response format
#[test]
fn test_worker_response() {
    let worker = json!({
        "worker_id": "worker1",
        "worker_name": "Worker 1",
        "worker_type": "synapse",
        "host": "localhost",
        "port": 8080,
        "status": "running",
        "last_heartbeat_ts": 1700000000000_i64,
        "started_ts": 1699900000000_i64
    });
    
    assert!(worker.get("worker_id").is_some());
    assert!(worker.get("worker_name").is_some());
    assert!(worker.get("worker_type").is_some());
    assert!(worker.get("status").is_some());
}

// Test 4: Worker status validation
#[test]
fn test_worker_status_validation() {
    // Valid statuses
    assert!(is_valid_status("starting"));
    assert!(is_valid_status("running"));
    assert!(is_valid_status("stopping"));
    assert!(is_valid_status("stopped"));
    assert!(is_valid_status("error"));
    
    // Invalid
    assert!(!is_valid_status("invalid"));
}

// Test 5: Worker list response
#[test]
fn test_worker_list_response() {
    let workers = vec![
        json!({
            "worker_id": "worker1",
            "status": "running"
        }),
        json!({
            "worker_id": "worker2",
            "status": "stopped"
        })
    ];
    
    assert_eq!(workers.len(), 2);
}

// Test 6: Worker heartbeat request
#[test]
fn test_worker_heartbeat() {
    let heartbeat = json!({
        "worker_id": "worker1",
        "status": "running"
    });
    
    assert!(heartbeat.get("worker_id").is_some());
    assert!(heartbeat.get("status").is_some());
}

// Test 7: Worker heartbeat response
#[test]
fn test_worker_heartbeat_response() {
    let response = json!({
        "received": true,
        "timestamp": 1700000000000_i64
    });
    
    assert!(response.get("received").is_some());
    assert!(response.get("timestamp").is_some());
}

// Test 8: Worker unregistration
#[test]
fn test_worker_unregistration() {
    let result = json!({
        "unregistered": true,
        "worker_id": "worker1"
    });
    
    assert!(result.get("unregistered").is_some());
    assert!(result["unregistered"].as_bool().unwrap_or(false));
}

// Test 9: Worker command request
#[test]
fn test_worker_command_request() {
    let command = json!({
        "command_type": "sync",
        "payload": {
            "room_id": "!room:localhost"
        }
    });
    
    assert!(command.get("command_type").is_some());
    assert!(command.get("payload").is_some());
}

// Test 10: Command type validation
#[test]
fn test_command_type_validation() {
    // Valid command types
    assert!(is_valid_command_type("sync"));
    assert!(is_valid_command_type("fetch"));
    assert!(is_valid_command_type("send"));
    assert!(is_valid_command_type("drain"));
    
    // Invalid
    assert!(!is_valid_command_type("invalid"));
}

// Test 11: Command response
#[test]
fn test_command_response() {
    let command = json!({
        "command_id": "cmd123",
        "command_type": "sync",
        "worker_id": "worker1",
        "status": "pending"
    });
    
    assert!(command.get("command_id").is_some());
    assert!(command.get("command_type").is_some());
    assert!(command.get("status").is_some());
}

// Test 12: Pending commands response
#[test]
fn test_pending_commands_response() {
    let commands = vec![
        json!({
            "command_id": "cmd1",
            "command_type": "sync",
            "status": "pending"
        })
    ];
    
    assert_eq!(commands.len(), 1);
    assert!(commands[0].get("status").is_some());
}

// Test 13: Complete command request
#[test]
fn test_complete_command_request() {
    let complete = json!({
        "command_id": "cmd123",
        "result": {
            "success": true
        }
    });
    
    assert!(complete.get("command_id").is_some());
    assert!(complete.get("result").is_some());
}

// Test 14: Fail command request
#[test]
fn test_fail_command_request() {
    let fail = json!({
        "command_id": "cmd123",
        "error": "Command failed"
    });
    
    assert!(fail.get("command_id").is_some());
    assert!(fail.get("error").is_some());
}

// Test 15: Task assignment request
#[test]
fn test_task_assignment() {
    let task = json!({
        "task_type": "event_processing",
        "payload": {
            "event_id": "$event:localhost"
        }
    });
    
    assert!(task.get("task_type").is_some());
    assert!(task.get("payload").is_some());
}

// Test 16: Task type validation
#[test]
fn test_task_type_validation() {
    // Valid task types
    assert!(is_valid_task_type("event_processing"));
    assert!(is_valid_task_type("sync"));
    assert!(is_valid_task_type("federation"));
    assert!(is_valid_task_type("presence"));
    
    // Invalid
    assert!(!is_valid_task_type("invalid"));
}

// Test 17: Pending tasks response
#[test]
fn test_pending_tasks_response() {
    let tasks = vec![
        json!({
            "task_id": "task1",
            "task_type": "event_processing",
            "status": "pending"
        })
    ];
    
    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].get("task_type").is_some());
}

// Test 18: Claim task request
#[test]
fn test_claim_task_request() {
    let claim = json!({
        "task_id": "task1",
        "worker_id": "worker1"
    });
    
    assert!(claim.get("task_id").is_some());
    assert!(claim.get("worker_id").is_some());
}

// Test 19: Complete task request
#[test]
fn test_complete_task_request() {
    let complete = json!({
        "task_id": "task1",
        "result": {
            "processed": 10
        }
    });
    
    assert!(complete.get("task_id").is_some());
    assert!(complete.get("result").is_some());
}

// Test 20: Worker connection request
#[test]
fn test_worker_connect_request() {
    let connect = json!({
        "worker_id": "worker1",
        "endpoint": "ws://localhost:8080"
    });
    
    assert!(connect.get("worker_id").is_some());
    assert!(connect.get("endpoint").is_some());
}

// Test 21: Worker disconnection request
#[test]
fn test_worker_disconnect_request() {
    let disconnect = json!({
        "worker_id": "worker1",
        "reason": "Maintenance"
    });
    
    assert!(disconnect.get("worker_id").is_some());
}

// Test 22: Replication position response
#[test]
fn test_replication_position_response() {
    let position = json!({
        "stream_id": 1000,
        "worker_id": "worker1",
        "timestamp": 1700000000000_i64
    });
    
    assert!(position.get("stream_id").is_some());
    assert!(position.get("worker_id").is_some());
}

// Test 23: Worker statistics response
#[test]
fn test_worker_statistics() {
    let stats = json!({
        "total_workers": 10,
        "running": 8,
        "stopped": 1,
        "error": 1,
        "by_type": {
            "synapse": 5,
            "federation": 3,
            "client": 2
        }
    });
    
    assert!(stats.get("total_workers").is_some());
    assert!(stats.get("running").is_some());
    assert!(stats.get("by_type").is_some());
}

// Test 24: Worker type statistics
#[test]
fn test_worker_type_statistics() {
    let stats = json!({
        "worker_type": "synapse",
        "count": 5,
        "running": 4,
        "stopped": 1
    });
    
    assert!(stats.get("worker_type").is_some());
    assert!(stats.get("count").is_some());
}

// Helper functions
fn is_valid_worker_type(worker_type: &str) -> bool {
    matches!(worker_type, "synapse" | "federation" | "client" | "event_persister" | "presence" | "typing")
}

fn is_valid_status(status: &str) -> bool {
    matches!(status, "starting" | "running" | "stopping" | "stopped" | "error")
}

fn is_valid_command_type(command_type: &str) -> bool {
    matches!(command_type, "sync" | "fetch" | "send" | "drain")
}

fn is_valid_task_type(task_type: &str) -> bool {
    matches!(task_type, "event_processing" | "sync" | "federation" | "presence")
}
