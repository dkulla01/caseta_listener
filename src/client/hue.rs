use std::net::IpAddr;
use anyhow::anyhow;
use reqwest::{Client, Url};
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::{instrument, debug};
use url::Host;

use uuid::Uuid;
use crate::client::model::hue::LightGroup;
use crate::config::scene::Room;

use super::model::hue::HueLightResponse;

const HUE_AUTH_KEY_HEADER: &str = "hue-application-key";

#[derive(Debug)]
pub struct HueClient {
    base_url: Url,
    auth_key: String,
    http_client: Client,
}

impl HueClient {
    pub fn new(host: Host, auth_key: String) -> HueClient {
        let mut headers = HeaderMap::new();
        let mut header_val = HeaderValue::from_str(auth_key.as_str())
            .expect("there was a problem setting the hue-application-key header");
        header_val.set_sensitive(true);
        headers.insert(HUE_AUTH_KEY_HEADER, header_val);
        let http_client = Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(true)
            .build()
            .expect("there was a problem building the http client");

        let base_url = Url::parse(
            format!("https://{}/clip/v2/resource/", host).as_str()
        ).expect("unable to parse the hue base URL");
        HueClient {
            base_url,
            auth_key,
            http_client
        }
    }

    #[instrument]
    pub async fn get_room_status(&self, grouped_light_room_id: Uuid) -> anyhow::Result<LightGroup> {
        let url = self.base_url.join(
            format!("grouped_light/{}", grouped_light_room_id).as_str()
        )
            .expect("unable to parse grouped_light url");
        debug!(request_url=?url, "calling out to {}", url.as_str());
        let response = self.http_client.get(url).send()
            .await?;
        // let content = response.text().await.unwrap();
        response.json::<LightGroup>()
            .await.map_err(|e| anyhow!(e))
    }

    #[instrument]
    pub async fn get_lights(&self) -> anyhow::Result<HueLightResponse> {
        let url = self.base_url.join("light").expect("msg");
        let response = self.http_client.get(url).send()
            .await.unwrap();
        let result = response.text()
            .await.unwrap();
        serde_json::from_str(&result).map_err(|err| anyhow!(err))
        // result.map_err(|e| anyhow!(e))
    }
}
