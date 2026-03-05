use crate::worker::protocol::{ReplicationCommand, ReplicationError, ReplicationProtocol};
use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
    time::{timeout, Duration},
};
use tracing::{debug, error, info, warn};

pub struct TcpReplicationServer {
    listener: TcpListener,
    #[allow(dead_code)]
    protocol: ReplicationProtocol,
    server_name: String,
    command_tx: Sender<ReplicationCommand>,
    command_rx: Option<Receiver<ReplicationCommand>>,
}

impl TcpReplicationServer {
    pub async fn bind(addr: &str, server_name: String) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(addr).await?;
        let (command_tx, command_rx) = mpsc::channel(100);

        info!("TCP replication server listening on {}", addr);

        Ok(Self {
            listener,
            protocol: ReplicationProtocol::new(),
            server_name,
            command_tx,
            command_rx: Some(command_rx),
        })
    }

    pub fn get_command_receiver(&mut self) -> Option<Receiver<ReplicationCommand>> {
        self.command_rx.take()
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        loop {
            let (stream, addr) = self.listener.accept().await?;
            info!("New replication connection from {}", addr);

            let protocol = ReplicationProtocol::new();
            let server_name = self.server_name.clone();
            let command_tx = self.command_tx.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    Self::handle_connection(stream, protocol, server_name, command_tx).await
                {
                    error!("Connection error from {}: {}", addr, e);
                }
            });
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        protocol: ReplicationProtocol,
        server_name: String,
        command_tx: Sender<ReplicationCommand>,
    ) -> Result<(), ReplicationError> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        writer
            .write_all(
                protocol
                    .encode_command(&ReplicationProtocol::create_pong(&server_name))
                    .as_slice(),
            )
            .await
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        loop {
            line.clear();
            let bytes_read = reader
                .read_line(&mut line)
                .await
                .map_err(|e| ReplicationError::IoError(e.to_string()))?;

            if bytes_read == 0 {
                info!("Connection closed by client");
                return Err(ReplicationError::ConnectionClosed);
            }

            let command = protocol.decode_command(line.as_bytes())?;
            debug!("Received command: {:?}", command);

            match &command {
                ReplicationCommand::Ping { timestamp } => {
                    let pong = ReplicationCommand::Pong {
                        timestamp: *timestamp,
                        server_name: server_name.clone(),
                    };
                    writer
                        .write_all(protocol.encode_command(&pong).as_slice())
                        .await
                        .map_err(|e| ReplicationError::IoError(e.to_string()))?;
                }
                ReplicationCommand::Name { name } => {
                    info!("Worker identified as: {}", name);
                }
                _ => {
                    if let Err(e) = command_tx.send(command).await {
                        warn!("Failed to send command to channel: {}", e);
                    }
                }
            }
        }
    }
}

pub struct TcpReplicationClient {
    stream: Option<TcpStream>,
    protocol: ReplicationProtocol,
    #[allow(dead_code)]
    server_name: String,
    worker_name: String,
}

impl TcpReplicationClient {
    pub fn new(server_name: String, worker_name: String) -> Self {
        Self {
            stream: None,
            protocol: ReplicationProtocol::new(),
            server_name,
            worker_name,
        }
    }

    pub async fn connect(&mut self, addr: &str) -> Result<(), ReplicationError> {
        let stream = timeout(Duration::from_secs(10), TcpStream::connect(addr))
            .await
            .map_err(|_| ReplicationError::IoError("Connection timeout".to_string()))?
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        info!("Connected to replication server at {}", addr);
        self.stream = Some(stream);

        self.send_name().await?;

        Ok(())
    }

    async fn send_name(&mut self) -> Result<(), ReplicationError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| ReplicationError::IoError("Not connected".to_string()))?;

        let name_cmd = ReplicationCommand::Name {
            name: self.worker_name.clone(),
        };
        stream
            .write_all(self.protocol.encode_command(&name_cmd).as_slice())
            .await
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        Ok(())
    }

    pub async fn send_command(
        &mut self,
        command: &ReplicationCommand,
    ) -> Result<(), ReplicationError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| ReplicationError::IoError("Not connected".to_string()))?;

        stream
            .write_all(self.protocol.encode_command(command).as_slice())
            .await
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        debug!("Sent command: {:?}", command);
        Ok(())
    }

    pub async fn receive_command(&mut self) -> Result<ReplicationCommand, ReplicationError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| ReplicationError::IoError("Not connected".to_string()))?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| ReplicationError::IoError(e.to_string()))?;

        if bytes_read == 0 {
            return Err(ReplicationError::ConnectionClosed);
        }

        let command = self.protocol.decode_command(line.as_bytes())?;
        debug!("Received command: {:?}", command);
        Ok(command)
    }

    pub async fn ping(&mut self) -> Result<i64, ReplicationError> {
        let start = chrono::Utc::now().timestamp_millis();
        self.send_command(&ReplicationProtocol::create_ping())
            .await?;

        match timeout(Duration::from_secs(5), self.receive_command()).await {
            Ok(Ok(ReplicationCommand::Pong { timestamp: _, .. })) => {
                let latency = chrono::Utc::now().timestamp_millis() - start;
                debug!("Ping latency: {}ms", latency);
                Ok(latency)
            }
            Ok(Ok(cmd)) => Err(ReplicationError::IoError(format!(
                "Expected Pong, got {:?}",
                cmd
            ))),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ReplicationError::IoError("Ping timeout".to_string())),
        }
    }

    pub async fn sync_stream(
        &mut self,
        stream_name: &str,
        position: i64,
    ) -> Result<(), ReplicationError> {
        self.send_command(&ReplicationProtocol::create_sync(stream_name, position))
            .await
    }

    pub async fn send_position(
        &mut self,
        stream_name: &str,
        position: i64,
    ) -> Result<(), ReplicationError> {
        self.send_command(&ReplicationProtocol::create_position(stream_name, position))
            .await
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    pub async fn disconnect(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
            info!("Disconnected from replication server");
        }
    }
}

#[derive(Clone)]
pub struct ReplicationConnection {
    client: Arc<tokio::sync::Mutex<Option<TcpReplicationClient>>>,
    server_name: String,
    worker_name: String,
}

impl ReplicationConnection {
    pub fn new(server_name: String, worker_name: String) -> Self {
        Self {
            client: Arc::new(tokio::sync::Mutex::new(None)),
            server_name,
            worker_name,
        }
    }

    pub async fn connect(&self, addr: &str) -> Result<(), ReplicationError> {
        let mut client =
            TcpReplicationClient::new(self.server_name.clone(), self.worker_name.clone());
        client.connect(addr).await?;

        let mut guard = self.client.lock().await;
        *guard = Some(client);

        Ok(())
    }

    pub async fn send_command(&self, command: &ReplicationCommand) -> Result<(), ReplicationError> {
        let mut guard = self.client.lock().await;
        if let Some(ref mut client) = *guard {
            client.send_command(command).await
        } else {
            Err(ReplicationError::IoError("Not connected".to_string()))
        }
    }

    pub async fn ping(&self) -> Result<i64, ReplicationError> {
        let mut guard = self.client.lock().await;
        if let Some(ref mut client) = *guard {
            client.ping().await
        } else {
            Err(ReplicationError::IoError("Not connected".to_string()))
        }
    }

    pub async fn disconnect(&self) {
        let mut guard = self.client.lock().await;
        if let Some(ref mut client) = *guard {
            client.disconnect().await;
        }
        *guard = None;
    }

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
        let client = TcpReplicationClient::new("server.com".to_string(), "worker1".to_string());
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_replication_connection() {
        let conn = ReplicationConnection::new("server.com".to_string(), "worker1".to_string());
        assert!(!conn.is_connected().await);
    }

    #[test]
    fn test_protocol_clone() {
        let protocol = ReplicationProtocol::new();
        let cloned = protocol.clone();
        let cmd = ReplicationProtocol::create_ping();
        assert_eq!(protocol.encode_command(&cmd), cloned.encode_command(&cmd));
    }
}
