use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use anyhow::{anyhow, Result, Ok};
use futures::future::join_all;
use itertools::Itertools;
use moka::future::Cache;
use reqwest::{Client, Url};
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::{instrument, debug};
use url::Host;

use uuid::Uuid;
use crate::client::model::hue::{LightGroup, HueReference};
use crate::config::scene::Room;

use super::model::hue::{HueLightResponse, HueLight, HueRoomResponse, HueRoom};

const HUE_AUTH_KEY_HEADER: &str = "hue-application-key";
type RoomIdAndLights = HashMap<Uuid, Vec<HueLight>>;
#[derive(Debug)]
pub struct HueClient {
    base_url: Url,
    auth_key: String,
    http_client: Client,
    room_id_and_lights_cache: Cache<Uuid, Vec<HueLight>>
}

impl HueClient {
    pub fn new(host: Host, auth_key: String, cache: Cache<Uuid, Vec<HueLight>>) -> HueClient {
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
            http_client,
            room_id_and_lights_cache: cache
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

    pub async fn get_lights_in_room(&self, room_id: Uuid) -> Result<Vec<HueLight>> {
        let cached_lights_entry = self.room_id_and_lights_cache.get(&room_id);

        match cached_lights_entry {
            Some(lights) => {
                return Ok(lights);
            }
            None => {
                let lights_map = self.get_lights().await?;
                let futures = lights_map.into_iter().map(|entry| {
                    let (id, lights) = entry;
                    self.room_id_and_lights_cache.insert(id, lights)
                }).collect_vec();
                join_all(futures).await;

                return Ok(self.room_id_and_lights_cache.get(&room_id).unwrap());
            }
        }
    }

    #[instrument]
    pub async fn get_lights(&self) -> anyhow::Result<RoomIdAndLights> {
        let url = self.base_url.join("light").expect("this should always be a well formed URL");
        let response = self.http_client.get(url).send()
            .await?;
        let result = response.json::<HueLightResponse>()
            .await?;
        
        let mut lights_by_device_id: HashMap<Uuid, HueLight> = HashMap::from_iter(
            result.data.into_iter().map(|light|{
                if let HueReference::Device(id) = light.owner {
                    return (id, light);
                } else {
                    panic!("all lights should be owned by a device")
                }
            })
         );

        let rooms = self.get_rooms().await?;
        // I want map<room_id, vec<hue_light>>
        // I have: map<room_id, set<device_id>>
        // I have: map<device_id, hue_light>
        let rooms_and_their_devices: HashMap<Uuid, Vec<Uuid>> = HashMap::from_iter(
            rooms.into_iter().map(|(id, room)| {
                let device_ids = Vec::from_iter(room.children.iter().map(|child| {
                    if let HueReference::Device(id) = child {
                        return *id;
                    }
                    panic!("this shouldn't happen")
                }));

                (id, device_ids)
            })
         );

        let rooms_and_their_lights: HashMap<Uuid, Vec<HueLight>> = HashMap::from_iter(
            rooms_and_their_devices.into_iter().map(|(room_id, device_ids)| {
                let lights = Vec::from_iter(
                    device_ids.iter()
                        .map(|device_id| 
                            lights_by_device_id.remove(&device_id).unwrap()
                        )
                    );
                return (room_id, lights);
            })
        );
        
        Ok(rooms_and_their_lights)
    }

    pub async fn get_rooms(&self) -> Result<HashMap<Uuid, HueRoom>> {
        let url = self.base_url.join("room").expect("this should always be a well formed URL");
        let response = self.http_client.get(url).send()
            .await.unwrap();

        let rooms = response.json::<HueRoomResponse>().await?;
        let mut rooms_by_id: HashMap<Uuid, HueRoom> = HashMap::new();
        rooms.data.iter().for_each(|room| { rooms_by_id.insert(room.id, room.clone()); });
        Ok(rooms_by_id)
    }
}
