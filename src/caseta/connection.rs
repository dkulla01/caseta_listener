use std::net::IpAddr;
use tokio::io::{BufWriter, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::sync::{Arc, Mutex};
use bytes::BytesMut;
use crate::caseta::message::Message;
use std::str::FromStr;
use anyhow::{anyhow, Result};
use thiserror::Error;


#[derive(Error, Debug)]
enum CasetaConnectionError {

    #[error("unable to connect to address {0}")]
    BadAddress(String),
    #[error("there was a problem authenticating with the caseta hub")]
    Authentication,
    #[error("got an empty message when we expected a message with content")]
    EmptyMessage,
    #[error("encountered an error initializing connection to the caseta hub")]
    Initialization,
    #[error("this connection has not been initialized yet")]
    Uninitialized,
    #[error("unknown caseta connection error")]
    Unknown(String)

}

pub struct CasetaConnection {
    address: IpAddr,
    port: u16,
    internal_caseta_connection: Option<InternalCasetaConnection>
}

impl CasetaConnection {
    pub fn new(address: IpAddr, port: u16) -> CasetaConnection {
        CasetaConnection {
            address,
            port,
            internal_caseta_connection: Option::None
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        if let Some(_) = self.internal_caseta_connection {
            return Ok(())
        }

        let mut internal_caseta_connection = InternalCasetaConnection::new(self.address, self.port);
        internal_caseta_connection.initialize().await?;
        self.internal_caseta_connection = Some(internal_caseta_connection);

        // start keep-alive message writer

        return Ok(())
    }

    pub async fn await_message(&mut self) -> Result<Message> {
        match self.internal_caseta_connection {
            Some(ref mut internal_connection) => {
                match internal_connection.await_message().await {
                    Ok(message) => Ok(message),
                    Err(caseta_connection_err) => {
                        Err(anyhow!(caseta_connection_err))
                    }
                }
            }
            None => Err(anyhow!("the caseta connection isn't initialized yet"))
        }
    }
}

struct InternalCasetaConnection {
    address: IpAddr,
    port: u16,
    stream: Option<BufWriter<TcpStream>>,
    logged_in: Arc<Mutex<bool>>
}

impl InternalCasetaConnection {
    fn new(address: IpAddr, port: u16) -> InternalCasetaConnection {
        InternalCasetaConnection {
            address,
            port,
            stream: Option::None,
            logged_in: Arc::new(Mutex::new(false))
        }
    }

    async fn initialize(&mut self) -> Result<(), CasetaConnectionError> {
        let tcp_stream = TcpStream::connect((self.address, self.port))
            .await;

        match tcp_stream {
            Ok(stream) => self.stream = Option::Some(BufWriter::new(stream)),
            Err(e) => {
                // print the error
                return Err(CasetaConnectionError::BadAddress(format!("{}:{}", self.address, self.port)));
            }
        }
     self.log_in().await
    }

    async fn read_frame(&mut self) -> Result<Option<Message>, CasetaConnectionError> {
        let stream = match self.stream {
            Some(ref mut buf_writer) => buf_writer,
            None => return Err(CasetaConnectionError::Uninitialized)
        };

        let mut buffer = BytesMut::with_capacity(128);
        let num_bytes_read = stream.read_buf(&mut buffer).await.expect("uh oh, there was a problem");
        if num_bytes_read == 0 {
            if buffer.is_empty() {
                return Ok(None)
            } else {
                return Err(CasetaConnectionError::EmptyMessage);
            }
        }
        let contents = std::str::from_utf8(&buffer[..]).expect("got unparseable content");
        let message = Message::from_str(contents).expect(format!("expected a valid message but got {}", contents).as_str());
        Ok(Some(message))
    }

    async fn await_message(&mut self) -> Result<Message, CasetaConnectionError> {
        let message = self.read_frame().await;
        match message {
            Ok(Some(content)) => Ok(content),
            Ok(None) => Err(CasetaConnectionError::EmptyMessage),
            Err(err) => Err(err)
        }
    }

    async fn write(&mut self, message: &str) -> Result<(), CasetaConnectionError> {
        let stream = match self.stream {
            Some(ref mut buf_writer) => buf_writer,
            None => return Err(CasetaConnectionError::Uninitialized)
        };
        let outcome = stream.write(message.as_bytes())
            .await;

        match outcome {
            Ok(_) => {},
            Err(e) => return Err(CasetaConnectionError::Unknown(format!("got an unknown error: {}", e)))
        }

        stream.flush().await.expect("couldn't flush the buffer");
        Ok(())
    }

    async fn log_in(&mut self) -> Result<(), CasetaConnectionError> {
        let mutex = self.logged_in.clone();
        let is_logged_in = mutex.lock().unwrap();
        if *is_logged_in {
            return Ok(());
        }

        let contents = self.read_frame().await;
        match contents {
            Ok(Some(Message::LoginPrompt)) => println!("got login prompt"),
            _ => {
                // todo: add more match arms and log the appropriate errors here
                return Err(CasetaConnectionError::Initialization)
            }
        }
        self.write("lutron\r\n").await.expect("uh oh....");
        let contents = self.read_frame().await;
        match contents {
            Ok(Some(Message::PasswordPrompt)) => println!("got password prompt"),
            _ => {
                // todo: add more match arms and log the appropriate errors here
                return Err(CasetaConnectionError::Authentication)
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
                return Ok(());
            },
            Ok(Some(other_message)) => {
                println!("expected GNET> message, but got {}", other_message);
                Err(CasetaConnectionError::Authentication)
            }
            _ => Err(CasetaConnectionError::Authentication)
        }
    }

    async fn write_keep_alive_message(&mut self) -> Result<(), String>{
        self.write("\r\n").await
    }

}
