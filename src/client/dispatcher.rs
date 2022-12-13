use crate::client::hue::HueClient;
use crate::client::scene_state::CurrentSceneEntry;
use crate::config::caseta_remote::{ButtonId, CasetaRemote, RemoteId};
use crate::config::scene::{Room, Scene, Topology};
use anyhow::{bail, ensure, Ok, Result};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::model::hue::{GroupedLight, HueResponse};
use super::scene_state::CurrentSceneCache;

#[derive(Debug, Copy, Clone)]
pub enum DeviceAction {
    SinglePressComplete,
    DoublePressComplete,
    LongPressStart,
    LongPressOngoing,
    LongPressComplete,
}

#[derive(Debug, Copy, Clone)]
pub struct DeviceActionMessage {
    device_action: DeviceAction,
    remote_id: RemoteId,
    button_id: ButtonId,
}

impl DeviceActionMessage {
    pub fn new(device_action: DeviceAction, remote_id: RemoteId, button_id: ButtonId) -> Self {
        DeviceActionMessage {
            device_action,
            remote_id,
            button_id,
        }
    }
}

pub struct DeviceActionDispatcher {
    message_receiver: Receiver<DeviceActionMessage>,
    hue_client: HueClient,
    topology: Arc<Topology>,
    current_scene_cache: Arc<CurrentSceneCache>,
}

impl DeviceActionDispatcher {
    pub fn new(
        message_receiver: Receiver<DeviceActionMessage>,
        hue_client: HueClient,
        topology: Arc<Topology>,
        current_scene_cache: Arc<CurrentSceneCache>,
    ) -> DeviceActionDispatcher {
        DeviceActionDispatcher {
            message_receiver,
            hue_client,
            topology,
            current_scene_cache,
        }
    }

    async fn get_current_scene(&self, room: &Room) -> Result<CurrentSceneEntry> {
        let cache_entry = self.current_scene_cache.get(&room.room_id);
        return match cache_entry {
            Some(entry) => Ok(entry),
            None => {
                let first_scene = room
                    .scenes
                    .first()
                    .expect("configurations must have at least one scene");
                let grouped_light_response = self
                    .hue_client
                    .get_grouped_light(room.grouped_light_room_id)
                    .await?;
                Ok(Self::build_cache_entry(
                    &first_scene,
                    &grouped_light_response,
                ))
            }
        };
    }

    fn cache_current_scene(&self, room_id: Uuid, current_scene_entry: CurrentSceneEntry) {
        self.current_scene_cache
            .insert(room_id, current_scene_entry)
    }

    fn get_first_scene(&self, remote_id: &RemoteId) -> &Scene {
        let (_, room) = self
            .topology
            .get(remote_id)
            .expect(format!("no configuration present for remote {}", remote_id).as_str());
        room.scenes
            .first()
            .expect("there should be at least one scene configured for every room")
    }

    fn build_cache_entry(
        scene: &Scene,
        grouped_light_response: &HueResponse<GroupedLight>,
    ) -> CurrentSceneEntry {
        let grouped_light = grouped_light_response
            .data
            .first()
            .expect("there should be a single grouped light in this response");
        let brightness = match grouped_light.on.on {
            true => Some(grouped_light.dimming.brightness),
            false => None,
        };
        CurrentSceneEntry::new(scene.clone(), brightness, grouped_light.on.on)
    }

    async fn handle_power_on_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        ensure!(message.button_id == ButtonId::PowerOn);
        let topology = &self.topology;

        let (remote, room) = topology
            .get(&message.remote_id)
            .expect(format!("no configuration present for remote {}", message.remote_id).as_str());
        match remote {
            CasetaRemote::TwoButtonPico { .. } => {
                bail!("we haven't implemented 2 button picos yet")
            }
            _ => (),
        }

        let current_scene = self.get_current_scene(room).await?;

        match message.device_action {
            DeviceAction::SinglePressComplete => {
                debug!("got a single press for remote in room {}", room.name);
                if !current_scene.on {
                    let current_light_status = self
                        .hue_client
                        .turn_on(room.grouped_light_room_id, Option::None)
                        .await?;
                    let scene = self.get_first_scene(&message.remote_id);
                    self.cache_current_scene(
                        room.room_id,
                        Self::build_cache_entry(scene, &current_light_status),
                    )
                }
            }
            DeviceAction::DoublePressComplete => {
                debug!("got a double press for remote in room {}", room.name)
            }
            DeviceAction::LongPressStart => {
                debug!("a long press has started in room {}", room.name)
            }
            DeviceAction::LongPressOngoing => {
                debug!("a long press is still ongoing in room {}", room.name)
            }
            DeviceAction::LongPressComplete => {
                debug!("our long press in {} is complete", room.name)
            }
        }

        Ok(())
    }

    async fn handle_power_off_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        ensure!(message.button_id == ButtonId::PowerOff);
        let topology = &self.topology;
        let (remote, room) = topology
            .get(&message.remote_id)
            .expect(format!("no configuration present for remote {}", message.remote_id).as_str());
        match remote {
            CasetaRemote::TwoButtonPico { .. } => bail!("two button picos are not supported yet"),
            _ => (),
        }

        let current_scene = self.get_current_scene(room).await?;
        match message.device_action {
            DeviceAction::SinglePressComplete
            | DeviceAction::DoublePressComplete
            | DeviceAction::LongPressComplete => {
                self.hue_client.turn_off(room.grouped_light_room_id).await?;
                let mut turned_off_scene = current_scene.clone();
                turned_off_scene.on = false;
                self.cache_current_scene(room.room_id, turned_off_scene);
            }
            DeviceAction::LongPressStart | DeviceAction::LongPressOngoing => (),
        }
        Ok(())
    }
}

#[instrument(skip(dispatcher))]
pub async fn dispatcher_loop(mut dispatcher: DeviceActionDispatcher) -> Result<()> {
    loop {
        let message = dispatcher.message_receiver.recv().await.unwrap();
        let (_caseta_remote, room) = dispatcher.topology.get(&message.remote_id).unwrap();
        match message.button_id {
            ButtonId::PowerOn => dispatcher.handle_power_on_button_press(message).await?,
            ButtonId::Up => todo!(),
            ButtonId::Favorite => todo!(),
            ButtonId::Down => todo!(),
            ButtonId::PowerOff => dispatcher.handle_power_off_button_press(message).await?,
        }

        match message.device_action {
            DeviceAction::SinglePressComplete => {
                // let content = dispatcher.hue_client.get_room_status(room.grouped_light_room_id).await.unwrap();
                // debug!(content=?content, "got some content from the hue api");
                let content = dispatcher
                    .hue_client
                    .get_grouped_light(room.grouped_light_room_id)
                    .await
                    .unwrap();
                debug!(content=?content, "got some content from the hue api");
            }
            _ => {}
        }
    }
}
