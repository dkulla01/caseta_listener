use serde::{Deserialize, Deserializer};
use url::Host;

pub fn deserialize_host<'de, D>(deserializer: D) -> Result<Host, D::Error>
    where D: Deserializer<'de> {
    let buf = String::deserialize(deserializer)?;

    Host::parse(&buf).map_err(serde::de::Error::custom)
}
