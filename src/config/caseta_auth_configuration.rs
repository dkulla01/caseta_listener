use url::Host;
use crate::config::CASETA_LISTENER_ENV_VAR_PREFIX;

#[derive(serde::Deserialize)]
pub struct CasetaAuthConfiguration {
    #[serde(deserialize_with = "crate::config::serde_util::deserialize_host")]
    pub caseta_host: Host<String>,
    pub caseta_port: u16,
    pub caseta_username: String,
    pub caseta_password: String,
}


pub fn get_caseta_auth_configuration() -> Result<CasetaAuthConfiguration, config::ConfigError> {
    let settings = config::Config::builder()
        .add_source(config::File::with_name("configuration.yaml"))
        .add_source(config::Environment::with_prefix(CASETA_LISTENER_ENV_VAR_PREFIX))
        .build();
    settings.unwrap().try_deserialize()
}
