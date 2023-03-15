use anyhow::{anyhow, bail, Ok, Result};
use log::error;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use std::collections::HashMap;
use tracing::{debug, instrument};
use url::Host;

use crate::client::model::hue::{HueResponse, HueRoom};
use uuid::Uuid;

use super::model::hue::{
    GroupedLight, GroupedLightPutBody, LightGroupDimming, LightGroupOn, RecallSceneBody,
};

const HUE_AUTH_KEY_HEADER: &str = "hue-application-key";
#[derive(Debug)]
pub struct HueClient {
    base_url: Url,
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
            http_client,
        }
    }

    #[instrument(level = "debug")]
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
        debug!("got get_grouped_light response: {:?}", response);
        response
            .json::<HueResponse<GroupedLight>>()
            .await
            .map_err(|e| anyhow!(e))
    }

    #[instrument(level = "debug")]
    pub async fn get_rooms(&self) -> Result<HashMap<Uuid, HueRoom>> {
        let url = self
            .base_url
            .join("room")
            .expect("this should always be a well formed URL");
        let response = self.http_client.get(url).send().await.unwrap();
        debug!("got get_rooms response: {:?}", response);

        let rooms = response.json::<HueResponse<HueRoom>>().await?;
        let mut rooms_by_id: HashMap<Uuid, HueRoom> = HashMap::new();
        rooms.data.into_iter().for_each(|room| {
            rooms_by_id.insert(room.id, room);
        });
        Ok(rooms_by_id)
    }

    fn build_grouped_light_url(&self, grouped_light_room_id: Uuid) -> Url {
        self.base_url
            .join(format!("grouped_light/{}", grouped_light_room_id).as_str())
            .expect("unable to build the request URI")
    }

    #[instrument(level = "debug")]
    pub async fn update_brightness(
        &self,
        grouped_light_room_id: Uuid,
        brightness: f32,
    ) -> anyhow::Result<()> {
        let url = self.build_grouped_light_url(grouped_light_room_id);
        let request_body = GroupedLightPutBody::builder()
            .dimming(LightGroupDimming::new(brightness))
            .on(LightGroupOn::ON)
            .build();
        let response = self.http_client.put(url).json(&request_body).send().await?;
        debug!("got update_brightness response: {:?}", response);
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

    #[instrument(level = "debug")]
    pub async fn turn_off(&self, grouped_light_room_id: Uuid) -> anyhow::Result<()> {
        let url = self.build_grouped_light_url(grouped_light_room_id);
        let request_body = GroupedLightPutBody::builder().on(LightGroupOn::OFF).build();

        let response = self.http_client.put(url).json(&request_body).send().await?;
        debug!("got turn_off response: {:?}", response);
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

    #[instrument(level = "debug")]
    pub async fn recall_scene(&self, scene_id: &Uuid, brightness: Option<f32>) -> Result<()> {
        let url = self
            .base_url
            .join(format!("scene/{}", scene_id).as_str())
            .expect("building the scene recall URL should not fail");

        let body = RecallSceneBody::new(brightness);

        let response = self.http_client.put(url).json(&body).send().await?;
        debug!("got recall_scene response: {:?}", response);
        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await?;
            error!(
                "there was a problem recalling the scene {}. status: {}, body: {}",
                scene_id, status, response_body
            );
            bail!(
                "there was a problem recalling the scene {}. status: {}, body: {}",
                scene_id,
                status,
                response_body
            )
        }
        Ok(())
    }
}
