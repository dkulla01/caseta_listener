use std::borrow::Borrow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Pointer, Write};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use itertools::Itertools;
use tokio::time::sleep;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{EnvFilter, fmt, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing::{debug, error, info, instrument, trace, warn};

use caseta_listener::caseta::{ButtonAction, ButtonId, DefaultTcpSocketProvider, RemoteId};
use caseta_listener::caseta::Message::ButtonEvent;
use caseta_listener::caseta::{CasetaConnection, CasetaConnectionError};
use caseta_listener::configuration::get_caseta_hub_settings;
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);
const REMOTE_WATCHER_LOOP_SLEEP_DURATION: Duration = Duration::from_millis(100);

#[derive(Debug)]
struct RemoteWatcher {
    remote_history: Arc<Mutex<RemoteHistory>>,
    remote_id: u8
}

impl RemoteWatcher {
    fn new(remote_id: u8) -> RemoteWatcher {
        RemoteWatcher {
            remote_history: Arc::new(Mutex::new(RemoteHistory::new())),
            remote_id
        }
    }
}

#[derive(Debug)]
struct RemoteHistory {
    button_history: HashMap<ButtonId, ButtonHistory>,
    finished: bool
}

impl RemoteHistory {
    fn new() -> RemoteHistory {
        RemoteHistory {
            button_history: HashMap::new(),
            finished: false
        }
    }

    fn increment(&mut self, button_id: ButtonId, button_action: &ButtonAction) -> () {
        match self.button_history.entry(button_id) {
            Entry::Vacant(entry) => {
                // no-op for stray releases on uninitialized buttons
                if let ButtonAction::Release = button_action {
                    return ();
                }

                let button_history = ButtonHistory::new();
                entry.insert(button_history);
            },
            Entry::Occupied(mut entry) => {
                entry.get_mut().increment(&button_action)
            }

        }
    }

    fn is_finished(&self) -> bool {
        self.finished || self.button_history.iter().any(|(_button_id, button_history)| button_history.finished)
    }
}


#[derive(Debug)]
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
#[derive(Debug)]
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

    fn increment(&mut self, button_action : &ButtonAction) {
        match button_action {
            ButtonAction::Press => self.press_count += 1,
            ButtonAction::Release => self.release_count += 1
        }
    }
}

#[derive(Debug)]
pub enum ButtonState {
    FirstPressAwaitingRelease,
    FirstPressAndFirstRelease,
    SecondPressAwaitingRelease,
    SecondPressAndSecondRelease
}

impl Display for ButtonState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

type ButtonWatcherDb = HashMap<String, Arc<ButtonWatcher>>;
type RemoteWatcherDb = HashMap<RemoteId, RemoteWatcher>;


#[derive(Debug)]
struct RemoteState(HashMap<ButtonId, ButtonState>);

impl RemoteState {
    fn new() -> RemoteState {
        RemoteState(HashMap::new())
    }
}

impl Deref for RemoteState {
    type Target = HashMap<ButtonId, ButtonState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RemoteState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for RemoteState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let formatting_layer = BunyanFormattingLayer::new(
        "caseta_listener".into(),
        std::io::stdout
    );

    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    set_global_default(subscriber).expect("Failed to set subscriber");
    watch_caseta_events().await
}

#[instrument]
async fn watch_caseta_events() -> Result<()> {
    let caseta_hub_settings = get_caseta_hub_settings().unwrap();

    let caseta_address = caseta_hub_settings.caseta_host;
    let port = caseta_hub_settings.caseta_port;
    let tcp_socket_provider = DefaultTcpSocketProvider::new(caseta_address, port);
    let mut connection = CasetaConnection::new(caseta_hub_settings, &tcp_socket_provider);
    connection.initialize()
        .await?;

    let mut button_watchers : ButtonWatcherDb = HashMap::new();
    let mut remote_watchers : RemoteWatcherDb = HashMap::new();

    loop {
        let contents = connection.await_message().await;
        match contents {
            Ok(ButtonEvent { remote_id, button_id, button_action }) => {
                let button_key = format!("{}-{}", remote_id, button_id);
                debug!(
                    remote_id=%remote_id,
                    button_id=%button_id,
                    button_action=%button_action,
                    button_key=button_key.as_str(),
                    "Observed a button event"
                );

                match remote_watchers.entry(remote_id) {
                    Entry::Occupied(mut entry) => {
                        let remote_watcher = entry.get();
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        if remote_history.is_finished() {
                            let remote_watcher = RemoteWatcher::new(remote_id);
                            remote_watcher.remote_history.lock().unwrap().increment(button_id, &button_action);
                            entry.insert(remote_watcher);
                            // spawn the loop
                        } else {
                            remote_history.increment(button_id, &button_action)
                        }
                    }
                    Entry::Vacant(entry) => {
                        if let ButtonAction::Release = button_action {
                            continue
                        }
                        let remote_watcher = RemoteWatcher::new(remote_id);
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        remote_history.increment(button_id, &button_action)
                    }
                }

                match button_watchers.entry(button_key) {
                    Entry::Occupied(mut entry) => {
                        let button_watcher = entry.get();
                        let history = button_watcher.button_history.clone();
                        let mut history =  history.lock().unwrap();
                        if history.finished {
                            let button_watcher = Arc::new(ButtonWatcher::new(remote_id, button_id.clone()));
                            button_watcher.button_history.lock().unwrap().increment(&button_action);
                            entry.insert(button_watcher.clone());
                            tokio::spawn(button_watcher_loop(button_watcher));
                        } else {
                            history.increment(&button_action)
                        }
                    },
                    Entry::Vacant(entry) => {

                        match button_action {
                            ButtonAction::Release => {}, // no-op for an errant release
                            ButtonAction::Press => {
                                let button_watcher = Arc::new(ButtonWatcher::new(remote_id, button_id));
                                button_watcher.button_history.lock().unwrap().increment(&button_action);
                                entry.insert(button_watcher.clone());
                                tokio::spawn(button_watcher_loop(button_watcher));
                            }
                        }
                    }
                }
            },
            Ok(unexpected_contents) => warn!(message_contents=%unexpected_contents, "got an unexpected message type: {}", unexpected_contents),
            Err(CasetaConnectionError::Disconnected) => {
                info!("looks like our caseta connection was disconnected, so we're gonna create a new one!");
                connection = CasetaConnection::new(get_caseta_hub_settings().unwrap(), &tcp_socket_provider);
                connection.initialize().await?;
            }
            Err(other_caseta_connection_err) => {
                error!(caseta_connection_error=%other_caseta_connection_err, "there was a problem with the caseta connection");
                break Err(anyhow!("there was an issue with the caseta connection {:?} ", other_caseta_connection_err))
            }
        }
    }
}

#[instrument(skip(watcher), fields(remote_id=watcher.remote_id))]
async fn remote_watcher_loop(watcher: Arc<RemoteWatcher>) {
    let remote_id = watcher.remote_id;
    debug!(remote_id=remote_id, "started tracking remote");
    sleep(DOUBLE_CLICK_WINDOW).await;

    {
        let history = watcher.remote_history.clone();
        let mut locked_history = history.lock().unwrap();
        let button_state_by_button_id = get_button_state_by_button_id(locked_history.borrow());
        debug!(button_state_by_button_id=%button_state_by_button_id);

        for (button_id, button_state) in button_state_by_button_id.iter() {
            debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state);
            match button_state {
                ButtonState::FirstPressAwaitingRelease => {
                 // perform the long press started action here
                    debug!(remote_id=remote_id, button_id=%button_id, "a long press has started");
                }
                ButtonState::FirstPressAndFirstRelease => {
                    // perform the single press action
                    locked_history.button_history.get_mut(button_id).unwrap().finished=true;
                    debug!(remote_id=remote_id, button_id=%button_id, "a single press has finished");
                    locked_history.finished = true;
                }
                ButtonState::SecondPressAwaitingRelease => {
                    // this is kind of a no-op -- we're waiting for this button to be released so that
                    // we can perform a double press action
                    debug!(remote_id=remote_id, button_id=%button_id, "we're waiting for a double press to finish");
                }
                ButtonState::SecondPressAndSecondRelease => {
                    //perform the double press action
                    locked_history.button_history.get_mut(button_id).unwrap().finished=true;
                    debug!(remote_id=remote_id, button_id=%button_id, "a double press has finished");
                    locked_history.finished = true;
                }
            }
        }
        if locked_history.is_finished() {
            return;
        }
    }

    loop {
        sleep(REMOTE_WATCHER_LOOP_SLEEP_DURATION).await;
        let history = watcher.remote_history.clone();
        let mut locked_history = history.lock().unwrap();
        let button_state_by_button_id = get_button_state_by_button_id(&locked_history);
        for (button_id, button_state) in button_state_by_button_id.iter() {
            match button_state {
                ButtonState::FirstPressAndFirstRelease => {
                    // a long press has finished here;
                    locked_history.button_history.get_mut(button_id).unwrap().finished = true;
                    locked_history.finished = true;
                    debug!(remote_id=%remote_id, button_id=%button_id, "a long press has just finished")
                }
                ButtonState::FirstPressAwaitingRelease => {
                    // a long press is still ongoing here. continue onward
                    debug!(remote_id=%remote_id, button_id=%button_id, "a long press is still ongoing here");
                    // there might be action depending on the button. E.G. do we increase/decrease the lights?
                }
                ButtonState::SecondPressAwaitingRelease => {
                    // a double press is still ongoing here. we're just waiting for the release, so nothing to see here.
                }
                ButtonState::SecondPressAndSecondRelease => {
                    // a double press has finished here!
                    locked_history.button_history.get_mut(button_id).unwrap().finished = true;
                    locked_history.finished = true;
                    debug!(remote_id=%remote_id, button_id=%button_id, "a double press has just finished")
                }
            }
        }
        // in theory, this is a place where we could listen for weird combo presses.
        // e.g. were there long presses of two buttons? that could trigger some funky fun action.
        // for now though, the remote watcher is "finished" whenever the first button is "finished"
        if locked_history.is_finished() {
            return
        }
    }
}

fn get_button_state_by_button_id(remote_history: &RemoteHistory) -> RemoteState {
    let mut button_state_by_button_id: RemoteState = RemoteState::new();
    for (button_id, button_history) in remote_history.button_history.iter() {
        let press_count = button_history.press_count;
        let release_count = button_history.release_count;
        trace!(
            press_count=press_count,
            release_count=release_count,
            "first pass at evaluating button state"
        );
        if press_count == 1 && release_count == 1 {
            debug!(
                press_count=press_count,
                release_count=release_count,
                button_id=%button_id,
                "a single press has finished."
            );
            button_state_by_button_id.insert(*button_id, ButtonState::FirstPressAndFirstRelease);
        } else if press_count == 1 && release_count == 0 {
            debug!(
                press_count=press_count,
                release_count=release_count,
                button_id=%button_id,
                "a long press has started but not finished."
            );
            button_state_by_button_id.insert(*button_id, ButtonState::FirstPressAwaitingRelease);
        } else if press_count >= 2 && release_count != press_count {
            debug!(
                press_count=press_count,
                release_count=release_count,
                button_id=%button_id,
                "a double press has started but not finished."
            );
            button_state_by_button_id.insert(*button_id, ButtonState::SecondPressAwaitingRelease);
        } else if press_count >= 2 && press_count == release_count {
            debug!(
                press_count=press_count,
                release_count=release_count,
                button_id=%button_id,
                "a double press has finished."
            );
            button_state_by_button_id.insert(*button_id, ButtonState::SecondPressAndSecondRelease);
        }
    }
    return button_state_by_button_id
}

#[instrument(skip(watcher), fields(remote_id=watcher.remote_id, button_id=%watcher.button_id))]
async fn button_watcher_loop(watcher: Arc<ButtonWatcher>) {
    let button_id = &watcher.button_id;
    let remote_id = watcher.remote_id;
    debug!(remote_id=remote_id, button_id=%button_id, "started tracking a new remote");
    // sleep for a smidge, then check the button state
    sleep(DOUBLE_CLICK_WINDOW).await;
    {
        let first_history = watcher.button_history.clone();
        let mut locked_history = first_history.lock().unwrap();
        let press_count = locked_history.press_count;
        let release_count = locked_history.release_count;
        info!(
            press_count=press_count,
            release_count=release_count,
            "first pass at evaluating button state"
        );
        if press_count == 1 && release_count == 1 {
            info!(
                press_count=press_count,
                release_count=release_count,
                "a single press has finished."
            );
            locked_history.finished = true;
            return;
        } else if press_count == 1 && release_count == 0 {
            info!(
                press_count=press_count,
                release_count=release_count,
                "a long press has been started..."
            );
            // send the "long_press_started" event
        } else if press_count >= 2 && release_count != press_count {
            info!(
                press_count=press_count,
                release_count=release_count,
                "a double press has been started but not finished..."
            );
            // this is a no-op
        } else if press_count > 2 && release_count == press_count {
            info!(
                press_count=press_count,
                release_count=release_count,
                "a double press has been finished."
            );
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
            info!(
                press_count=press_count,
                release_count=release_count,
                "ongoing long press..."
            );
            // send long press is still going on event
        } else if press_count == 1 && release_count == 1 {
            info!(
                press_count=press_count,
                release_count=release_count,
                "a long press has finished!"
            );
            println!("a long press has finished!");
            // send long press is finished event
            locked_history.finished = true;
            return;
        } else if press_count >= 2 && press_count > release_count {
            info!(
                press_count=press_count,
                release_count=release_count,
                "ongoing double press..."
            );
        } else if press_count >= 2 && press_count == release_count {
            info!(
                press_count=press_count,
                release_count=release_count,
                "a double press has finished!"
            );
            // send double press has finished event
            locked_history.finished = true;
            return
        } else {
            // this shouldn't happen?
        }
    }

}
