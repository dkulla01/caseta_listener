use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use caseta_listener::caseta::connection::{
    CasetaConnectionProvider, DefaultCasetaConnectionProvider, DefaultTcpSocketProvider,
    DelegatingCasetaConnectionManager, ReadOnlyConnection,
};
use caseta_listener::client::room_state::new_cache;
use tokio::sync::mpsc;
use tracing::subscriber::set_global_default;
use tracing::{debug, error, info, instrument, warn};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

use caseta_listener::caseta::message::Message;
use caseta_listener::caseta::remote::{remote_watcher_loop, RemoteWatcher};
use caseta_listener::client::dispatcher::{dispatcher_loop, DeviceActionDispatcher};
use caseta_listener::client::hue::HueClient;
use caseta_listener::config::auth_configuration::get_auth_configuration;
use caseta_listener::config::caseta_remote::{
    get_caseta_remote_configuration, ButtonAction, CasetaRemote, RemoteConfiguration, RemoteId,
};
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
    let auth_configuration = get_auth_configuration().unwrap();
    let caseta_remote_configuration = get_caseta_remote_configuration().unwrap();
    let home_scene_configuration = get_room_configurations().unwrap();
    let topology = Arc::new(build_topology(
        caseta_remote_configuration,
        home_scene_configuration,
    ));

    let caseta_address = auth_configuration.caseta_host.clone();
    let port = auth_configuration.caseta_port;

    let tcp_socket_provider = Box::new(DefaultTcpSocketProvider::new(caseta_address, port));
    let connection_manager_provider = DefaultCasetaConnectionProvider::new(
        auth_configuration.caseta_username,
        auth_configuration.caseta_password,
        tcp_socket_provider,
    );
    let mut connection =
        DelegatingCasetaConnectionManager::new(Box::new(connection_manager_provider));

    let (action_sender, action_receiver) = mpsc::channel(64);
    let mut remote_watchers: RemoteWatcherDb = HashMap::new();
    let hue_host = auth_configuration.hue_host;
    let hue_application_key = auth_configuration.hue_application_key;
    let hue_client = HueClient::new(hue_host, hue_application_key);
    let dispatcher = Arc::new(DeviceActionDispatcher::new(
        hue_client,
        topology.clone(),
        Arc::new(new_cache()),
    ));
    tokio::spawn(dispatcher_loop(dispatcher, action_receiver));
    loop {
        let contents = connection.await_message().await;
        match contents {
            Ok(Some(Message::ButtonEvent {
                remote_id,
                button_id,
                button_action,
            })) => {
                let button_key = format!("{}-{}-{}", remote_id, button_id, button_action);
                let room_configuration = topology.get(&remote_id);
                if let None = room_configuration {
                    info!(
                        "ignoring unconfigured remote {{id: {}: button_action: {}}}",
                        remote_id, button_action
                    );
                    continue;
                }
                let (_remote, room) = room_configuration.unwrap();
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
            Ok(Some(unexpected_contents)) => {
                warn!(message_contents=%unexpected_contents, "got an unexpected message type: {}", unexpected_contents)
            }
            Ok(None) => {
                error!("received an empty message, which shouldn't happen in the main loop");
                bail!("empty messages shouldn't crop up here");
            }
            Err(connection_manager_error) => {
                error!(caseta_connection_error=%connection_manager_error, "there was a problem with the caseta connection");
                break Err(anyhow!(
                    "there was an issue with the caseta connection {:?} ",
                    connection_manager_error
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
