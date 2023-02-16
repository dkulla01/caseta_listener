use std::{str::FromStr, time::Duration};

use anyhow::bail;
use async_trait::async_trait;
use bytes::BytesMut;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::mpsc,
    time,
};
use tracing::{debug, error, info, instrument};
use url::Host;

use super::message::Message;

#[async_trait]
pub trait TcpSocketProvider: std::fmt::Debug {
    async fn new_socket(&self) -> Result<TcpStream, anyhow::Error>;
}

#[derive(Debug)]
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
            Ok(Err(e)) => bail!("unable to connect: {}", e),
            Err(_elapsed) => bail!("timed out trying to connect"),
        }
    }
}

#[async_trait]
pub trait ReadOnlyConnection: std::fmt::Debug {
    async fn await_message(&mut self) -> Result<Option<Message>, ConnectionManagerError>;
}
#[async_trait]
pub trait WriteOnlyConnection: std::fmt::Debug {
    async fn write_message(&mut self, message: String) -> Result<(), ConnectionManagerError>;
    async fn write_keep_alive_message(&mut self) -> Result<(), ConnectionManagerError>;
}

#[async_trait]
pub trait ReadWriteConnection: std::fmt::Debug {
    async fn await_message(&mut self) -> Result<Option<Message>, ConnectionManagerError>;
    async fn write_message(&mut self, message: String) -> Result<(), ConnectionManagerError>;
    async fn write_keep_alive_message(&mut self) -> Result<(), ConnectionManagerError>;
}

#[derive(Error, Debug)]
pub enum ConnectionManagerError {
    #[error("received an empty message")]
    EmptyMessageError,
    #[error("the caseta connection is no longer live")]
    LivenessError,
    #[error("encountered an error: {0}, the underlying connection should be refreshed.")]
    RecoverableError(String),
    #[error("encountered an unrecoverable error: {0}")]
    UnrecoverableError(String),
}

#[derive(Debug)]
pub struct CasetaReadConnectionManager {
    connection: OwnedReadHalf,
    disconnect_receiver: mpsc::Receiver<()>,
}

impl CasetaReadConnectionManager {
    fn new(connection: OwnedReadHalf, disconnect_receiver: mpsc::Receiver<()>) -> Self {
        Self {
            connection,
            disconnect_receiver,
        }
    }

    async fn read_frame(&mut self) -> Result<Option<Message>, ConnectionManagerError> {
        let mut buffer = BytesMut::with_capacity(128);
        let stream_read_future = self.connection.read_buf(&mut buffer);

        tokio::select! {
            _disconnect_message = self.disconnect_receiver.recv() => {
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
}

#[async_trait]
impl ReadOnlyConnection for CasetaReadConnectionManager {
    async fn await_message(&mut self) -> Result<Option<Message>, ConnectionManagerError> {
        self.read_frame().await
    }
}

#[derive(Debug)]
pub struct CasetaWriteConnectionManager {
    connection: OwnedWriteHalf,
    disconnect_sender: mpsc::Sender<()>,
}

impl CasetaWriteConnectionManager {
    fn new(connection: OwnedWriteHalf, disconnect_sender: mpsc::Sender<()>) -> Self {
        Self {
            connection,
            disconnect_sender,
        }
    }
}

#[async_trait]
impl WriteOnlyConnection for CasetaWriteConnectionManager {
    async fn write_message(&mut self, message: String) -> Result<(), ConnectionManagerError> {
        let outcome = self.connection.write(message.as_bytes()).await;

        match outcome {
            Ok(_) => {}
            Err(e) => {
                error!(error=%e, "couldn't write the socket read/write buffer");
                return Err(ConnectionManagerError::UnrecoverableError(format!("unable to write the socket read/write buffer. was the connection closed?, error: {}", e)));
            }
        }

        let outcome = self.connection.flush().await;
        match outcome {
            Ok(_) => Ok(()),
            Err(e) => {
                error!(error=%e, "couldn't flush the socket read/write buffer");
                Err(ConnectionManagerError::UnrecoverableError(format!("couldn't flush the socket read/write buffer. was the connection closed? error: {}", e)))
            }
        }
    }

    #[instrument(level = "debug")]
    async fn write_keep_alive_message(&mut self) -> Result<(), ConnectionManagerError> {
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
}

#[derive(Debug)]
pub struct CasetaConnectionManager {
    connection: Option<(CasetaReadConnectionManager, CasetaWriteConnectionManager)>,
}

impl CasetaConnectionManager {
    fn new() -> Self {
        Self {
            connection: Option::None,
        }
    }

    #[instrument(level = "debug", skip(caseta_username, caseta_password))]
    async fn initialize(
        &mut self,
        caseta_username: &str,
        caseta_password: &str,
        tcp_stream: TcpStream,
    ) -> Result<(), ConnectionManagerError> {
        // this should only handle a single disconnect message in the connection manager's lifetime,
        // so a buffer size of one message is sufficient.
        let (sender, receiver) = mpsc::channel(1);
        let (tcp_read_half, tcp_write_half) = TcpStream::into_split(tcp_stream);
        let mut caseta_read_half = CasetaReadConnectionManager::new(tcp_read_half, receiver);
        let mut caseta_write_half = CasetaWriteConnectionManager::new(tcp_write_half, sender);

        let login_response = Self::log_in(
            caseta_username,
            caseta_password,
            &mut caseta_read_half,
            &mut caseta_write_half,
        )
        .await;

        if let Err(e) = login_response {
            return Err(ConnectionManagerError::UnrecoverableError(format!(
                "there was a problem authenticating with the caseta hub: {}",
                e
            )));
        }
        self.connection = Option::Some((caseta_read_half, caseta_write_half));

        Ok(())
    }

    fn split(
        self,
    ) -> Result<
        (
            Box<dyn ReadOnlyConnection + Send + Sync>,
            Box<dyn WriteOnlyConnection + Send + Sync>,
        ),
        ConnectionManagerError,
    > {
        return match self.connection {
            Some((read_half, write_half)) => Ok((Box::new(read_half), Box::new(write_half))),
            None => {
                Err(ConnectionManagerError::UnrecoverableError("This is a bug; you cannot split a ReadWriteConnection before initializing it. did you call the initialize method?".to_string()))
            }
        };
    }
    #[instrument(level = "debug", skip(caseta_username, caseta_password))]
    async fn log_in(
        caseta_username: &str,
        caseta_password: &str,
        caseta_read_half: &mut CasetaReadConnectionManager,
        caseta_write_half: &mut CasetaWriteConnectionManager,
    ) -> Result<(), ConnectionManagerError> {
        Self::ensure_expected_message(Message::LoginPrompt, caseta_read_half.read_frame().await)?;
        debug!("writing caseta username");
        caseta_write_half
            .write_message(format!("{}\r\n", caseta_username))
            .await?;

        Self::ensure_expected_message(
            Message::PasswordPrompt,
            caseta_read_half.read_frame().await,
        )?;

        debug!("writing caseta password");
        caseta_write_half
            .write_message(format!("{}\r\n", caseta_password))
            .await?;
        Self::ensure_expected_message(Message::LoggedIn, caseta_read_half.read_frame().await)?;
        Ok(())
    }

    fn ensure_expected_message(
        expected_message: Message,
        actual_message: Result<Option<Message>, ConnectionManagerError>,
    ) -> Result<(), ConnectionManagerError> {
        match actual_message {
            Ok(Some(message)) => {
                if message == expected_message {
                    debug!("received the expected message: {}", expected_message);
                    return Ok(());
                }
                error!("got an unexpected message: {}", message);
                return Err(ConnectionManagerError::UnrecoverableError(format!(
                    "unexpected message: {}",
                    message
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
}

#[async_trait]
pub trait CasetaConnectionProvider: std::fmt::Debug {
    async fn new_connection(
        &mut self,
    ) -> Result<
        (
            Box<dyn ReadOnlyConnection + Send + Sync>,
            Box<dyn WriteOnlyConnection + Send + Sync>,
        ),
        ConnectionManagerError,
    >;
}

#[derive(Debug)]
pub struct DefaultCasetaConnectionProvider {
    username: String,
    password: String,
    tcp_socket_provider: Box<dyn TcpSocketProvider + Send + Sync>,
}

impl DefaultCasetaConnectionProvider {
    pub fn new(
        username: String,
        password: String,
        tcp_socket_provider: Box<dyn TcpSocketProvider + Send + Sync>,
    ) -> Self {
        Self {
            username,
            password,
            tcp_socket_provider,
        }
    }
}

#[async_trait]
impl CasetaConnectionProvider for DefaultCasetaConnectionProvider {
    async fn new_connection(
        &mut self,
    ) -> Result<
        (
            Box<dyn ReadOnlyConnection + Send + Sync>,
            Box<dyn WriteOnlyConnection + Send + Sync>,
        ),
        ConnectionManagerError,
    > {
        let mut connection = CasetaConnectionManager::new();
        let tcp_stream = self.tcp_socket_provider.new_socket().await;

        if let Err(e) = tcp_stream {
            return Err(ConnectionManagerError::UnrecoverableError(format!(
                "there was a problem getting a tcp connection to the caseta hub: {}",
                e
            )));
        }

        connection
            .initialize(&self.username, &self.password, tcp_stream.unwrap())
            .await?;

        connection.split()
    }
}

#[derive(Debug)]
pub struct DelegatingCasetaConnectionManager {
    connection_manager: Option<(
        Box<dyn ReadOnlyConnection + Send + Sync>,
        Box<dyn WriteOnlyConnection + Send + Sync>,
    )>,
    caseta_connection_provider: Box<dyn CasetaConnectionProvider + Send + Sync>,
}

#[async_trait]
impl ReadOnlyConnection for DelegatingCasetaConnectionManager {
    #[instrument(level = "debug", skip(self))]
    async fn await_message(&mut self) -> Result<Option<Message>, ConnectionManagerError> {
        match self.connection_manager {
            Some(_) => {}
            None => {
                debug!("no delegate caseta connection present. creating a new caseta connection");
                let new_connection = self.caseta_connection_provider.new_connection().await;
                if let Err(e) = new_connection {
                    return Err(ConnectionManagerError::UnrecoverableError(format!(
                        "there was a problem creating a new caseta connection: {}",
                        e
                    )));
                }
                let new_connection = new_connection.unwrap();
                self.connection_manager = Option::Some(new_connection);
            }
        };
        loop {
            let connection_manager = self.connection_manager.as_mut().unwrap();
            let (read_connection, write_connection) = connection_manager;

            tokio::select! {
                next_message = read_connection.await_message() => {
                    match next_message {
                        Ok(Some(Message::LoggedIn)) => {
                            debug!("got the logged in prompt: {}. in this case, it's a response from the keep alive message", Message::LoggedIn);
                            continue;
                        },
                        Ok(Some(message)) => return Ok(Some(message)),
                        Ok(None) | Err(ConnectionManagerError::LivenessError) => {
                            // liveness errors mean the delegate connection is dead. drop the delegate connection so we can replace it.
                            info!("the existing caseta connection is no longer valid. Replacing it with an empty option to trigger reconnection");
                            self.connection_manager = Option::None;
                            continue;
                        }
                        Err(e) => return Err(ConnectionManagerError::UnrecoverableError(format!(
                            "encountered an unexpected and unrecoverable error: {}",
                            e
                        ))),
                    };
                },
                keep_alive_result = async {
                    time::sleep(Duration::from_secs(60)).await;
                    debug!("writing keep alive message");
                    write_connection.write_keep_alive_message().await
                } => {
                    match keep_alive_result {
                        Ok(_) => continue,
                        Err(e) => return Err(ConnectionManagerError::UnrecoverableError(format!("unable to write the keepalive message: {}", e)))
                    }
                }

            };
        }
    }
}

impl DelegatingCasetaConnectionManager {
    pub fn new(
        caseta_connection_provider: Box<dyn CasetaConnectionProvider + Send + Sync>,
    ) -> Self {
        Self {
            connection_manager: Option::None,
            caseta_connection_provider: caseta_connection_provider,
        }
    }
}
