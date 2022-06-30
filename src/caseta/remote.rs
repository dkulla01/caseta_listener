use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;
use tracing::{debug, instrument};
use crate::caseta::{ButtonAction, ButtonId};

// note: it seems like caseta has some built in timeout for long presses.
// when you press and hold the remote, it blinks once when you first press it, and then again after about 5 seconds
// the caseta hub sees the first button press, but then it doesn't see the button release event after this second post-timeout flash
// so a 5 second hard timeout here is probably enough to capture the longest long presses
const REMOTE_WATCHER_LOOP_MAXIMUM_DURATION: Duration = Duration::from_secs(5);


#[derive(Debug)]
pub struct RemoteWatcher {
    pub remote_history: Arc<Mutex<RemoteHistory>>,
    pub remote_id: u8,
    pub button_id: ButtonId
}

impl RemoteWatcher {
    pub fn new(remote_id: u8, button_id: ButtonId) -> RemoteWatcher {
        RemoteWatcher {
            remote_history: Arc::new(Mutex::new(RemoteHistory::new(button_id))),
            remote_id,
            button_id
        }
    }
}

#[derive(Debug)]
pub struct RemoteHistory {
    button_id: ButtonId,
    pub button_state: Option<ButtonState>, // todo: should this be an option? should there be an "unpressed" button state?
    pub finished: bool,
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
    pub fn increment(&mut self, button_id: ButtonId, button_action: &ButtonAction) -> () {
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

    pub fn is_finished(&self) -> bool {
        let now = Instant::now();
        let elapsed_tracking_time = now.duration_since(self.tracking_started_at);
        self.finished || elapsed_tracking_time >= REMOTE_WATCHER_LOOP_MAXIMUM_DURATION
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
