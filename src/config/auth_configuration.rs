use std::net::IpAddr;

#[derive(serde::Deserialize)]
pub struct AuthConfiguration {
    pub caseta_host: IpAddr,
    pub caseta_port: u16,
    pub caseta_username: String,
    pub caseta_password: String,
    pub hue_application_key: String
}


pub fn get_auth_configuration() -> Result<AuthConfiguration, config::ConfigError> {
    let settings = config::Config::builder()
        .add_source(config::File::with_name("configuration.yaml"))
        .add_source(config::Environment::with_prefix("CASETA_LISTENER"))
        .build();
    settings.unwrap().try_deserialize()
}
