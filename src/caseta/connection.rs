use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::io;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use bytes::BytesMut;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc;
use async_trait::async_trait;
use tracing::instrument;

use crate::caseta::message::Message;

#[derive(Error, Debug)]
pub enum CasetaConnectionError {

    #[error("unable to connect to address")]
    BadAddress,
    #[error("there was a problem authenticating with the caseta hub")]
    Authentication,
    #[error("the connection to the caseta hub was disconnected")]
    Disconnected,
    #[error("got an empty message when we expected a message with content")]
    EmptyMessage,
    #[error("encountered an error initializing connection to the caseta hub")]
    Initialization,
    #[error("this connection has not been initialized yet")]
    Uninitialized,
    #[error("encountered a problem reading/writing messages with caseta")]
    ReadWriteIo(#[from] io::Error),
    #[error("encountered a problem writing the keepalive message")]
    KeepAlive,
    #[error("Encountered an unknown error: {0}")]
    Unknown(String)

}

#[derive(Debug)]
struct DisconnectCommand {
    message: String,
    cause: CasetaConnectionError
}

#[async_trait]
pub trait TcpSocketProvider {
    async fn new_socket(&self) -> io::Result<TcpStream>;
}

pub struct DefaultTcpSocketProvider {
    address: IpAddr,
    port: u16
}

impl DefaultTcpSocketProvider {
    pub fn new(address: IpAddr, port: u16) -> Self {
        DefaultTcpSocketProvider{ address, port}
    }
}

#[async_trait]
impl TcpSocketProvider for DefaultTcpSocketProvider {
    async fn new_socket(&self) -> io::Result<TcpStream> {
        Ok(TcpStream::connect((self.address, self.port)).await?)
    }
}

pub struct CasetaConnection<'a> {
    tcp_socket_provider: &'a (dyn TcpSocketProvider + 'a),
    stream: Option<BufWriter<TcpStream>>,
    logged_in: bool,
    disconnect_sender: mpsc::Sender<DisconnectCommand>,
    disconnect_receiver: mpsc::Receiver<DisconnectCommand>
}

impl Debug for CasetaConnection<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CasetaConnection").finish()
    }
}

impl <'a>  CasetaConnection<'a> {
    pub fn new(tcp_socket_provider: &'a dyn TcpSocketProvider) -> CasetaConnection<'a> {
        let (disconnect_sender, mut disconnect_receiver) = mpsc::channel(64);
        CasetaConnection {
            tcp_socket_provider,
            stream: Option::None,
            logged_in: false,
            disconnect_sender,
            disconnect_receiver
        }
    }

    async fn read_frame(&mut self) -> Result<Option<Message>, CasetaConnectionError> {
        let stream = match self.stream {
            Some(ref mut buf_writer) => buf_writer,
            None => return Err(CasetaConnectionError::Uninitialized)
        };

        let mut buffer = BytesMut::with_capacity(128);
        let ref mut disconnect_recv = self.disconnect_receiver;

        let stream_read_future = stream.read_buf(&mut buffer);
        let disconnect_recv_future = disconnect_recv.recv();
        tokio::select! {
            read_result = stream_read_future => {
                let num_bytes_read = read_result.expect("there was a problem reading the buffer");
                if num_bytes_read == 0 {
                    if buffer.is_empty() {
                        return Ok(None)
                    } else {
                        return Err(CasetaConnectionError::EmptyMessage);
                    }
                }
                let contents = std::str::from_utf8(&buffer[..]).expect("got unparseable content");
                let message = Message::from_str(contents).expect(format!("expected a valid message but got {}", contents).as_str());
                return Ok(Some(message))
            }
            disconnect_command = disconnect_recv_future => {
                match disconnect_command {
                    Some(DisconnectCommand{cause, message}) => {
                        println!("encountered an error communicating with the caseta hub: {}", message);
                        Err(cause)
                    }
                    _ => Err(CasetaConnectionError::Unknown("there was an issue waiting on the the disconnect channel".into()))
                }
            }
        }
    }

    #[instrument]
    async fn log_in(&mut self) -> Result<(), CasetaConnectionError> {
        if self.logged_in {
            return Ok(());
        }

        let contents = self.read_frame().await;
        match contents {
            Ok(Some(Message::LoginPrompt)) => tracing::trace!("received the login prompt"),
            Ok(Some(unexpected_message)) => {
                tracing::error!("got a weird random message: {:?}", unexpected_message);
                return Err(CasetaConnectionError::Initialization)
            }
            Ok(None) => {
                tracing::error!("got an empty message");
                return Err(CasetaConnectionError::Initialization);
            }
            Err(e) => {
                tracing::error!("got an error: {:?}", e);
            }
        }
        self.write("lutron\r\n").await?;
        let contents = self.read_frame().await;
        match contents {
            Ok(Some(Message::PasswordPrompt)) => tracing::trace!("got password prompt"),
            Ok(Some(unexpected_message)) => {
                tracing::error!("got a weird random message: {:?}", unexpected_message);
                return Err(CasetaConnectionError::Authentication);
            }
            Ok(None) => {
                tracing::error!("got an empty message");
                return Err(CasetaConnectionError::Authentication);
            }
            Err(e) => {
                tracing::error!("got an error: {:?}", e);
            }
        }
        if let Ok(()) = self.write("integration\r\n").await {
        } else {
            println!("got an error logging in");
            return Err(CasetaConnectionError::Authentication)
        }

        let contents = self.read_frame().await;


        match contents {
            Ok(Some(Message::LoggedIn)) => {
                self.logged_in = true;
                return Ok(());
            },
            Ok(Some(other_message)) => {
                println!("expected GNET> message, but got {}", other_message);
                Err(CasetaConnectionError::Authentication)
            }
            _ => Err(CasetaConnectionError::Authentication)
        }
    }

    async fn write_keep_alive_message(&mut self) -> Result<(), CasetaConnectionError> {
        let write_result = self.write("\r\n").await;
        match write_result {
            Ok(_) => Ok(()),
            Err(e) => {
                // println!("there was an issue writing the keepalive message: {}", e);
                self.disconnect_sender.send(
                    DisconnectCommand{
                        message: "there was a problem writing the keepalive message".into(),
                        cause: CasetaConnectionError::KeepAlive
                    }
                ).await
                .expect("unrecoverable error");
                Ok(())
            }
        }
    }

    pub async fn initialize(&mut self) -> Result<(), CasetaConnectionError> {
        let tcp_stream = self.tcp_socket_provider.new_socket()
            .await;

        match tcp_stream {
            Ok(stream) => self.stream = Option::Some(BufWriter::new(stream)),
            Err(e) => {
                // print the error
                return Err(CasetaConnectionError::BadAddress);
            }
        }
        self.log_in().await
    }


    pub async fn await_message(&mut self) -> Result<Message, CasetaConnectionError> {
        let message = self.read_frame().await;
        match message {
            Ok(Some(content)) => Ok(content),
            Ok(None) => {
                Err(CasetaConnectionError::Disconnected)
            }
            Err(CasetaConnectionError::KeepAlive) => {
                println!("there was an issue writing the keepalive message...");
                Err(CasetaConnectionError::Disconnected)
            }
            Err(CasetaConnectionError::EmptyMessage) => {
                Err(CasetaConnectionError::Disconnected)
            }
            Err(err) => Err(err)
        }
    }

    pub async fn write(&mut self, message: &str) -> Result<(), CasetaConnectionError> {
        let stream = match self.stream {
            Some(ref mut buf_writer) => buf_writer,
            None => return Err(CasetaConnectionError::Uninitialized)
        };
        let outcome = stream.write(message.as_bytes())
            .await;

        match outcome {
            Ok(_) => {},
            Err(e) => return Err(CasetaConnectionError::ReadWriteIo(e))
        }

        let outcome = stream.flush().await;
        match outcome {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("couldn't flush the buffer");
                Err(CasetaConnectionError::ReadWriteIo(e))
            }
        }
    }
}
