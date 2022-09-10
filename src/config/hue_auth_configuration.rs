use url::Host;
use crate::config::CASETA_LISTENER_ENV_VAR_PREFIX;

#[derive(serde::Deserialize)]
pub struct HueAuthConfiguration {
    #[serde(deserialize_with = "crate::config::serde_util::deserialize_host")]
    #[serde(rename = "hue_host")]
    pub host: Host<String>,
    #[serde(rename = "hue_application_key")]
    pub application_key: String
}

pub fn get_hue_auth_configuration() -> Result<HueAuthConfiguration, config::ConfigError> {
    config::Config::builder()
        .add_source(config::Environment::with_prefix(CASETA_LISTENER_ENV_VAR_PREFIX))
        .build()
        .unwrap()
        .try_deserialize()
}
