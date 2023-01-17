use crate::config::caseta_remote::{ButtonAction, ButtonId, RemoteId};
use std::fmt::{Debug, Display};
use std::str::FromStr;

#[derive(Debug)]
pub enum Message {
    ButtonEvent {
        remote_id: RemoteId,
        button_id: ButtonId,
        button_action: ButtonAction,
    },
    LoggedIn,
    LoginPrompt,
    PasswordPrompt,
}

impl FromStr for Message {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.starts_with("login: ") {
            return Ok(Message::LoginPrompt);
        } else if s.starts_with("password: ") {
            return Ok(Message::PasswordPrompt);
        } else if s.starts_with("GNET>") {
            return Ok(Message::LoggedIn);
        } else if s.starts_with("~DEVICE") {
            let parts: Vec<&str> = s.trim().split(",").collect();
            let remote_id: u8 = parts[1].parse().expect(
                format!("only integer values are allowed here, but got {}", parts[1]).as_str(),
            );
            let button_id: u8 = parts[2].parse().expect(
                format!("only integer values are allowed here, but got {}", parts[2]).as_str(),
            );
            let button_action_value: u8 = parts[3]
                .parse()
                .expect(format!("only integers are allowed, but got {}", parts[3]).as_str());
            let parsed_message = Message::ButtonEvent {
                remote_id,
                button_id: button_id.try_into().expect("got an invalid button ID"),
                button_action: button_action_value
                    .try_into()
                    .expect("got an invalid button action ID"),
            };
            return Ok(parsed_message);
        }

        Err(format!("got an un-parseable message: {}", s))
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Message::LoginPrompt => write!(f, "LoginPrompt"),
            Message::PasswordPrompt => write!(f, "PasswordPrompt"),
            Message::LoggedIn => write!(f, "LoggedIn"),
            Message::ButtonEvent {
                remote_id,
                button_id,
                button_action,
            } => write!(
                f,
                "ButtonAction remote_id: {}, button_id: {}, button_action: {}",
                remote_id, button_id, button_action
            ),
        }
    }
}
