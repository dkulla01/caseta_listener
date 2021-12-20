use std::io;

use bytes::BytesMut;
use tokio::io::{ AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use std::sync::{Mutex, Arc};

type Result<T> = std::result::Result<T, String>;

struct CasetaConnection {
    stream: BufWriter<TcpStream>,
    logged_in: Arc<Mutex<bool>>
}

impl CasetaConnection {
    fn new(socket: TcpStream) -> CasetaConnection {
        CasetaConnection {
            stream: BufWriter::new(socket),
            logged_in: Arc::new(Mutex::new(false))
        }
    }

    async fn read_frame(&mut self) -> Result<Option<String>> {

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
        Ok(Some(String::from(contents)))
    }

    async fn write(&mut self, message: &str) -> Result<()> {
        let outcome = self.stream.write(message.as_bytes())
            .await;

        if outcome.is_err() {
            return Err(String::from("couldn't write the buffer"))
        }
        self.stream.flush().await.expect("couldn't flush the buffer");
        Ok(())
    }

    async fn log_in(&mut self) -> Result<()> {
        let mutex = self.logged_in.clone();
        let mut is_logged_in = mutex.lock().unwrap();
        if *is_logged_in {
            return Ok(());
        }

        self.write("lutron\r\n").await.expect("uh oh....");
        let contents = self.read_frame().await.expect("something weird happened");

        if contents.is_some() {
            println!("got contents: {}", contents.unwrap());
        }
        self.write("integration\r\n").await.expect("again, uh oh");
        let contents = self.read_frame().await.expect("something weird, again");


        match contents {
            Some(val) => {
                if val.starts_with("GNET>") {
                    println!("got contents: {}", val);
                    *is_logged_in = true;
                    return Ok(())
                }
                Err(format!("there was a problem logging in. got `{}` instead of GNET>", val))
            }
            _ => Err(String::from("there was a problem logging in"))
        }

    }

    async fn write_keep_alive_message(&mut self) -> Result<()>{
        self.write("\r\n").await
    }

}

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("192.168.86.144:23").await?;

    let mut connection = CasetaConnection::new(stream);

    let contents = connection.read_frame().await.expect("something weird happened");

    match contents {
        Some(val) => {
            if val.starts_with("login:") {
                println!("starting the login sequence")
            } else {
                panic!("expected login prompt but got {}", val);
            }
        }
        None => {
            panic!("expected to read the login prompt but got nothing");
        }
    }

    connection.log_in().await.expect("unable to log in");

    loop {
        let contents = connection.read_frame().await.expect("something weird, again");
        if contents.is_some() {
            println!("got contents: {}", contents.unwrap());
        }
    }

    Ok(())
}