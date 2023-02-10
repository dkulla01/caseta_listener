use std::{iter, str::FromStr, time::Duration};

use anyhow::bail;
use async_trait::async_trait;
use bytes::BytesMut;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
    sync::{mpsc, oneshot},
    time,
};
use tracing::{debug, error, info};
use url::Host;

use super::{connection::CasetaConnectionError, message::Message};

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
    async fn await_message(&self) -> Result<Option<Message>, ConnectionManagerError>;
    // async fn await_dead_connection(&self) -> Result<(), ConnectionManagerError>;
    async fn write_message(&self, message: String) -> Result<(), ConnectionManagerError>;
}

#[derive(Error, Debug)]
enum ConnectionManagerError {
    #[error("received an empty message")]
    EmptyMessageError,
    #[error("the caseta connection is no longer live")]
    LivenessError,
    #[error("encountered an error: {0}, the underlying connection should be refreshed.")]
    RecoverableError(String),
    #[error("encountered an unrecoverable error: {0}")]
    UnrecoverableError(String),
}

pub struct CasetaConnectionManager {
    connection: Option<BufWriter<TcpStream>>,
    disconnect_sender: mpsc::Sender<()>,
    disconnect_receiver: mpsc::Receiver<()>,
}

#[async_trait]
impl<'a> ReadWriteConnection for CasetaConnectionManager {
    async fn await_message(&self) -> Result<Option<Message>, ConnectionManagerError> {
        self.read_frame().await
    }

    async fn write_message(&self, message: String) -> Result<(), ConnectionManagerError> {
        let stream = match self.connection {
            Some(ref mut buf_writer) => buf_writer,
            None => {
                return Err(ConnectionManagerError::UnrecoverableError(
                    "Write was called before the connection was initialized. this is a bug"
                        .to_string(),
                ))
            }
        };
        let outcome = stream.write(message.as_bytes()).await;

        match outcome {
            Ok(_) => {}
            Err(e) => {
                error!(error=%e, "couldn't write the socket read/write buffer");
                return Err(ConnectionManagerError::UnrecoverableError(format!("unable to write the socket read/write buffer. was the connection closed?, error: {}", e)));
            }
        }

        let outcome = stream.flush().await;
        match outcome {
            Ok(_) => Ok(()),
            Err(e) => {
                error!(error=%e, "couldn't flush the socket read/write buffer");
                Err(ConnectionManagerError::UnrecoverableError(format!("couldn't flush the socket read/write buffer. was the connection closed? error: {}", e)))
            }
        }
    }
}

impl CasetaConnectionManager {
    fn new() -> Self {
        // this should only handle a single disconnect message in the connection manager's lifetime,
        // so a buffer size of one message is sufficient.
        let (sender, receiver) = mpsc::channel(1);
        Self {
            connection: Option::None,
            disconnect_sender: sender,
            disconnect_receiver: receiver,
        }
    }

    async fn read_frame(&mut self) -> Result<Option<Message>, ConnectionManagerError> {
        let stream = match self.connection {
            Some(ref mut buf_writer) => buf_writer,
            None => {
                return Err(ConnectionManagerError::UnrecoverableError(
                    "connection is not initialized. this is a bug".to_string(),
                ))
            }
        };

        let mut buffer = BytesMut::with_capacity(128);
        let stream_read_future = stream.read_buf(&mut buffer);

        tokio::select! {
            disconnect_message = self.disconnect_receiver.recv() => {
                info!("The connection to the caseta hub is no longer alive");
                return Err(ConnectionManagerError::LivenessError)
            },
            read_result = stream_read_future => {
                let num_bytes_read = read_result.expect("there was a problem reading the buffer");
                if num_bytes_read == 0 {
                    if buffer.is_empty() {
                        return Ok(None)
                    } else {
                        // todo: not sure if this is appropriate here. maybe this is a
                        // recoverable error (aka one where we should just kill this connection
                        // and replace it with a new one)
                        return Err(ConnectionManagerError::EmptyMessageError);
                    }
                }

                let contents = match std::str::from_utf8(&buffer[..]) {
                    Ok(parsed_buffer) => parsed_buffer,
                    Err(e) => return Err(
                        ConnectionManagerError::UnrecoverableError(
                            format!("got unparsable contents, buffer bytes cannot be parsed from utf-8 into String: {}", e)
                        )
                    )
                };
                return match Message::from_str(contents) {
                    Ok(message) => Ok(Some(message)),
                    Err(e) => Err(ConnectionManagerError::UnrecoverableError(
                        format!("got an unparsable message. message from Caseta cannot be parsed into a Message object: {}", e)))
                }
            }
        }
    }

    async fn initialize(
        &mut self,
        caseta_username: String,
        caseta_password: String,
        caseta_host: Host<String>,
        caseta_port: u8,
        tcp_socket_provider: impl TcpSocketProvider,
    ) -> Result<(), ConnectionManagerError> {
        let tcp_stream = match tcp_socket_provider.new_socket().await {
            Ok(stream) => stream,
            Err(e) => {
                error!(
                    "encountered an error initializing the connection to the caseta hub, {}",
                    e
                );
                return Err(ConnectionManagerError::UnrecoverableError(format!(
                    "encountered an unrecoverable error initializing connection with caseta: {}",
                    e
                )));
            }
        };

        self.connection = Option::Some(BufWriter::new(tcp_stream));
        self.log_in(caseta_username, caseta_password).await
    }

    async fn log_in(
        &mut self,
        caseta_username: String,
        caseta_password: String,
    ) -> Result<(), ConnectionManagerError> {
        Self::ensure_expected_message(Message::LoginPrompt, self.read_frame().await)?;

        self.write_message(format!("{}\r\n", caseta_username))
            .await?;

        Self::ensure_expected_message(Message::PasswordPrompt, self.read_frame().await)?;
        self.write_message(format!("{}\r\n", caseta_password))
            .await?;
        Self::ensure_expected_message(Message::LoggedIn, self.read_frame().await)?;
        Ok(())
    }

    fn ensure_expected_message(
        expected_message: Message,
        actual_message: Result<Option<Message>, ConnectionManagerError>,
    ) -> Result<(), ConnectionManagerError> {
        match actual_message {
            Ok(Some(expected_message)) => {
                debug!("received the expected message: {}", expected_message);
                return Ok(());
            }
            Ok(Some(unexpected_message)) => {
                error!("got an unexpected message: {}", unexpected_message);
                return Err(ConnectionManagerError::UnrecoverableError(format!(
                    "unexpected message: {}",
                    unexpected_message
                )));
            }
            Ok(None) => {
                error!("got an empty message we did not expect");
                return Err(ConnectionManagerError::UnrecoverableError(
                    "unexpected empty message".to_string(),
                ));
            }
            Err(e) => {
                error!("got an error: {}", e);
                return Err(ConnectionManagerError::UnrecoverableError(format!(
                    "got an unexpected error: {}",
                    e
                )));
            }
        }
    }

    async fn write_keep_alive_message(&self) -> Result<(), ConnectionManagerError> {
        let write_result = self.write_message("\r\n".to_string()).await;
        return match write_result {
            Ok(_) => Ok(()),
            Err(e) => {
                info!(
                    "unable to write keep alive message to caseta. Is the connection closed? {}",
                    e
                );
                self.disconnect_sender.send(()).await.map_err(|send_error| {
                    ConnectionManagerError::UnrecoverableError(format!(
                        "unable to send disconnect message. original error: {}, send error: {}",
                        e, send_error
                    ))
                })
            }
        };
    }

    async fn socket_liveness_checker_loop(&self) -> () {
        loop {
            let mut interval = time::interval(Duration::from_secs(60));
            interval.tick().await;
            let result = self.write_keep_alive_message().await;
            match result {
                Ok(_) => {}
                Err(e) => {
                    error!("the connection manager disconnected, but we were unable to send a message for this disconnection. this is a bug. {}", e);
                    panic!("the connection manager disconnect, but we were unable to send a message for this disconnection. this is a bug. error: {}", e)
                }
            }
        }
    }
}

pub struct DelegatingCasetaConnectionManager {
    internal_connection_manager: Box<dyn ReadWriteConnection + Send + Sync>,
}

#[async_trait]
impl ReadWriteConnection for DelegatingCasetaConnectionManager {
    async fn await_message(&self) -> Result<Option<Message>, ConnectionManagerError> {
        todo!()
    }

    async fn write_message(&self, message: String) -> Result<(), ConnectionManagerError> {
        todo!()
    }
}

impl DelegatingCasetaConnectionManager {
    fn new() -> Self {
        Self {
            internal_connection_manager: Box::new(CasetaConnectionManager::new()),
        }
    }
}
