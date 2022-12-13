use anyhow::{anyhow, bail, Ok, Result};
use log::error;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use std::collections::HashMap;
use tracing::{debug, instrument};
use url::Host;

use crate::client::model::hue::{HueResponse, HueRoom};
use uuid::Uuid;

use super::model::hue::{GroupedLight, GroupedLightPutBody, LightGroupOn};

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

        let base_url = Url::parse(format!("https://{}/clip/v2/resource/", host).as_str())
            .expect("unable to parse the hue base URL");
        HueClient {
            base_url,
            auth_key,
            http_client,
        }
    }

    #[instrument]
    pub async fn get_grouped_light(
        &self,
        grouped_light_room_id: Uuid,
    ) -> anyhow::Result<HueResponse<GroupedLight>> {
        let url = self
            .base_url
            .join(format!("grouped_light/{}", grouped_light_room_id).as_str())
            .expect("unable to parse grouped_light url");
        debug!(request_url=?url, "calling out to {}", url.as_str());
        let response = self.http_client.get(url).send().await?;
        response
            .json::<HueResponse<GroupedLight>>()
            .await
            .map_err(|e| anyhow!(e))
    }

    pub async fn get_rooms(&self) -> Result<HashMap<Uuid, HueRoom>> {
        let url = self
            .base_url
            .join("room")
            .expect("this should always be a well formed URL");
        let response = self.http_client.get(url).send().await.unwrap();

        let rooms = response.json::<HueResponse<HueRoom>>().await?;
        let mut rooms_by_id: HashMap<Uuid, HueRoom> = HashMap::new();
        rooms.data.into_iter().for_each(|room| {
            rooms_by_id.insert(room.id, room);
        });
        Ok(rooms_by_id)
    }

    pub async fn turn_on(
        &self,
        grouped_light_room_id: Uuid,
        brightness: Option<f32>,
    ) -> anyhow::Result<HueResponse<GroupedLight>> {
        let request_body = GroupedLightPutBody::builder().on(LightGroupOn::ON).build();

        let url = self
            .base_url
            .join(format!("grouped_light/{}", grouped_light_room_id).as_str())
            .expect("unable to parse grouped light url");
        let response = self.http_client.put(url).json(&request_body).send().await?;
        let status = response.status();
        if !status.is_success() {
            let response_body = &response.text().await?;
            error!(
                "there was a problem turning on the grouped_light {}. code: {}, body: {}",
                grouped_light_room_id, status, response_body
            );
            anyhow::bail!(
                "there was a problem turning on the grouped light {}. code: {}, body: {}",
                grouped_light_room_id,
                status,
                response_body
            )
        }
        // now the light should be on, so let's get the state of the grouped_light
        self.get_grouped_light(grouped_light_room_id).await
    }

    pub async fn turn_off(&self, grouped_light_room_id: Uuid) -> anyhow::Result<()> {
        let url = self
            .base_url
            .join(format!("grouped_light/{}", grouped_light_room_id).as_str())
            .expect("unable to build the request URI");

        let request_body = GroupedLightPutBody::builder().on(LightGroupOn::OFF).build();

        let response = self.http_client.put(url).json(&request_body).send().await?;
        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await?;
            error!(
                "there was a problem turning on the grouped light {}. status: {}, body: {}",
                grouped_light_room_id, status, response_body
            );
            bail!(
                "there was a problem turning on the grouped light {}. status: {}, body: {}",
                grouped_light_room_id,
                status,
                response_body
            )
        }

        Ok(())
    }
}
