use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tracing::{debug, instrument};
use crate::client::hue::HueClient;
use crate::config::caseta_remote::{ButtonId, CasetaRemote, RemoteId};
use crate::config::scene::{Room, Topology};

#[derive(Debug, Copy, Clone)]
pub enum DeviceAction {
    SinglePressComplete,
    DoublePressComplete,
    LongPressStart,
    LongPressOngoing,
    LongPressComplete
}

#[derive(Debug, Copy, Clone)]
pub struct DeviceActionMessage {
    device_action: DeviceAction,
    remote_id: RemoteId,
    button_id: ButtonId
}

impl DeviceActionMessage {
    pub fn new(
        device_action: DeviceAction,
        remote_id: RemoteId,
        button_id: ButtonId
    ) -> Self {
        DeviceActionMessage {device_action, remote_id, button_id}
    }
}

pub struct DeviceActionDispatcher {
    message_receiver: Receiver<DeviceActionMessage>,
hue_client: HueClient,
    topology: Arc<Topology>

}

impl DeviceActionDispatcher {
    pub fn new(message_receiver: Receiver<DeviceActionMessage>, hue_client: HueClient, topology: Arc<Topology>) -> DeviceActionDispatcher {
        DeviceActionDispatcher{message_receiver, hue_client, topology}
    }
}

#[instrument(skip(dispatcher))]
pub async fn dispatcher_loop(mut dispatcher: DeviceActionDispatcher) {
    loop {
        let message = dispatcher.message_receiver.recv().await.unwrap();
        let (_caseta_remote, room) = dispatcher.topology.get(&message.remote_id).unwrap();

        match message.device_action {
            DeviceAction::SinglePressComplete => {
                // let content = dispatcher.hue_client.get_room_status(room.grouped_light_room_id).await.unwrap();
                // debug!(content=?content, "got some content from the hue api");
                let content = dispatcher.hue_client.get_lights_in_room(room.room_id).await.unwrap();
                debug!(content=?content, "got some content from the hue api");
            },
            _ => {}
        }
    }
}
