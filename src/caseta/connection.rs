use std::net::IpAddr;
use tokio::io::{BufWriter, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::sync::{Arc, Mutex};
use bytes::BytesMut;
use crate::caseta::message::Message;
use std::str::FromStr;
use anyhow::{anyhow, Context, Result};
use thiserror::Error;


#[derive(Error, Debug)]
enum CasetaConnectionError {

    #[error("unable to connect to address {0}")]
    BadAddress(String),
    #[error("there was a problem authenticating with the caseta hub")]
    Authentication,
    #[error("got an empty message when we expected a message with content")]
    EmptyMessage,
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

        let tcp_stream = TcpStream::connect((self.address, self.port))
            .await
            .with_context(|| CasetaConnectionError::BadAddress(format!("{}:{}", self.address, self.port)))?;

        let mut internal_caseta_connection = InternalCasetaConnection::new(tcp_stream);
        internal_caseta_connection.log_in().await?;

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
    stream: BufWriter<TcpStream>,
    logged_in: Arc<Mutex<bool>>
}

impl InternalCasetaConnection {
    pub fn new(socket: TcpStream) -> InternalCasetaConnection {
        InternalCasetaConnection {
            stream: BufWriter::new(socket),
            logged_in: Arc::new(Mutex::new(false))
        }
    }

    pub async fn read_frame(&mut self) -> Result<Option<Message>, CasetaConnectionError> {

        let mut buffer = BytesMut::with_capacity(128);
        let num_bytes_read = self.stream.read_buf(&mut buffer).await.expect("uh oh, there was a problem");
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

    pub async fn await_message(&mut self) -> Result<Message, CasetaConnectionError> {
        let message = self.read_frame().await;
        match message {
            Ok(Some(content)) => Ok(content),
            Ok(None) => Err(CasetaConnectionError::EmptyMessage),
            Err(err) => Err(err)
        }
    }

    pub async fn write(&mut self, message: &str) -> Result<(), String> {
        let outcome = self.stream.write(message.as_bytes())
            .await;

        if outcome.is_err() {
            return Err("couldn't write the buffer".into())
        }
        self.stream.flush().await.expect("couldn't flush the buffer");
        Ok(())
    }

    pub async fn log_in(&mut self) -> Result<(), CasetaConnectionError> {
        let mutex = self.logged_in.clone();
        let mut is_logged_in = mutex.lock().unwrap();
        if *is_logged_in {
            return Ok(());
        }

        self.write("lutron\r\n").await.expect("uh oh....");
        let contents = self.read_frame().await.expect("something weird happened");

        if let Some(value) = contents {
            println!("got contents: {}", value);
        }
        self.write("integration\r\n").await.expect("again, uh oh");
        let contents = self.read_frame().await.expect("something weird, again");


        match contents {
            Some(message) => {
                if let Message::LoggedIn = message {
                    println!("got contents: {}", message);
                    *is_logged_in = true;
                    return Ok(())
                }
                println!("expected GNET> message, but got {}", message);
                Err(CasetaConnectionError::Authentication)
            }
            _ => Err(CasetaConnectionError::Authentication)
        }

    }

    async fn write_keep_alive_message(&mut self) -> Result<(), String>{
        self.write("\r\n").await
    }

}
