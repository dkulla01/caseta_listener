use std::str::FromStr;
use std::fmt::{Debug, Display, Formatter};
use anyhow::anyhow;

pub type RemoteId = u8;

#[derive(Debug)]
pub enum Message {
    ButtonEvent{remote_id: RemoteId, button_id: ButtonId, button_action: ButtonAction},
    LoggedIn,
    LoginPrompt,
    PasswordPrompt
}

impl FromStr for Message {

    type Err = String;

    fn from_str(s : &str) -> std::result::Result<Self, Self::Err> {
        if s.starts_with("login: ") {
            return Ok(Message::LoginPrompt);
        } else if s.starts_with("password: ") {
            return Ok(Message::PasswordPrompt);
        } else if s.starts_with("GNET>") {
            return Ok(Message::LoggedIn);
        } else if s.starts_with("~DEVICE") {
            let parts : Vec<&str> = s.trim().split(",").collect();
            let remote_id: u8 = parts[1].parse().expect(format!("only integer values are allowed here, but got {}", parts[1]).as_str());
            let button_id: u8 = parts[2].parse().expect(format!("only integer values are allowed here, but got {}", parts[2]).as_str());
            let button_action_value : u8 = parts[3].parse().expect(format!("only integers are allowed, but got {}", parts[3]).as_str());
            let parsed_message = Message::ButtonEvent {
                remote_id,
                button_id: button_id.try_into().expect("got an invalid button ID"),
                button_action: button_action_value.try_into().expect("got an invalid button action ID")
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
            Message::ButtonEvent{remote_id, button_id, button_action} => write!(f, "ButtonDown, remote_id: {}, button_id: {}, button_action: {}", remote_id, button_id, button_action)
        }
    }
}
#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub enum ButtonId {
    PowerOn,
    Up,
    Favorite,
    Down,
    PowerOff
}

impl Display for ButtonId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl TryFrom<u8> for ButtonId {
    type Error = anyhow::Error;

    fn try_from(id: u8) -> Result<Self, anyhow::Error> {
        match id {
            2 => Ok(ButtonId::PowerOn),
            5 => Ok(ButtonId::Up),
            3 => Ok(ButtonId::Favorite),
            6 => Ok(ButtonId::Down),
            4 => Ok(ButtonId::PowerOff),
            _ => Err(anyhow!("{} is not a valid button id", id))
        }
    }
}

#[derive(Debug)]
pub enum ButtonAction {
    Press,
    Release
}

impl Display for ButtonAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl TryFrom<u8> for ButtonAction {
    type Error = anyhow::Error;
    fn try_from(id: u8) -> Result<Self, Self::Error> {
        match id {
            3 => Ok(ButtonAction::Press),
            4 => Ok(ButtonAction::Release),
            _ => Err(anyhow!("{} is not a valid button action", id))
        }
    }
}
