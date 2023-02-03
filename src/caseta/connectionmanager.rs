use std::time::Duration;

use anyhow::bail;
use async_trait::async_trait;
use thiserror::Error;
use tokio::net::TcpStream;
use url::Host;

use super::message::Message;

#[async_trait]
pub trait TcpSocketProvider {
    async fn new_socket(&self) -> Result<TcpStream, anyhow::Error>;
}

pub struct DefaultTcpSocketProvider {
    address: Host<String>,
    port: u16,
}

impl DefaultTcpSocketProvider {
    pub fn new(address: Host<String>, port: u16) -> Self {
        DefaultTcpSocketProvider { address, port }
    }
}

#[async_trait]
impl TcpSocketProvider for DefaultTcpSocketProvider {
    async fn new_socket(&self) -> Result<TcpStream, anyhow::Error> {
        let connection = tokio::time::timeout(
            Duration::from_secs(10),
            TcpStream::connect((self.address.to_string(), self.port)),
        )
        .await;

        match connection {
            Ok(Ok(tcp_stream)) => Ok(tcp_stream),
            Ok(Err(e)) => bail!("unable to connect"),
            Err(_elapsed) => bail!("timed out trying to connect"),
        }
    }
}

#[derive(Error, Debug)]
pub enum CasetaConnectionLivenessError {
    #[error("there was a problem refreshing the connection liveness")]
    KeepAliveRefreshError,
}

#[async_trait]
pub trait ConnectionLivenessRefresher {
    async fn RefreshConnectionLiveness() -> Result<(), CasetaConnectionLivenessError>;
}

#[async_trait]
pub trait ReadWriteConnection {
    async fn await_message(&self) -> Result<Option<Message>, anyhow::Error>;
    async fn write_message(&self, message: String) -> Result<(), anyhow::Error>;
}

pub struct CasetaConnectionManager {
    connection: Option<TcpStream>,
}

impl CasetaConnectionManager {
    pub fn new() -> Self {
        Self {
            connection: Option::None,
        }
    }
}

#[async_trait]
impl ReadWriteConnection for CasetaConnectionManager {
    async fn await_message(&self) -> Result<Option<Message>, anyhow::Error> {
        todo!()
    }

    async fn write_message(&self, message: String) -> Result<(), anyhow::Error> {
        todo!()
    }
}

pub struct DelegatingCasetaConnectionManager {
    internal_connection_manager: CasetaConnectionManager,
}
