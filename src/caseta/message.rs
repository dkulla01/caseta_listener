use std::str::FromStr;
use std::fmt::Display;

#[derive(Debug)]
pub enum Message {
    ButtonEvent{remote_id: u8, button_id: ButtonId, button_action: ButtonAction},
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
            let remote_id: u8 = parts[1].parse().expect("only integer values are allowed");
            let button_id: u8 = parts[2].parse().expect("only integer values are allowed");
            let button_action_value : u8 = parts[3].parse().expect("only integers are allowed, but got {}");
            let parsed_message = Message::ButtonEvent {
                remote_id,
                button_id: ButtonId::from_id(button_id).expect("got an invalid button ID"),
                button_action: ButtonAction::from_id(button_action_value).expect("got an invalid button action ID")
            };
            return Ok(parsed_message);
        }

        Err("this is also not a thing".into())
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
#[derive(enum_display_derive::Display, Debug)]
pub enum ButtonId {
    PowerOn,
    Up,
    Favorite,
    Down,
    PowerOff
}

impl ButtonId {
    fn from_id(id: u8) -> Result<ButtonId, String>{
        match id {
            2 => Ok(ButtonId::PowerOn),
            5 => Ok(ButtonId::Up),
            3 => Ok(ButtonId::Favorite),
            6 => Ok(ButtonId::Down),
            8 => Ok(ButtonId::PowerOff),
            _ => Err(format!("{} is not a valid button id", id))
        }
    }
}

#[derive(enum_display_derive::Display, Debug)]
pub enum ButtonAction {
    Press,
    Release
}

impl ButtonAction {
    fn from_id(id: u8) -> Result<ButtonAction, String> {
        match id {
            3 => Ok(ButtonAction::Press),
            4 => Ok(ButtonAction::Release),
            _ => Err(format!("{} is not a valid button action", id))
        }
    }
}