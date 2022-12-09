use crate::client::dispatcher::{DeviceAction, DeviceActionMessage};
use crate::client::hue::HueClient;
use crate::config::caseta_remote::{ButtonAction, ButtonId};
use crate::config::scene::Room;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tracing::{debug, instrument, warn};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);
const REMOTE_WATCHER_LOOP_SLEEP_DURATION: Duration = Duration::from_millis(500);

// note: it seems like caseta has some built in timeout for long presses.
// when you press and hold the remote, it blinks once when you first press it, and then again after about 5 seconds
// the caseta hub sees the first button press, but then it doesn't see the button release event after this second post-timeout flash
// so a 5 second hard timeout here is probably enough to capture the longest long presses
const REMOTE_WATCHER_LOOP_MAXIMUM_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct RemoteWatcher {
    pub remote_history: Arc<Mutex<RemoteHistory>>,
    pub remote_id: u8,
    pub button_id: ButtonId,
    pub action_sender: mpsc::Sender<DeviceActionMessage>,
}

impl RemoteWatcher {
    pub fn new(
        remote_id: u8,
        button_id: ButtonId,
        action_sender: mpsc::Sender<DeviceActionMessage>,
    ) -> RemoteWatcher {
        RemoteWatcher {
            remote_history: Arc::new(Mutex::new(RemoteHistory::new(button_id))),
            remote_id,
            button_id,
            action_sender,
        }
    }
}

#[derive(Debug)]
pub struct RemoteHistory {
    button_id: ButtonId,
    pub button_state: Option<ButtonState>, // todo: should this be an option? should there be an "unpressed" button state?
    pub finished: bool,
    tracking_started_at: Instant,
}

impl RemoteHistory {
    fn new(button_id: ButtonId) -> RemoteHistory {
        RemoteHistory {
            button_id,
            button_state: Option::None,
            finished: false,
            tracking_started_at: Instant::now(),
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
    pub fn increment(&mut self, button_id: ButtonId, button_action: &ButtonAction) -> () {
        if button_id != self.button_id {
            return;
        }
        if self.button_state.is_none() {
            match button_action {
                ButtonAction::Press => {
                    self.button_state = Option::Some(ButtonState::FirstPressAwaitingRelease);
                }
                ButtonAction::Release => {
                    // no-op for releases on the first button action
                }
            }
            return;
        }

        let current_button_state = self.button_state.as_ref().unwrap();
        match (current_button_state, button_action) {
            (ButtonState::FirstPressAwaitingRelease, ButtonAction::Release)
            | (ButtonState::FirstPressAndFirstRelease, ButtonAction::Press)
            | (ButtonState::SecondPressAwaitingRelease, ButtonAction::Release) => {
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

    pub fn is_finished(&self) -> bool {
        let now = Instant::now();
        let elapsed_tracking_time = now.duration_since(self.tracking_started_at);
        self.finished || elapsed_tracking_time >= REMOTE_WATCHER_LOOP_MAXIMUM_DURATION
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ButtonState {
    FirstPressAwaitingRelease,
    FirstPressAndFirstRelease,
    SecondPressAwaitingRelease,
    SecondPressAndSecondRelease,
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
            ButtonState::SecondPressAndSecondRelease => ButtonState::SecondPressAndSecondRelease,
        }
    }
}

#[instrument(skip(watcher), fields(remote_id=watcher.remote_id))]
pub async fn remote_watcher_loop(watcher: Arc<RemoteWatcher>) {
    let remote_id = watcher.remote_id;
    let button_id = watcher.button_id;
    debug!(remote_id = remote_id, "started tracking remote");
    sleep(DOUBLE_CLICK_WINDOW).await;
    let mut device_action_message = Option::None;
    let mut finished = false;
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
                    device_action_message = Option::Some(DeviceActionMessage::new(
                        DeviceAction::SinglePressComplete,
                        remote_id,
                        button_id,
                    ));
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
                    device_action_message = Option::Some(DeviceActionMessage::new(
                        DeviceAction::DoublePressComplete,
                        remote_id,
                        button_id,
                    ));
                    locked_history.finished = true;
                }
            }
        } else {
            warn!(remote_id=remote_id, button_id=%button_id, "there was no initial button state for this button, which is unusual to say the least");
            // todo: should this be an exceptional condition that short-circuits?
        }
        finished = locked_history.is_finished();
    }

    match device_action_message {
        Some(message) => {
            watcher.action_sender.send(message).await.unwrap();
        }
        None => {}
    }

    if finished {
        return;
    }

    loop {
        sleep(REMOTE_WATCHER_LOOP_SLEEP_DURATION).await;
        let history = watcher.remote_history.clone();
        {
            let mut locked_history = history.lock().unwrap();
            let button_state = locked_history
                .button_state
                .as_ref()
                .expect("button state should have been set by now.");
            match button_state {
                ButtonState::FirstPressAndFirstRelease => {
                    // a long press has finished here;
                    locked_history.finished = true;
                    debug!(remote_id=%remote_id, button_id=%button_id, "a long press has just finished");
                    device_action_message = Option::Some(DeviceActionMessage::new(
                        DeviceAction::LongPressComplete,
                        remote_id,
                        button_id,
                    ));
                }
                ButtonState::FirstPressAwaitingRelease => {
                    // a long press is still ongoing here. continue onward
                    debug!(remote_id=%remote_id, button_id=%button_id, "a long press is still ongoing here");
                    // there might be action depending on the button. E.G. do we increase/decrease the lights?
                    device_action_message = Option::Some(DeviceActionMessage::new(
                        DeviceAction::LongPressOngoing,
                        remote_id,
                        button_id,
                    ));
                }
                ButtonState::SecondPressAwaitingRelease => {
                    // a double press is still ongoing here. we're just waiting for the release, so nothing to see here.
                }
                ButtonState::SecondPressAndSecondRelease => {
                    // a double press has finished here!
                    locked_history.finished = true;
                    debug!(remote_id=%remote_id, button_id=%button_id, "a double press has just finished");
                    device_action_message = Option::Some(DeviceActionMessage::new(
                        DeviceAction::DoublePressComplete,
                        remote_id,
                        button_id,
                    ));
                }
            }
            finished = locked_history.is_finished();
        }

        match device_action_message {
            Some(message) => {
                watcher.action_sender.send(message).await.unwrap();
            }
            None => {}
        }
        if finished {
            return;
        }
    }
}
