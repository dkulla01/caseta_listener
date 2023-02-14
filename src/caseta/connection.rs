use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::bail;
use async_trait::async_trait;
use bytes::BytesMut;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::mpsc,
    time,
};
use tracing::{debug, error, info};
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

#[async_trait]
pub trait ReadOnlyConnection {
    async fn await_message(&mut self) -> Result<Option<Message>, ConnectionManagerError>;
}
#[async_trait]
pub trait WriteOnlyConnection {
    async fn write_message(&mut self, message: String) -> Result<(), ConnectionManagerError>;
    async fn write_keep_alive_message(&mut self) -> Result<(), ConnectionManagerError>;
}

#[async_trait]
pub trait ReadWriteConnection {
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

pub struct CasetaConnectionManager {
    connection: Option<(CasetaReadConnectionManager, CasetaWriteConnectionManager)>,
}

impl CasetaConnectionManager {
    fn new() -> Self {
        Self {
            connection: Option::None,
        }
    }

    async fn initialize(
        &mut self,
        caseta_username: &str,
        caseta_password: &str,
        caseta_host: &Host<String>,
        caseta_port: u16,
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

    async fn log_in(
        caseta_username: &str,
        caseta_password: &str,
        caseta_read_half: &mut CasetaReadConnectionManager,
        caseta_write_half: &mut CasetaWriteConnectionManager,
    ) -> Result<(), ConnectionManagerError> {
        Self::ensure_expected_message(Message::LoginPrompt, caseta_read_half.read_frame().await)?;

        caseta_write_half
            .write_message(format!("{}\r\n", caseta_username))
            .await?;

        Self::ensure_expected_message(
            Message::PasswordPrompt,
            caseta_read_half.read_frame().await,
        )?;
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
                if (message == expected_message) {
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

    // async fn socket_liveness_checker_loop(&mut self) -> () {
    //     loop {
    //         let mut interval = time::interval(Duration::from_secs(60));
    //         interval.tick().await;
    //         let result = self.write_keep_alive_message().await;
    //         match result {
    //             Ok(_) => {}
    //             Err(e) => {
    //                 error!("the connection manager disconnected, but we were unable to send a message for this disconnection. this is a bug. {}", e);
    //                 panic!("the connection manager disconnect, but we were unable to send a message for this disconnection. this is a bug. error: {}", e)
    //             }
    //         }
    //     }
    // }
}

#[async_trait]
pub trait CasetaConnectionProvider {
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

pub struct DefaultCasetaConnectionProvider {
    host: Host<String>,
    port: u16,
    username: String,
    password: String,
    tcp_socket_provider: Box<dyn TcpSocketProvider + Send + Sync>,
}

impl DefaultCasetaConnectionProvider {
    pub fn new(
        host: Host<String>,
        port: u16,
        username: String,
        password: String,
        tcp_socket_provider: Box<dyn TcpSocketProvider + Send + Sync>,
    ) -> Self {
        Self {
            host,
            port,
            username,
            password,
            tcp_socket_provider,
        }
    }

    async fn socket_liveness_checker_loop(
        mut write_connection: Box<dyn WriteOnlyConnection>,
    ) -> () {
        loop {
            let mut interval = time::interval(Duration::from_secs(60));
            interval.tick().await;
            let result = write_connection.write_keep_alive_message().await;
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
            .initialize(
                &self.username,
                &self.password,
                &self.host,
                self.port,
                tcp_stream.unwrap(),
            )
            .await?;

        connection.split()
    }
}

pub struct DelegatingCasetaConnectionManager {
    read_connection_manager: Option<Box<dyn ReadOnlyConnection + Send + Sync>>,
    write_connection_manager: Option<Box<dyn WriteOnlyConnection + Send + Sync>>,
    caseta_connection_provider: Box<dyn CasetaConnectionProvider + Send + Sync>,
}

#[async_trait]
impl ReadOnlyConnection for DelegatingCasetaConnectionManager {
    async fn await_message(&mut self) -> Result<Option<Message>, ConnectionManagerError> {
        match self.read_connection_manager {
            Some(_) => {}
            None => {
                let new_connection = self.caseta_connection_provider.new_connection().await;
                if let Err(e) = new_connection {
                    return Err(ConnectionManagerError::UnrecoverableError(format!(
                        "there was a problem creating a new caseta connection: {}",
                        e
                    )));
                }
                let (read_connection, mut write_connection) = new_connection.unwrap();
                self.read_connection_manager = Option::Some(read_connection);
                tokio::spawn(async move {
                    Self::socket_liveness_checker_loop(write_connection).await;
                });
            }
        }

        let read_connection_manager = self.read_connection_manager.as_mut().expect("we just made sure that we had a nonempty connection above. this cannot happen/is a bug");
        let next_message = read_connection_manager.await_message().await;
        return match next_message {
            Ok(Some(message)) => Ok(Some(message)),
            Ok(None) | Err(ConnectionManagerError::LivenessError) => {
                // swap in a new connection here
                //return a liveness error
                self.read_connection_manager = Option::None;
                Err(ConnectionManagerError::LivenessError)
            }
            Err(e) => Err(ConnectionManagerError::UnrecoverableError(format!(
                "encountered an unexpected and unrecoverable error: {}",
                e
            ))),
        };
    }
}

impl DelegatingCasetaConnectionManager {
    pub fn new(
        caseta_connection_provider: Box<dyn CasetaConnectionProvider + Send + Sync>,
    ) -> Self {
        Self {
            read_connection_manager: Option::None,
            write_connection_manager: Option::None,
            caseta_connection_provider: caseta_connection_provider,
        }
    }

    async fn socket_liveness_checker_loop(
        mut write_connection: Box<dyn WriteOnlyConnection + Sync + Send>,
    ) -> () {
        loop {
            let mut interval = time::interval(Duration::from_secs(60));
            interval.tick().await;
            // let mut write_connection_mutex = write_connection.lock().unwrap();
            // let write_connection = match write_connection_mutex.as_mut() {
            //     Some(connection) => connection,
            //     None => return,
            // };
            let result = write_connection.write_keep_alive_message().await;
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
