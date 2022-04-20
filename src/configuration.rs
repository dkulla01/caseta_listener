use std::net::IpAddr;
use config::ConfigError;

#[derive(serde::Deserialize)]
pub struct CasetaHubSettings {
    pub caseta_host: IpAddr,
    pub caseta_port: u16,
    pub caseta_username: String,
    pub caseta_password: String
}


pub fn get_caseta_hub_settings() -> Result<CasetaHubSettings, config::ConfigError> {
    let settings = config::Config::builder()
        .add_source(config::File::with_name("configuration.yaml"))
        .add_source(config::Environment::with_prefix("CASETA_LISTENER"))
        .build();
    settings.unwrap().try_deserialize()
}
