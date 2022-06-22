use std::borrow::Borrow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Pointer, Write};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::time::{Instant, sleep};
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{EnvFilter, fmt, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing::{debug, error, info, instrument, trace, warn};

use caseta_listener::caseta::{ButtonAction, ButtonId, DefaultTcpSocketProvider, RemoteId};
use caseta_listener::caseta::Message::ButtonEvent;
use caseta_listener::caseta::{CasetaConnection, CasetaConnectionError};
use caseta_listener::configuration::get_caseta_hub_settings;
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(100);
const REMOTE_WATCHER_LOOP_SLEEP_DURATION: Duration = Duration::from_millis(500);

// note: it seems like caseta has some built in timeout for long presses.
// when you press and hold the remote, it blinks once when you first press it, and then again after about 5 seconds
// the caseta hub sees the first button press, but then it doesn't see the button release event after this second post-timeout flash
// so a 5 second hard timeout here is probably enough to capture the longest long presses
const REMOTE_WATCHER_LOOP_MAXIMUM_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct RemoteWatcher {
    remote_history: Arc<Mutex<RemoteHistory>>,
    remote_id: u8,
    button_id: ButtonId
}

impl RemoteWatcher {
    fn new(remote_id: u8, button_id: ButtonId) -> RemoteWatcher {
        RemoteWatcher {
            remote_history: Arc::new(Mutex::new(RemoteHistory::new(button_id))),
            remote_id,
            button_id
        }
    }
}

#[derive(Debug)]
struct RemoteHistory {
    button_id: ButtonId,
    button_state: Option<ButtonState>, // todo: should this be an option? should there be an "unpressed" button state?
    finished: bool,
    tracking_started_at: Instant
}

impl RemoteHistory {
    fn new(button_id: ButtonId) -> RemoteHistory {
        RemoteHistory {
            button_id,
            button_state: Option::None,
            finished: false,
            tracking_started_at: Instant::now()
        }
    }


    //todo: this has to be smarter than simply counting presses and releases, because the caseta remotes misbehave
    // when you're holding down a button, pressing and releasing a different button on
    // the same remote causes the remote to send a signal for the held down button instead of the
    // just pressed button
    // e.g.
    // press the (and hold) power on button -> caseta reports REMOTE X, BUTTON_ID: PowerOn, BUTTON_ACTION: Press
    // press the power off button           -> caseta doesn't see this signal
    // release the power off button         -> caseta reports REMOTE X, BUTTON_ID: PowerOn, BUTTON_ACTION: Press
    // release the power on button          -> caseta reports REMOTE X, BUTTON_ID: PowerOn, BUTTON_ACTION: Release
    // instead of incrementing press and release counts, I want this to walk through the transitions in the button
    // behavior state machine
    #[instrument]
    fn increment(&mut self, button_id: ButtonId, button_action: &ButtonAction) -> () {
        if button_id != self.button_id {
            return;
        }
        if self.button_state.is_none() {
            match button_action {
                ButtonAction::Press => {
                    self.button_state = Option::Some(ButtonState::FirstPressAwaitingRelease);
                },
                ButtonAction::Release => {
                    // no-op for releases on the first button action
                }
            }
            return;
        }

        let current_button_state = self.button_state.as_ref().unwrap();
        match (current_button_state, button_action) {
            (ButtonState::FirstPressAwaitingRelease, ButtonAction::Release) |
            (ButtonState::FirstPressAndFirstRelease, ButtonAction::Press) |
            (ButtonState::SecondPressAwaitingRelease, ButtonAction::Release)  => {
                let next_button_state = current_button_state.next_button_state();
                debug!(
                    current_button_state=%current_button_state,
                    button_action=%button_action,
                    "transitioning from {} to {} because of button {}",
                    current_button_state,
                    next_button_state,
                    button_action
                );
                self.button_state = Option::Some(next_button_state)
            }
            (_ignored_button_state, _ignored_action) => {
                debug!(
                    current_button_state=%_ignored_button_state,
                    button_action=%_ignored_action,
                    "no-op here, because the current button state is {}, but we saw a button {}",
                    _ignored_button_state,
                    _ignored_action
                );
            }
        }
    }

    fn is_finished(&self) -> bool {
        let now = Instant::now();
        let elapsed_tracking_time = now.duration_since(self.tracking_started_at);
        self.finished || elapsed_tracking_time >= REMOTE_WATCHER_LOOP_MAXIMUM_DURATION
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

impl ButtonState {
    fn next_button_state(&self) -> ButtonState {
        match self {
            ButtonState::FirstPressAwaitingRelease => ButtonState::FirstPressAndFirstRelease,
            ButtonState::FirstPressAndFirstRelease => ButtonState::SecondPressAwaitingRelease,
            ButtonState::SecondPressAwaitingRelease => ButtonState::SecondPressAndSecondRelease,
            // for many consecutive rapid presses, we'll just treat them as a single "double press"
            ButtonState::SecondPressAndSecondRelease => ButtonState::SecondPressAndSecondRelease
        }
    }
}

type ButtonWatcherDb = HashMap<String, Arc<ButtonWatcher>>;
type RemoteWatcherDb = HashMap<RemoteId, Arc<RemoteWatcher>>;


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

    let mut remote_watchers : RemoteWatcherDb = HashMap::new();

    loop {
        let contents = connection.await_message().await;
        match contents {
            Ok(ButtonEvent { remote_id, button_id, button_action }) => {
                let button_key = format!("{}-{}-{}", remote_id, button_id, button_action);
                debug!(
                    remote_id=%remote_id,
                    button_id=%button_id,
                    button_action=%button_action,
                    button_key=button_key.as_str(),
                    "Observed a button event: {}",
                    button_key
                );

                match remote_watchers.entry(remote_id) {
                    Entry::Occupied(mut entry) => {
                        let remote_watcher = entry.get();
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        if remote_history.is_finished() {
                            let remote_watcher = Arc::new(RemoteWatcher::new(remote_id, button_id));
                            remote_watcher.remote_history.lock().unwrap().increment(button_id, &button_action);
                            entry.insert(remote_watcher.clone());
                            tokio::spawn(remote_watcher_loop(remote_watcher));
                        } else {
                            remote_history.increment(button_id, &button_action)
                        }
                    }
                    Entry::Vacant(entry) => {
                        if let ButtonAction::Release = button_action {
                            continue
                        }
                        let remote_watcher = Arc::new(RemoteWatcher::new(remote_id, button_id));
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        remote_history.increment(button_id, &button_action);
                        entry.insert(remote_watcher.clone());
                        tokio::spawn(remote_watcher_loop(remote_watcher));
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
    let button_id = watcher.button_id;
    debug!(remote_id=remote_id, "started tracking remote");
    sleep(DOUBLE_CLICK_WINDOW).await;

    {
        let history = watcher.remote_history.clone();
        let mut locked_history = history.lock().unwrap();
        let button_state = &locked_history.button_state;

        debug!(remote_id=remote_id, button_id=%button_id, "first pass at evaluating button state");
        if button_state.is_some() {
            let button_state = button_state.as_ref().unwrap();
            match button_state {
                ButtonState::FirstPressAwaitingRelease => {
                    // perform the long press started action here
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "a long press has started but not finished");
                }
                ButtonState::FirstPressAndFirstRelease => {
                    // perform the single press action
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "a single press has finished");
                    locked_history.finished = true;
                }
                ButtonState::SecondPressAwaitingRelease => {
                    // this is kind of a no-op -- we're waiting for this button to be released so that
                    // we can perform a double press action
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "we're waiting for a double press to finish");
                }
                ButtonState::SecondPressAndSecondRelease => {
                    //perform the double press action
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "a double press has finished");
                    locked_history.finished = true;
                }
            }
        } else {
            warn!(remote_id=remote_id, button_id=%button_id, "there was no initial button state for this button, which is unusual to say the least")
            // todo: should this be an exceptional condition that short-circuits?
        }
        if locked_history.is_finished() {
            return;
        }
    }

    loop {
        sleep(REMOTE_WATCHER_LOOP_SLEEP_DURATION).await;
        let history = watcher.remote_history.clone();
        let mut locked_history = history.lock().unwrap();
        let button_state = locked_history.button_state.as_ref().expect("button state should have been set by now.");
        match button_state {
            ButtonState::FirstPressAndFirstRelease => {
                // a long press has finished here;
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
                locked_history.finished = true;
                debug!(remote_id=%remote_id, button_id=%button_id, "a double press has just finished")
            }
        }
        if locked_history.is_finished() {
            return
        }
    }
}
