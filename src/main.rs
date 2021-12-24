use std::io;

use tokio::net::TcpStream;
use caseta_listener::caseta::{Message, CasetaConnection, ButtonId, ButtonAction};
use caseta_listener::caseta::Message::ButtonEvent;
use std::collections::HashMap;
use std::time::Duration;
use std::collections::hash_map::Entry;
use tokio::sync::mpsc::{self, Receiver, Sender};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(250);

struct ButtonWatcher {
    remote_id: u8,
    button_id: ButtonId,
    press_count: u8,
    release_count: u8,
    event_receiver: Receiver<ButtonAction>
}

impl ButtonWatcher {
    fn new(receiver: Receiver<ButtonAction>, remote_id: u8, button_id: ButtonId) -> ButtonWatcher {
        ButtonWatcher {
            remote_id: remote_id,
            button_id: button_id,
            press_count: 0,
            release_count: 0,
            event_receiver: receiver
        }
    }
}

type ButtonWatcherDb = HashMap<String, Sender<ButtonAction>>;

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("192.168.86.144:23").await?;

    let mut connection = CasetaConnection::new(stream);

    let contents = connection.read_frame().await.expect("something weird happened");
    let mut button_watchers : ButtonWatcherDb= HashMap::new();


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
        match contents {
            Some(ButtonEvent { remote_id, button_id, button_action }) => {
                let button_key = format!("{}-{}", remote_id, button_id);
                match button_watchers.entry(button_key) {
                    Entry::Occupied(entry) => {
                        entry.get().send(button_action).await.unwrap();
                    },
                    Entry::Vacant(entry) => {
                        let (sender, receiver) = mpsc::channel(16);
                        let button_watcher = ButtonWatcher::new(receiver, remote_id, button_id);
                        let output = sender.send(button_action).await;
                        match output {
                            Ok(_) => println!("inserted!"),
                            Err(x) => println!("uh oh...: {:?}", x)
                        }

                        entry.insert(sender);

                        tokio::spawn(button_watcher_loop(button_watcher));
                    }
                }

                // add to events map
                // spawn a watcher task
            },
            Some(_) => println!("{}", contents.unwrap()),
            None => println!("got a frame with nothing in it")
        }
    }
}

async fn button_watcher_loop(mut watcher: ButtonWatcher) {
    while let Some(event) = watcher.event_receiver.recv().await {
        println!("remote: {}, buttonId: {}, got event: {}", watcher.remote_id, watcher.button_id, event)
    }
}