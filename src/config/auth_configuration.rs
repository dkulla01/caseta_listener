use std::env;

use url::Host;

const DEFAULT_CONFIGURATION_FILE_NAME: &str = "configuration.yaml";
const DEFAULT_AUTH_CONFIGURATION_FILE_NAME: &str = "auth_configuration.yaml";
const AUTH_CONFIGURATION_FILE_NAME_ENV_VAR: &str = "CASETA_LISTENER_AUTH_CONFIGURATION_FILE";
const CASETA_LISTENER_ENV_VAR_PREFIX: &str = "CASETA_LISTENER";

#[derive(serde::Deserialize)]
pub struct AuthConfiguration {
    #[serde(deserialize_with = "crate::config::serde_util::deserialize_host")]
    pub caseta_host: Host<String>,
    pub caseta_port: u16,
    pub caseta_username: String,
    pub caseta_password: String,
    #[serde(deserialize_with = "crate::config::serde_util::deserialize_host")]
    pub hue_host: Host<String>,
    pub hue_application_key: String,
}

pub fn get_auth_configuration() -> Result<AuthConfiguration, config::ConfigError> {
    let mut settings = config::Config::builder()
        .add_source(config::File::with_name(DEFAULT_CONFIGURATION_FILE_NAME))
        .add_source(config::File::with_name(DEFAULT_AUTH_CONFIGURATION_FILE_NAME).required(false));

    match env::var(AUTH_CONFIGURATION_FILE_NAME_ENV_VAR) {
        Ok(filename) => {
            settings = settings.add_source(config::File::with_name(filename.as_str()));
        }
        Err(..) => {} // no-op. don't try to add a file without an env var pointing to it
    }
    settings = settings.add_source(config::Environment::with_prefix(
        CASETA_LISTENER_ENV_VAR_PREFIX,
    ));
    settings.build().unwrap().try_deserialize()
}
