use tokio::net::TcpStream;
use caseta_listener::caseta::{Message, CasetaConnection, ButtonId, ButtonAction};
use caseta_listener::caseta::Message::ButtonEvent;
use std::collections::HashMap;
use std::time::{Duration};
use std::collections::hash_map::Entry;
use std::net::{IpAddr, Ipv4Addr};
use tokio::time::sleep;
use std::sync::{Arc, Mutex};
use anyhow::Result;

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
            ButtonAction::Release => self.release_count += 1
        }
    }
}
type ButtonWatcherDb = HashMap<String, Arc<ButtonWatcher>>;

#[tokio::main]
async fn main() -> Result<()> {
    let caseta_address = IpAddr::V4("192.168.86.144".parse()?);
    let port = 23;
    let mut connection = CasetaConnection::new(caseta_address, port);
    connection.initialize()
        .await?;

    let mut button_watchers : ButtonWatcherDb = HashMap::new();
    loop {
        let contents = connection.await_message().await.expect("something weird, again");
        match contents {
            Some(ButtonEvent { remote_id, button_id, button_action }) => {
                let button_key = format!("{}-{}", remote_id, button_id);
                match button_watchers.entry(button_key) {
                    Entry::Occupied(mut entry) => {
                        let button_watcher = entry.get();
                        let history = button_watcher.button_history.clone();
                        let mut history =  history.lock().unwrap();
                        if history.finished {
                            let button_watcher = Arc::new(ButtonWatcher::new(remote_id, button_id));
                            button_watcher.button_history.lock().unwrap().increment(button_action);
                            entry.insert(button_watcher.clone());
                            tokio::spawn(button_watcher_loop(button_watcher));
                        } else {
                            history.increment(button_action)
                        }
                    },
                    Entry::Vacant(entry) => {

                        match button_action {
                            ButtonAction::Release => {}, // no-op for an errant release
                            ButtonAction::Press => {
                                let button_watcher = Arc::new(ButtonWatcher::new(remote_id, button_id));
                                button_watcher.button_history.lock().unwrap().increment(button_action);
                                entry.insert(button_watcher.clone());
                                tokio::spawn(button_watcher_loop(button_watcher));
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

async fn button_watcher_loop(watcher: Arc<ButtonWatcher>) {
    let button_id = &watcher.button_id;
    let remote_id = watcher.remote_id;
    println!("tracking remote {}, button {}", remote_id, button_id);
    // sleep for a smidge, then check the button state
    sleep(DOUBLE_CLICK_WINDOW).await;
    {
        let first_history = watcher.button_history.clone();
        let mut locked_history = first_history.lock().unwrap();
        let press_count = locked_history.press_count;
        let release_count = locked_history.release_count;

        if press_count == 1 && release_count == 1 {
            println!("a single press has finished");
            locked_history.finished = true;
            return;
        } else if press_count == 1 && release_count == 0 {
            print!("a long press has been started...");
            // send the "long_press_started" event
        } else if press_count >= 2 && release_count != press_count {
            print!("a double press has started but not finished...");
            // this is a no-op
        } else if press_count > 2 && release_count == press_count {
            println!("a double press has finished");
            // send the "double press" event
            locked_history.finished = true;
            return;
        }
    }
    loop {
        sleep(Duration::from_millis(100)).await;
        let history = watcher.button_history.clone();
        let mut locked_history = history.lock().unwrap();
        let press_count = locked_history.press_count;
        let release_count = locked_history.release_count;
        if press_count == 1 && release_count == 0 {
            println!("long press...");
        } else if press_count == 1 && release_count == 1 {
            println!("a long press has finished!");
            locked_history.finished = true;
            return;
        } else if press_count >= 2 && press_count > release_count {
            println!("double press...")
        } else if press_count >= 2 && press_count == release_count {
            println!("a double click has finished");
            locked_history.finished = true;
            return
        } else {
            // this shouldn't happen?
        }
    }

}
