use tokio::io::{BufWriter, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::sync::{Arc, Mutex};
use bytes::BytesMut;
use crate::casetta::message::Message;
use std::str::FromStr;

pub struct CasetaConnection {
    stream: BufWriter<TcpStream>,
    logged_in: Arc<Mutex<bool>>
}

impl CasetaConnection {
    pub fn new(socket: TcpStream) -> CasetaConnection {
        CasetaConnection {
            stream: BufWriter::new(socket),
            logged_in: Arc::new(Mutex::new(false))
        }
    }

    pub async fn read_frame(&mut self) -> Result<Option<Message>, String> {

        let mut buffer = BytesMut::with_capacity(128);
        let num_bytes_read = self.stream.read_buf(&mut buffer).await.expect("uh oh, there was a problem");
        if num_bytes_read == 0 {
            if buffer.is_empty() {
                return Ok(None)
            } else {
                return Err("uh oh".into());
            }
        }
        let contents = std::str::from_utf8(&buffer[..]).expect("got unparseable content");
        let message = Message::from_str(contents).expect(format!("expected a valid message but got {}", contents).as_str());
        Ok(Some(message))
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

    pub async fn log_in(&mut self) -> Result<(), String> {
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
                Err(format!("there was a problem logging in. got `{}` instead of GNET>", message))
            }
            _ => Err("there was a problem logging in".into())
        }

    }

    async fn write_keep_alive_message(&mut self) -> Result<(), String>{
        self.write("\r\n").await
    }

}