use anyhow::anyhow;
use serde_derive::Deserialize;
use std::env;
use std::fmt::{Display, Formatter};

const CASETA_REMOTE_CONFIG_FILE_NAME_ENV_VAR: &str = "CASETA_LISTENER_REMOTE_CONFIG_FILE";
const DEFAULT_CASETA_REMOTE_CONFIGURATION_FILE_NAME: &str = "caseta_remote_configuration.yaml";

pub type RemoteId = u8;

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub enum ButtonId {
    PowerOn,
    Up,
    Favorite,
    Down,
    PowerOff,
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
            _ => Err(anyhow!("{} is not a valid button id", id)),
        }
    }
}

#[derive(Debug)]
pub enum ButtonAction {
    Press,
    Release,
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
            _ => Err(anyhow!("{} is not a valid button action", id)),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CasetaRemote {
    TwoButtonPico { id: RemoteId, name: String },
    FiveButtonPico { id: RemoteId, name: String },
}

#[derive(Deserialize, Debug)]
pub struct RemoteConfiguration {
    pub remotes: Vec<CasetaRemote>,
}

pub fn get_caseta_remote_configuration() -> Result<RemoteConfiguration, config::ConfigError> {
    let configuration_file_name = match env::var(CASETA_REMOTE_CONFIG_FILE_NAME_ENV_VAR) {
        Ok(filename) => filename,
        _ => String::from(DEFAULT_CASETA_REMOTE_CONFIGURATION_FILE_NAME),
    };

    let settings = config::Config::builder()
        .add_source(config::File::with_name(configuration_file_name.as_str()))
        .add_source(config::Environment::with_prefix("CASETA_LISTENER"));

    settings.build().unwrap().try_deserialize()
}

#[cfg(test)]
mod tests {
    use crate::config::caseta_remote::{CasetaRemote, RemoteConfiguration};
    use spectral::assert_that;
    use spectral::prelude::*;

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

        let remote_configuration: RemoteConfiguration =
            serde_yaml::from_str(remote_configuration_text)
                .expect("unable to deserialize remote configuration");

        assert_that(&remote_configuration.remotes).has_length(2);
        assert!(matches!(
            &remote_configuration.remotes[0],
            CasetaRemote::FiveButtonPico { .. }
        ));
        assert!(matches!(
            &remote_configuration.remotes[1],
            CasetaRemote::TwoButtonPico { .. }
        ));
    }
}
