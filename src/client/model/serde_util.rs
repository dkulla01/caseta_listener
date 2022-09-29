use serde::{Deserializer, Deserialize, de};

use crate::client::model::hue::HueFloat;


pub fn deserialize_hue_float<'de, D>(deserializer: D) -> Result<HueFloat, D::Error>
where D: Deserializer<'de> {
    let buf = f64::deserialize(deserializer)?.to_string();
    let parts: Vec<&str> = buf.split(".").collect();
    match parts.len() {
        2 => {
            let integral: u8 = parts[0].parse().map_err(serde::de::Error::custom)?;
            let decimal: u32 = parts[1].parse().map_err(serde::de::Error::custom)?;
            Ok(HueFloat::new(integral, decimal))
        }
        _ => Err(de::Error::custom("invalaid float value"))
    }
}
