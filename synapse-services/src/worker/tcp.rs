use crate::worker::protocol::{ReplicationCommand, ReplicationError, ReplicationProtocol};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    time::{timeout, Duration},
};
use tracing::{debug, info};

/// GHSA-8q93 mitigation: Maximum time to wait when acquiring the
/// `ReplicationConnection::client` Mutex before failing closed.
///
/// Without this bound, an attacker can starve the CPU by flooding the
/// worker with concurrent `send_command`/`ping` calls that all contend
/// on the same lock. 5 seconds is generous for an internal worker lock;
/// normal operation acquires the lock in microseconds.
const LOCK_ACQUIRE_TIMEOUT_SECS: u64 = 5;

/// The `TcpReplicationClient` struct.
pub struct TcpReplicationClient {
    stream: Option<TcpStream>,
    protocol: ReplicationProtocol,
    worker_name: String,
}

impl TcpReplicationClient {
    /// See [`new`].
    pub fn new(worker_name: String) -> Self {
        Self { stream: None, protocol: ReplicationProtocol::new(), worker_name }
    }

    /// See [`connect`].
    pub async fn connect(&mut self, addr: &str) -> Result<(), ReplicationError> {
        let stream = timeout(Duration::from_secs(10), TcpStream::connect(addr))
            .await
            .map_err(|_| ReplicationError::IoError("Connection timeout".to_string()))?
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        info!(remote_addr = %addr, worker_name = %self.worker_name, "Connected to replication server");
        self.stream = Some(stream);

        self.send_name().await?;

        Ok(())
    }

    async fn send_name(&mut self) -> Result<(), ReplicationError> {
        let stream = self.stream.as_mut().ok_or_else(|| ReplicationError::IoError("Not connected".to_string()))?;

        let name_cmd = ReplicationCommand::Name { name: self.worker_name.clone() };
        stream
            .write_all(self.protocol.encode_command(&name_cmd).as_slice())
            .await
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        Ok(())
    }

    /// See [`send_command`].
    pub async fn send_command(&mut self, command: &ReplicationCommand) -> Result<(), ReplicationError> {
        let stream = self.stream.as_mut().ok_or_else(|| ReplicationError::IoError("Not connected".to_string()))?;

        stream
            .write_all(self.protocol.encode_command(command).as_slice())
            .await
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        debug!("Sent command: {:?}", command);
        Ok(())
    }

    /// See [`receive_command`].
    pub async fn receive_command(&mut self) -> Result<ReplicationCommand, ReplicationError> {
        let stream = self.stream.as_mut().ok_or_else(|| ReplicationError::IoError("Not connected".to_string()))?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        let bytes_read = reader.read_line(&mut line).await.map_err(|e| ReplicationError::IoError(e.to_string()))?;

        if bytes_read == 0 {
            return Err(ReplicationError::ConnectionClosed);
        }

        let command = self.protocol.decode_command(line.as_bytes())?;
        debug!("Received command: {:?}", command);
        Ok(command)
    }

    /// See [`ping`].
    pub async fn ping(&mut self) -> Result<i64, ReplicationError> {
        let start = current_timestamp_millis();
        self.send_command(&ReplicationProtocol::create_ping()).await?;

        match timeout(Duration::from_secs(5), self.receive_command()).await {
            Ok(Ok(ReplicationCommand::Pong { timestamp: _, .. })) => {
                let latency = current_timestamp_millis() - start;
                debug!("Ping latency: {}ms", latency);
                Ok(latency)
            }
            Ok(Ok(cmd)) => Err(ReplicationError::IoError(format!("Expected Pong, got {cmd:?}"))),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ReplicationError::IoError("Ping timeout".to_string())),
        }
    }

    /// See [`sync_stream`].
    pub async fn sync_stream(&mut self, stream_name: &str, position: i64) -> Result<(), ReplicationError> {
        self.send_command(&ReplicationProtocol::create_sync(stream_name, position)).await
    }

    /// See [`send_position`].
    pub async fn send_position(&mut self, stream_name: &str, position: i64) -> Result<(), ReplicationError> {
        self.send_command(&ReplicationProtocol::create_position(stream_name, position)).await
    }

    /// See [`is_connected`].
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// See [`disconnect`].
    pub async fn disconnect(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
            info!(worker_name = %self.worker_name, "Disconnected from replication server");
        }
    }
}

/// The `ReplicationConnection` struct.
#[derive(Clone)]
pub struct ReplicationConnection {
    client: Arc<tokio::sync::Mutex<Option<TcpReplicationClient>>>,
    worker_name: String,
}

impl ReplicationConnection {
    /// See [`new`].
    pub fn new(worker_name: String) -> Self {
        Self { client: Arc::new(tokio::sync::Mutex::new(None)), worker_name }
    }

    /// See [`connect`].
    pub async fn connect(&self, addr: &str) -> Result<(), ReplicationError> {
        let mut client = TcpReplicationClient::new(self.worker_name.clone());
        client.connect(addr).await?;

        let mut guard = self.client.lock().await;
        *guard = Some(client);

        Ok(())
    }

    /// See [`send_command`].
    pub async fn send_command(&self, command: &ReplicationCommand) -> Result<(), ReplicationError> {
        // GHSA-8q93: Bound lock acquisition to prevent CPU starvation under
        // contention. Fail closed (return error) instead of hanging indefinitely.
        let mut guard = timeout(Duration::from_secs(LOCK_ACQUIRE_TIMEOUT_SECS), self.client.lock())
            .await
            .map_err(|_| {
                ReplicationError::IoError(format!(
                    "Worker lock acquisition timed out after {LOCK_ACQUIRE_TIMEOUT_SECS}s — possible contention or deadlock"
                ))
            })?;
        if let Some(ref mut client) = *guard {
            client.send_command(command).await
        } else {
            Err(ReplicationError::IoError("Not connected".to_string()))
        }
    }

    /// See [`ping`].
    pub async fn ping(&self) -> Result<i64, ReplicationError> {
        // GHSA-8q93: Same lock-acquire timeout as send_command.
        let mut guard = timeout(Duration::from_secs(LOCK_ACQUIRE_TIMEOUT_SECS), self.client.lock())
            .await
            .map_err(|_| {
                ReplicationError::IoError(format!(
                    "Worker lock acquisition timed out after {LOCK_ACQUIRE_TIMEOUT_SECS}s — possible contention or deadlock"
                ))
            })?;
        if let Some(ref mut client) = *guard {
            client.ping().await
        } else {
            Err(ReplicationError::IoError("Not connected".to_string()))
        }
    }

    /// See [`disconnect`].
    pub async fn disconnect(&self) {
        let mut guard = self.client.lock().await;
        if let Some(ref mut client) = *guard {
            client.disconnect().await;
        }
        *guard = None;
    }

    /// See [`is_connected`].
    pub async fn is_connected(&self) -> bool {
        let guard = self.client.lock().await;
        guard.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_replication_client_creation() {
        let client = TcpReplicationClient::new("worker1".to_string());
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_replication_connection() {
        let conn = ReplicationConnection::new("worker1".to_string());
        assert!(!conn.is_connected().await);
    }

    #[test]
    fn test_protocol_clone() {
        let protocol = ReplicationProtocol::new();
        let cloned = protocol.clone();
        let cmd = ReplicationProtocol::create_ping();
        assert_eq!(protocol.encode_command(&cmd), cloned.encode_command(&cmd));
    }

    #[test]
    fn test_replication_connection_new() {
        let conn = ReplicationConnection::new("worker1".to_string());
        assert_eq!(conn.worker_name, "worker1");
    }

    #[tokio::test]
    async fn test_replication_client_disconnect_when_not_connected() {
        let mut client = TcpReplicationClient::new("worker1".to_string());
        assert!(!client.is_connected());
        client.disconnect().await;
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_replication_connection_disconnect_when_not_connected() {
        let conn = ReplicationConnection::new("worker1".to_string());
        conn.disconnect().await;
        assert!(!conn.is_connected().await);
    }

    #[test]
    fn test_replication_protocol_create_ping() {
        let ping = ReplicationProtocol::create_ping();
        match ping {
            ReplicationCommand::Ping { timestamp } => assert!(timestamp > 0),
            _ => panic!("Expected Ping command"),
        }
    }

    #[test]
    fn test_replication_protocol_create_sync() {
        let sync = ReplicationProtocol::create_sync("stream1", 42);
        match sync {
            ReplicationCommand::Sync { stream_name, position } => {
                assert_eq!(stream_name, "stream1");
                assert_eq!(position, 42);
            }
            _ => panic!("Expected Sync command"),
        }
    }

    #[test]
    fn test_replication_protocol_create_position() {
        let pos = ReplicationProtocol::create_position("stream1", 100);
        match pos {
            ReplicationCommand::Position { stream_name, position } => {
                assert_eq!(stream_name, "stream1");
                assert_eq!(position, 100);
            }
            _ => panic!("Expected Position command"),
        }
    }

    // ── GHSA-8q93: Worker Lock DoS mitigation ──────────────────────────
    //
    // `send_command` and `ping` acquire a Mutex before performing TCP I/O.
    // Without a bound on lock acquisition, an attacker can starve the CPU
    // by flooding the worker with concurrent requests that all contend on
    // the same lock. These tests verify that lock acquisition is bounded
    // by a timeout and fails closed (returns error, does not hang).

    #[tokio::test(start_paused = true)]
    async fn test_send_command_times_out_under_lock_contention() {
        let conn = ReplicationConnection::new("worker1".to_string());

        // Hold the client lock to simulate contention (e.g., a slow TCP peer
        // or an attacker flooding concurrent send_command calls).
        let _held_guard = conn.client.lock().await;

        // send_command should time out trying to acquire the lock, not hang.
        let cmd = ReplicationProtocol::create_ping();
        let send_fut = conn.send_command(&cmd);

        // Advance virtual time past the lock-acquire timeout.
        tokio::time::advance(Duration::from_secs(LOCK_ACQUIRE_TIMEOUT_SECS + 1)).await;

        let result = send_fut.await;
        assert!(result.is_err(), "send_command must fail when lock acquisition times out");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.to_lowercase().contains("timeout") || err_msg.to_lowercase().contains("timed out"),
            "error should mention timeout, got: {err_msg}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_ping_times_out_under_lock_contention() {
        let conn = ReplicationConnection::new("worker1".to_string());

        let _held_guard = conn.client.lock().await;

        let ping_fut = conn.ping();

        tokio::time::advance(Duration::from_secs(LOCK_ACQUIRE_TIMEOUT_SECS + 1)).await;

        let result = ping_fut.await;
        assert!(result.is_err(), "ping must fail when lock acquisition times out");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.to_lowercase().contains("timeout") || err_msg.to_lowercase().contains("timed out"),
            "error should mention timeout, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_send_command_fails_closed_when_not_connected() {
        // Without contention, send_command on a disconnected client should
        // still fail-closed with "Not connected" — not hang or silently succeed.
        let conn = ReplicationConnection::new("worker1".to_string());
        let cmd = ReplicationProtocol::create_ping();
        let result = conn.send_command(&cmd).await;
        assert!(result.is_err());
    }
}
