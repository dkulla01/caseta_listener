use std::io;

use tokio::net::TcpStream;
use caseta_listener::caseta::{Message, CasetaConnection, ButtonId, ButtonAction};
use caseta_listener::caseta::Message::ButtonEvent;
use std::collections::HashMap;
use std::time::{Instant, Duration};
use std::fmt::Display;

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(250);

struct ButtonHistory {
    first_press: Option<Instant>,
    first_release: Option<Instant>,
    second_press: Option<Instant>,
}

enum ButtonLifecycle {
    AwaitingFirstPress,
    AwaitingFirstRelease,
    AwaitingSecondPress,
    AwaitingSecondRelease
}

impl ButtonLifecycle {
    fn from_button_history(button_history: &ButtonHistory) -> Result<ButtonLifecycle, String> {
        match (button_history.first_press, button_history.first_release, button_history.second_press) {
            (None, None, None) => Ok(ButtonLifecycle::AwaitingFirstPress),
            (Some(_), None, None) => Ok(ButtonLifecycle::AwaitingFirstRelease),
            (Some(_), Some(_), None) => Ok(ButtonLifecycle::AwaitingSecondPress),
            (Some(_), Some(_), Some(_)) => Ok(ButtonLifecycle::AwaitingSecondRelease),
            _ => Err(format!("got an invalid button history"))
        }
    }
}

impl Display for ButtonLifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ButtonLifecycle::AwaitingFirstPress => write!(f, "AwaitingFirstPress"),
            ButtonLifecycle::AwaitingFirstRelease => write!(f, "AwaitingFirstRelease"),
            ButtonLifecycle::AwaitingSecondPress => write!(f, "AwaitingSecondPress"),
            ButtonLifecycle::AwaitingSecondRelease => write!(f, "AwaitingSecondRelease")
        }
    }
}

impl ButtonHistory {

    fn new() -> ButtonHistory {
        ButtonHistory {
            first_press: None,
            first_release: None,
            second_press: None
        }
    }

    fn transition_to_next_lifecycle_stage(&mut self, now: Instant) -> Result<(), String> {
        let button_lifecycle = ButtonLifecycle::from_button_history(self)?;
        match button_lifecycle {
            ButtonLifecycle::AwaitingFirstPress => {
                self.first_press = Some(now);
            },
            ButtonLifecycle::AwaitingFirstRelease => {
                self.first_release = Some(now);
            },
            ButtonLifecycle::AwaitingSecondPress => {
                // if more time than the double click duration has elapsed, treat this second press like a "first press"
                // and go back to the beginning
                let elapsed_so_far = now.duration_since(self.first_press.unwrap());
                if (elapsed_so_far.gt(&DOUBLE_CLICK_WINDOW)) {
                    self.first_press = Some(now);
                    self.first_release = None;
                } else {
                    self.second_press = Some(now);
                }
            },
            ButtonLifecycle::AwaitingSecondRelease => {
                self.first_press = None;
                self.first_release = None;
                self.second_press = None;
            }
        }

        Ok(())
    }

    fn has_been_double_clicked(&self) -> bool {
        let button_lifecycle = ButtonLifecycle::from_button_history(self)
            .expect("got an invalid button lifecycle");
        match button_lifecycle {
            ButtonLifecycle::AwaitingSecondRelease => {
                let duration_between_clicks = self.second_press.unwrap() - self.first_press.unwrap();
                duration_between_clicks.gt(&DOUBLE_CLICK_WINDOW)
            }
            _ => false
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("192.168.86.144:23").await?;

    let mut connection = CasetaConnection::new(stream);

    let contents = connection.read_frame().await.expect("something weird happened");
    let mut history_map : HashMap<String, ButtonHistory> = HashMap::new();

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
                // println!("got contents: {}", contents.unwrap());
                let now = Instant::now();
                let button_key = &format!("{}-{}", remote_id, button_id)[..];
                let button_history = history_map.entry(button_key.to_string()).or_insert(ButtonHistory::new());
                button_history.transition_to_next_lifecycle_stage(now);
                let current_stage = ButtonLifecycle::from_button_history(button_history)
                    .expect("we should have a valid stage here");
                println!("button-key: {}, current stage {}", button_key, current_stage);
                // add to events map
                // spawn a watcher task
            },
            Some(_) => println!("{}", contents.unwrap()),
            None => println!("got a frame with nothing in it")
        }
    }
}