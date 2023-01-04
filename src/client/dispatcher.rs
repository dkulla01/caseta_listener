use crate::client::hue::HueClient;
use crate::client::room_state::CurrentRoomState;
use crate::config::caseta_remote::{ButtonId, CasetaRemote, RemoteId};
use crate::config::scene::{Device, Room, Scene, Topology};
use anyhow::{bail, ensure, Ok, Result};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::model::hue::{GroupedLight, HueResponse};
use super::room_state::CurrentRoomStateCache;

const BRIGHTNESS_UPDATE_AMOUNT: f32 = 5.0;
const MAXIMUM_BRIGHTNESS_PERCENT: f32 = 100.0;
const MINIMUM_BRIGHTNESS_PERCENT: f32 = 1.0;

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
    hue_client: HueClient,
    topology: Arc<Topology>,
    current_scene_cache: Arc<CurrentRoomStateCache>,
}

impl DeviceActionDispatcher {
    pub fn new(
        hue_client: HueClient,
        topology: Arc<Topology>,
        current_scene_cache: Arc<CurrentRoomStateCache>,
    ) -> DeviceActionDispatcher {
        DeviceActionDispatcher {
            hue_client,
            topology,
            current_scene_cache,
        }
    }

    async fn get_current_state(&self, room: &Room) -> Result<CurrentRoomState> {
        let cache_entry = self.current_scene_cache.get(&room.room_id);
        return match cache_entry {
            Some(entry) => Ok(entry),
            None => {
                let grouped_light_response = self
                    .hue_client
                    .get_grouped_light(room.grouped_light_room_id)
                    .await?;
                Ok(Self::build_cache_entry(
                    Option::None,
                    &grouped_light_response,
                ))
            }
        };
    }

    fn cache_current_state(&self, room_id: Uuid, current_room_state: CurrentRoomState) {
        self.current_scene_cache.insert(room_id, current_room_state)
    }

    fn build_cache_entry(
        scene: Option<Scene>,
        grouped_light_response: &HueResponse<GroupedLight>,
    ) -> CurrentRoomState {
        let grouped_light = grouped_light_response
            .data
            .first()
            .expect("there should be a single grouped light in this response");
        let brightness = match grouped_light.on.on {
            true => Some(grouped_light.dimming.brightness),
            false => None,
        };
        CurrentRoomState::new(scene, brightness, grouped_light.on.on)
    }

    fn get_room_configuration(&self, remote_id: u8) -> (&CasetaRemote, &Room) {
        let (remote, room) = &self
            .topology
            .get(&remote_id)
            .expect(format!("no configuration present for remote {}", remote_id).as_str());

        return (remote, room);
    }

    fn get_bounded_next_higher_brightness_val(current_value: f32) -> f32 {
        let quotient = (current_value / BRIGHTNESS_UPDATE_AMOUNT).trunc();
        let next_higher_value = BRIGHTNESS_UPDATE_AMOUNT * (quotient + 1.0);
        f32::min(MAXIMUM_BRIGHTNESS_PERCENT, next_higher_value)
    }

    fn get_bounded_next_lower_brightness_val(current_value: f32) -> f32 {
        let quotient = (current_value / BRIGHTNESS_UPDATE_AMOUNT).trunc();
        let next_lower_value = BRIGHTNESS_UPDATE_AMOUNT * (quotient - 1.0);
        f32::max(MINIMUM_BRIGHTNESS_PERCENT, next_lower_value)
    }

    async fn handle_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        match message.button_id {
            ButtonId::PowerOn => self.handle_power_on_button_press(message).await,
            ButtonId::Up => self.handle_up_button_press(message).await,
            ButtonId::Favorite => self.handle_favorite_button_press(message).await,
            ButtonId::Down => self.handle_down_button_press(message).await,
            ButtonId::PowerOff => self.handle_power_off_button_press(message).await,
        }
    }

    async fn handle_power_on_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        ensure!(message.button_id == ButtonId::PowerOn);
        let (remote, room) = self.get_room_configuration(message.remote_id);
        match remote {
            CasetaRemote::TwoButtonPico { .. } => {
                bail!("we haven't implemented 2 button picos yet")
            }
            _ => (),
        }

        let current_room_state = self.get_current_state(room).await?;

        match message.device_action {
            DeviceAction::SinglePressComplete => {
                debug!("got a single press for remote in room {}", room.name);
                if !current_room_state.on {
                    let current_light_status =
                        self.hue_client.turn_on(room.grouped_light_room_id).await?;
                    self.cache_current_state(
                        room.room_id,
                        Self::build_cache_entry(current_room_state.scene, &current_light_status),
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
        let (remote, room) = self.get_room_configuration(message.remote_id);
        match remote {
            CasetaRemote::TwoButtonPico { .. } => bail!("two button picos are not supported yet"),
            _ => (),
        }

        let current_room_state = self.get_current_state(room).await?;
        match message.device_action {
            DeviceAction::SinglePressComplete
            | DeviceAction::DoublePressComplete
            | DeviceAction::LongPressComplete => {
                self.hue_client.turn_off(room.grouped_light_room_id).await?;
                let mut turned_off_scene = current_room_state.clone();
                turned_off_scene.on = false;
                self.cache_current_state(room.room_id, turned_off_scene);
            }
            DeviceAction::LongPressStart | DeviceAction::LongPressOngoing => (),
        }
        Ok(())
    }

    async fn handle_up_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        ensure!(message.button_id == ButtonId::Up);
        let (_remote, room) = self.get_room_configuration(message.remote_id);
        let current_room_state = self.get_current_state(room).await?;

        if !current_room_state.on {
            // can't increase brightness of a room that's off
            // todo: maybe a double press when it's off turns the lights on to full brightness?
            return Ok(());
        }

        self.handle_brightness_change_button_press(
            message,
            room,
            current_room_state,
            Self::get_bounded_next_higher_brightness_val,
        )
        .await
    }

    async fn handle_down_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        ensure!(message.button_id == ButtonId::Down);
        let (_remote, room) = self.get_room_configuration(message.remote_id);
        let current_room_state = self.get_current_state(room).await?;

        if !current_room_state.on {
            // can't decrease brightness of a room that's off
            // todo: maybe a double press when it's off turns the lights on to minimum brightness?
            return Ok(());
        }

        self.handle_brightness_change_button_press(
            message,
            room,
            current_room_state,
            Self::get_bounded_next_lower_brightness_val,
        )
        .await
    }

    async fn handle_brightness_change_button_press(
        &self,
        message: DeviceActionMessage,
        room: &Room,
        current_room_state: CurrentRoomState,
        update_fn: fn(f32) -> f32,
    ) -> Result<()> {
        if !current_room_state.on {
            // can't update brightness of a room that's off
            // todo: maybe a double press when it's off turns the lights on to full brightness?
            return Ok(());
        }

        let mut target_brightness = current_room_state.brightness.expect(
            format!(
                "room {} is on, but its brightness is not specified",
                room.name
            )
            .as_str(),
        );
        match message.device_action {
            DeviceAction::SinglePressComplete
            | DeviceAction::LongPressStart
            | DeviceAction::LongPressOngoing => {
                target_brightness = update_fn(target_brightness);
            }
            DeviceAction::DoublePressComplete => {
                let intermediate_brightness = update_fn(target_brightness);
                target_brightness = update_fn(intermediate_brightness);
            }
            DeviceAction::LongPressComplete => {
                // no op here. the long press is over, so there's no update needed
            }
        }

        self.hue_client
            .update_brightness(room.grouped_light_room_id, target_brightness)
            .await?;
        let mut new_room_state = current_room_state.clone();
        new_room_state.brightness = Some(target_brightness);
        self.cache_current_state(room.room_id, new_room_state);
        Ok(())
    }

    async fn handle_favorite_button_press(&self, message: DeviceActionMessage) -> Result<()> {
        ensure!(message.button_id == ButtonId::Favorite);
        let (remote, room) = self.get_room_configuration(message.remote_id);
        match remote {
            CasetaRemote::TwoButtonPico { .. } => {
                bail!("two button picos don't have favorite buttons")
            }
            _ => (),
        }
        let mut current_room_state = self.get_current_state(room).await?;
        if !current_room_state.on {
            // don't do anything to the scene if the lights in the room aren't on
            return Ok(());
        }

        let brightness = current_room_state
            .brightness
            .expect("rooms that are on must have a brightness value associated with them");
        let target_scene;

        if current_room_state.scene.is_none() {
            target_scene = Self::get_first_scene(room);
        } else {
            let current_scene = current_room_state.scene.unwrap();
            match message.device_action {
                DeviceAction::SinglePressComplete => {
                    target_scene = Self::get_next_scene(room, &current_scene);
                }
                DeviceAction::DoublePressComplete => {
                    target_scene = Self::get_previous_scene(room, &current_scene);
                }
                DeviceAction::LongPressComplete => target_scene = Self::get_first_scene(room),
                DeviceAction::LongPressStart | DeviceAction::LongPressOngoing => return Ok(()), // no actions to take for non-terminal long press states
            }
        }
        // here is where we'd update the hue device
        for device in target_scene.devices.iter() {
            if let Device::HueScene { id, name } = device {
                debug!(
                    "updating the hue scene to {} at brightness level {}",
                    name, brightness
                );
                let _response = self.hue_client.recall_scene(id, brightness).await?;
            }
        }
        current_room_state.scene = Option::Some(target_scene.clone());
        self.cache_current_state(room.room_id, current_room_state);

        Ok(())
    }

    fn get_scene_index(room: &Room, current_scene: &Scene) -> usize {
        room.scenes
            .iter()
            .position(|scene| scene.name == current_scene.name)
            .expect(
                format!(
                    "scene {} should be present in configuration, but it was not",
                    current_scene.name
                )
                .as_str(),
            )
    }

    fn get_next_scene<'a>(room: &'a Room, current_scene: &Scene) -> &'a Scene {
        let position = Self::get_scene_index(room, current_scene);
        let scene_count = room.scenes.len();
        room.scenes.get((position + 1) % scene_count).unwrap()
    }

    fn get_previous_scene<'a>(room: &'a Room, current_scene: &Scene) -> &'a Scene {
        let position = Self::get_scene_index(room, current_scene);
        let scene_count = room.scenes.len();
        room.scenes.get((position - 1) % scene_count).unwrap()
    }

    fn get_first_scene<'a>(room: &'a Room) -> &'a Scene {
        room.scenes
            .first()
            .expect("Rooms must be configured with at least one scene")
    }
}

#[instrument(skip(dispatcher))]
pub async fn dispatcher_loop(
    dispatcher: Arc<DeviceActionDispatcher>,
    mut action_receiver: Receiver<DeviceActionMessage>,
) -> Result<()> {
    loop {
        let message = action_receiver.recv().await.unwrap();
        let dispatcher_instance = dispatcher.clone();
        tokio::spawn(async move {
            dispatcher_instance
                .clone()
                .handle_button_press(message)
                .await
        });
    }
}
