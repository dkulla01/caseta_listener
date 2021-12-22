use std::io;

use tokio::net::TcpStream;
use caseta_listener::caseta::{Message, CasetaConnection};


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