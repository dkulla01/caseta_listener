use std::str::FromStr;
use std::fmt::Display;
use crate::caseta::ButtonEventType::*;

#[derive(Debug)]
pub enum Message {
    ButtonEvent{remote_id: u8, event: ButtonEventType},
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
            println!("got line: {}", s);
            let parts : Vec<&str> = s.trim().split(",").collect();
            let remote_id: u8 = parts[1].parse().expect("only integer values are allowed");
            let button_id: u8 = parts[2].parse().expect("only integer values are allowed");
            let button_action_value : u8 = parts[3].parse().expect("only integers are allowed, but got {}");
            let parsed_message = Message::ButtonEvent {
                remote_id,
                event: ButtonEventType::from_ids(button_id, button_action_value)
                    .expect(format!("got invalid ids: {}, {}", button_id, button_action_value).as_str())
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
            Message::ButtonEvent{remote_id, event} => write!(f, "ButtonDown, remote_id: {}, event_type: {}", remote_id, event)
        }
    }
}


#[derive(enum_display_derive::Display, Debug)]
pub enum ButtonEventType {
    PowerOnButtonPressed,
    PowerOnButtonReleased,
    UpButtonPressed,
    UpButtonReleased,
    FavoriteButtonPressed,
    FavoriteButtonReleased,
    DownButtonPressed,
    DownButtonReleased,
    PowerOffButtonPressed,
    PowerOffButtonReleased,

}

impl ButtonEventType {
    pub
    fn from_ids(button_id: u8, button_action_id: u8) -> Result<ButtonEventType, String>{
        match (button_id, button_action_id) {
            (2, 3) => Ok(PowerOnButtonPressed),
            (2, 4) => Ok(PowerOnButtonReleased),
            (5, 3) => Ok(UpButtonPressed),
            (5, 4) => Ok(UpButtonReleased),
            (3, 3) => Ok(FavoriteButtonPressed),
            (3, 4) => Ok(FavoriteButtonReleased),
            (6, 3) => Ok(DownButtonPressed),
            (6, 4) => Ok(DownButtonReleased),
            (4, 3) => Ok(PowerOffButtonPressed),
            (4, 4) => Ok(PowerOnButtonReleased),
            (_1, _2) => Err(format!("button_id {}, action_id {} is not a valid button id", _1, _2))
        }
    }
}
