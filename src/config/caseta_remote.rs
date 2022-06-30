use std::fmt::{Display, Formatter};
use anyhow::anyhow;
use serde_derive::Deserialize;
pub type RemoteId = u8;

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

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CasetaRemote {
    TwoButtonPico {id: RemoteId, name: String},
    FiveButtonPico {id: RemoteId, name: String},
}

#[derive(Deserialize, Debug)]
struct RemoteConfiguration {
    remotes: Vec<CasetaRemote>
}

#[cfg(test)]
mod tests {
    use spectral::assert_that;
    use spectral::prelude::*;
    use crate::config::caseta_remote::{CasetaRemote, RemoteConfiguration};

    #[test]
    fn it_deserializes_remote_configuration() {
        let remote_configuration_text = r#"
            remotes:
            - id: 2
              name: Office Pico
              type: five_button_pico
            - id: 3
              name: Fireplace Pico
              type: two_button_pico
        "#;

        let remote_configuration: RemoteConfiguration = serde_yaml::from_str(remote_configuration_text)
            .expect("unable to deserialize remote configuration");

        assert_that(&remote_configuration.remotes).has_length(2);
        assert!(matches!(&remote_configuration.remotes[0], CasetaRemote::FiveButtonPico {..}));
        assert!(matches!(&remote_configuration.remotes[1], CasetaRemote::TwoButtonPico {..}));
    }
}
