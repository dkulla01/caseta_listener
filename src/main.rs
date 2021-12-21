use std::io;

use bytes::BytesMut;
use tokio::io::{ AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use std::sync::{Mutex, Arc};
use std::str::FromStr;
use std::fmt::Display;

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

    async fn read_frame(&mut self) -> Result<Option<Message>> {

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

    async fn write(&mut self, message: &str) -> Result<()> {
        let outcome = self.stream.write(message.as_bytes())
            .await;

        if outcome.is_err() {
            return Err("couldn't write the buffer".into())
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

    async fn write_keep_alive_message(&mut self) -> Result<()>{
        self.write("\r\n").await
    }

}
#[derive(Debug)]
enum Message {
    ButtonDown { remote_id: u8, button_id: u8 },
    ButtonUp { remote_id: u8, button_id: u8 },
    LoggedIn,
    LoginPrompt,
    PasswordPrompt
}

impl FromStr for Message {

    type Err = String;

    fn from_str(s : &str) -> std::result::Result<Self, Self::Err> {
        if s.starts_with("login: ") {
            return Ok(Message::LoginPrompt);
        } else if s.starts_with("password: ") {
            return Ok(Message::PasswordPrompt);
        } else if s.starts_with("GNET>") {
            return Ok(Message::LoggedIn);
        } else if s.starts_with("~DEVICE") {
            let parts : Vec<&str> = s.split(",").collect();
            let remote_id: u8 = parts[1].parse().expect("only integer values are allowed");
            let button_id: u8 = parts[2].parse().expect("only integer values are allowed");
            let button_action_value : u8 = parts[3].parse().expect("only integers are allowed");
            return match button_action_value {
                3 => Ok(Message::ButtonDown {remote_id, button_id}),
                4 => Ok(Message::ButtonUp {remote_id, button_id}),
                _ => Err("this is not a thing".into())
            };
        }

        Err("this is also not a thing".into())
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Message::LoginPrompt => write!(f, "LoginPrompt"),
            Message::PasswordPrompt => write!(f, "PasswordPrompt"),
            Message::LoggedIn => write!(f, "LoggedIn"),
            Message::ButtonDown{remote_id, button_id} => write!(f, "ButtonDown, remote_id: {}, button_id: {}", remote_id, button_id),
            Message::ButtonUp{remote_id, button_id} => write!(f, "ButtonUp, remote_id: {}, button_id: {}", remote_id, button_id)
        }
    }
}



#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("192.168.86.144:23").await?;

    let mut connection = CasetaConnection::new(stream);

    let contents = connection.read_frame().await.expect("something weird happened");

    if let Some(message) = contents {
        if let Message::LoginPrompt = message{
            println!("great! we're able to log in")
        } else {
            panic!("expected a login prompt, but got nothing")
        }
    } else {
        panic!("expected to read the login prompt but got nothing");
    }


    connection.log_in().await.expect("unable to log in");

    loop {
        let contents = connection.read_frame().await.expect("something weird, again");
        if contents.is_some() {
            println!("got contents: {}", contents.unwrap());
        }
    }
}