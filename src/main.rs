use std::io;

use tokio::net::TcpStream;
use caseta_listener::caseta::{Message, CasetaConnection, ButtonId, ButtonAction};
use caseta_listener::caseta::Message::ButtonEvent;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::collections::hash_map::Entry;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::macros::support::Future;
use tokio::time::sleep;
use tokio::task::JoinHandle;
use std::sync::{Arc, Mutex};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);

struct ButtonWatcher {
    button_history: Arc<Mutex<ButtonHistory>>,
    remote_id: u8,
    button_id: ButtonId,
}

impl ButtonWatcher {
    fn new(remote_id: u8, button_id: ButtonId) -> ButtonWatcher {
        ButtonWatcher {
            button_history: Arc::new(Mutex::new(ButtonHistory::new())),
            remote_id: remote_id,
            button_id: button_id,
        }
    }
}

struct ButtonHistory {
    press_count: u8,
    release_count: u8,
    finished: bool
}

impl ButtonHistory {
    fn new() -> ButtonHistory {
        ButtonHistory {
            press_count: 0,
            release_count: 0,
            finished: false
        }
    }

    fn increment(&mut self, button_action : ButtonAction) {
        match button_action {
            ButtonAction::Press => self.press_count += 1,
            ButtonAction::Release => self.press_count += 1
        }
    }
}
type ButtonWatcherDb = HashMap<String, ButtonWatcher>;

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

    let mut button_watchers : ButtonWatcherDb = HashMap::new();
    loop {
        let contents = connection.read_frame().await.expect("something weird, again");
        match contents {
            Some(ButtonEvent { remote_id, button_id, button_action }) => {
                let button_key = format!("{}-{}", remote_id, button_id);
                match button_watchers.entry(button_key) {
                    Entry::Occupied(mut entry) => {
                        let mut button_watcher = entry.get();
                        let history = button_watcher.button_history.clone();
                        let mut history =  history.lock().unwrap();
                        if history.finished {
                            entry.insert(ButtonWatcher::new(remote_id, button_id));
                        } else {
                            history.increment(button_action)
                        }
                    },
                    Entry::Vacant(entry) => {

                        match button_action {
                            ButtonAction::Release => {}, // no-op for an errant release
                            ButtonAction::Press => {
                                let button_watcher = ButtonWatcher::new(remote_id, button_id);
                                // entry.insert(button_watcher)
                                tokio::spawn(button_watcher_loop(entry.insert(button_watcher)));
                            }
                        }
                    }
                }
            },
            Some(_) => println!("{}", contents.unwrap()),
            None => println!("got a frame with nothing in it")
        }
    }
}

async fn button_watcher_loop<'a>(mut watcher: &'a ButtonWatcher) {

    // sleep for a smidge, then check the button state
    sleep(DOUBLE_CLICK_WINDOW).await;
    {
        let mut first_history = watcher.button_history.clone();
        let mut locked_history = first_history.lock().unwrap();
        let press_count = locked_history.press_count;
        let release_count = locked_history.release_count;

        if press_count == 1 && release_count == 1 {
            println!("a single press has finished");
            locked_history.finished = true;
            return;
        } else if press_count == 1 && release_count == 0 {
            println!("a long press has been started");
            // send the "long_press_started" event
        } else if press_count >= 2 && release_count != press_count {
            println!("a double press has started but not finished");
            // this is a no-op
        } else if press_count > 2 && release_count == press_count {
            println!("a double press has finished");
            // send the "double press" event
            locked_history.finished = true;
            return;
        }
    }
    loop {
        sleep(Duration::from_millis(100));
        let history = watcher.button_history.clone();
        let mut locked_history = history.lock().unwrap();
        let press_count = locked_history.press_count;
        let release_count = locked_history.release_count;
        if press_count == 1 && release_count == 0 {
            println!("a long press is in progress");
        } else if press_count == 1 && release_count == 1{
            println!("a long press has finished!");
            locked_history.finished = true;
            return;
        } else if press_count >= 2 && press_count > release_count {
            println!("a double click has started but not finished")
        } else if press_count >= 2 && press_count == release_count {
            println!("a double click has finished");
            locked_history.finished = true;
            return
        } else {
            // this shouldn't happen?
        }
    }

}