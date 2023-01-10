use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use caseta_listener::client::room_state::new_cache;
use tokio::sync::mpsc;
use tracing::subscriber::set_global_default;
use tracing::{debug, error, info, instrument, warn};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use caseta_listener::caseta::connection::{
    CasetaConnection, CasetaConnectionError, DefaultTcpSocketProvider,
};
use caseta_listener::caseta::message::Message;
use caseta_listener::caseta::remote::{remote_watcher_loop, RemoteWatcher};
use caseta_listener::client::dispatcher::{dispatcher_loop, DeviceActionDispatcher};
use caseta_listener::client::hue::HueClient;
use caseta_listener::config::caseta_auth_configuration::get_caseta_auth_configuration;
use caseta_listener::config::caseta_remote::{
    get_caseta_remote_configuration, ButtonAction, CasetaRemote, RemoteConfiguration, RemoteId,
};
use caseta_listener::config::hue_auth_configuration::get_hue_auth_configuration;
use caseta_listener::config::scene::{get_room_configurations, HomeConfiguration, Topology};

type RemoteWatcherDb = HashMap<RemoteId, Arc<RemoteWatcher>>;

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let formatting_layer = BunyanFormattingLayer::new("caseta_listener".into(), std::io::stdout);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    set_global_default(subscriber).expect("Failed to set subscriber");
    watch_caseta_events().await
}

#[instrument]
async fn watch_caseta_events() -> Result<()> {
    let caseta_hub_settings = get_caseta_auth_configuration().unwrap();
    let caseta_remote_configuration = get_caseta_remote_configuration().unwrap();
    let home_scene_configuration = get_room_configurations().unwrap();
    let hue_auth_configuration = get_hue_auth_configuration().unwrap();
    let topology = Arc::new(build_topology(
        caseta_remote_configuration,
        home_scene_configuration,
    ));

    let caseta_address = caseta_hub_settings.caseta_host.clone();
    let port = caseta_hub_settings.caseta_port;
    let tcp_socket_provider = DefaultTcpSocketProvider::new(caseta_address, port);
    let mut connection = CasetaConnection::new(caseta_hub_settings, &tcp_socket_provider);
    connection.initialize().await?;

    let (action_sender, action_receiver) = mpsc::channel(64);
    let mut remote_watchers: RemoteWatcherDb = HashMap::new();
    let hue_client = HueClient::new(
        hue_auth_configuration.host,
        hue_auth_configuration.application_key,
    );
    let dispatcher = Arc::new(DeviceActionDispatcher::new(
        hue_client,
        topology.clone(),
        Arc::new(new_cache()),
    ));
    tokio::spawn(dispatcher_loop(dispatcher, action_receiver));
    loop {
        let contents = connection.await_message().await;
        match contents {
            Ok(Message::ButtonEvent {
                remote_id,
                button_id,
                button_action,
            }) => {
                let button_key = format!("{}-{}-{}", remote_id, button_id, button_action);
                let (_remote, room) = topology.get(&remote_id).expect(
                    format!("there must be configuration for this remote {}", remote_id).as_str(),
                );
                debug!(
                    remote_id=%remote_id,
                    button_id=%button_id,
                    button_action=%button_action,
                    button_key=button_key.as_str(),
                    "Observed a button event: {}, room: {}",
                    button_key,
                    room.name
                );

                match remote_watchers.entry(remote_id) {
                    Entry::Occupied(mut entry) => {
                        let remote_watcher = entry.get();
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        if remote_history.is_finished() {
                            if let ButtonAction::Release = button_action {
                                debug!("we saw a ButtonAction::Release for an initial button action, so we're ignoring it");
                                continue;
                            }
                            let remote_watcher = Arc::new(RemoteWatcher::new(
                                remote_id,
                                button_id,
                                action_sender.clone(),
                            ));
                            remote_watcher
                                .remote_history
                                .lock()
                                .unwrap()
                                .increment(&button_id, &button_action)
                                .unwrap();
                            entry.insert(remote_watcher.clone());
                            tokio::spawn(remote_watcher_loop(remote_watcher));
                        } else {
                            remote_history
                                .increment(&button_id, &button_action)
                                .unwrap()
                        }
                    }
                    Entry::Vacant(entry) => {
                        if let ButtonAction::Release = button_action {
                            debug!("we saw a ButtonAction::Release for an initial button action, so we're ignoring it");
                            continue;
                        }
                        let remote_watcher = Arc::new(RemoteWatcher::new(
                            remote_id,
                            button_id,
                            action_sender.clone(),
                        ));
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        remote_history
                            .increment(&button_id, &button_action)
                            .unwrap();
                        entry.insert(remote_watcher.clone());
                        tokio::spawn(remote_watcher_loop(remote_watcher));
                    }
                }
            }
            Ok(unexpected_contents) => {
                warn!(message_contents=%unexpected_contents, "got an unexpected message type: {}", unexpected_contents)
            }
            Err(CasetaConnectionError::Disconnected) => {
                info!("looks like our caseta connection was disconnected, so we're gonna create a new one!");
                connection = CasetaConnection::new(
                    get_caseta_auth_configuration().unwrap(),
                    &tcp_socket_provider,
                );
                connection.initialize().await?;
            }
            Err(other_caseta_connection_err) => {
                error!(caseta_connection_error=%other_caseta_connection_err, "there was a problem with the caseta connection");
                break Err(anyhow!(
                    "there was an issue with the caseta connection {:?} ",
                    other_caseta_connection_err
                ));
            }
        }
    }
}

fn build_topology(
    caseta_remote_configuration: RemoteConfiguration,
    home_configuration: HomeConfiguration,
) -> Topology {
    let mut remotes_by_remote_id: HashMap<RemoteId, CasetaRemote> = HashMap::new();
    for remote in caseta_remote_configuration.remotes.iter() {
        match remote {
            CasetaRemote::TwoButtonPico { id, .. } => {
                remotes_by_remote_id.insert(*id, remote.clone());
            }
            CasetaRemote::FiveButtonPico { id, .. } => {
                remotes_by_remote_id.insert(*id, remote.clone());
            }
        }
    }

    let mut topology: Topology = HashMap::new();
    for room in home_configuration.rooms.iter() {
        for remote_id in room.remotes.iter() {
            let remote = remotes_by_remote_id
                .get(&remote_id)
                .expect(format!("no remote with id {} in our configuration", *remote_id).as_str());
            topology.insert(*remote_id, (remote.clone(), room.clone()));
        }
    }

    topology
}
