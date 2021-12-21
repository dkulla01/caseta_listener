use std::str::FromStr;
use std::fmt::Display;

#[derive(Debug)]
pub enum Message {
    ButtonDown { remote_id: u8, button_id: u8 },
    ButtonUp { remote_id: u8, button_id: u8 },
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
            let parts : Vec<&str> = s.split(",").collect();
            let remote_id: u8 = parts[1].parse().expect("only integer values are allowed");
            let button_id: u8 = parts[2].parse().expect("only integer values are allowed");
            let button_action_value : u8 = parts[3].parse().expect("only integers are allowed");
            return match button_action_value {
                3 => Ok(Message::ButtonDown {remote_id, button_id}),
                4 => Ok(Message::ButtonUp {remote_id, button_id}),
                _ => Err("this is not a thing".into())
            };
        }

        Err("this is also not a thing".into())
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Message::LoginPrompt => write!(f, "LoginPrompt"),
            Message::PasswordPrompt => write!(f, "PasswordPrompt"),
            Message::LoggedIn => write!(f, "LoggedIn"),
            Message::ButtonDown{remote_id, button_id} => write!(f, "ButtonDown, remote_id: {}, button_id: {}", remote_id, button_id),
            Message::ButtonUp{remote_id, button_id} => write!(f, "ButtonUp, remote_id: {}, button_id: {}", remote_id, button_id)
        }
    }
}

