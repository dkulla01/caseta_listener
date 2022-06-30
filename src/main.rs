use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::time::sleep;
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{EnvFilter, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing::{debug, error, info, instrument, warn};

use caseta_listener::caseta::{ButtonAction, ButtonState, DefaultTcpSocketProvider, RemoteId, RemoteWatcher};
use caseta_listener::caseta::Message::ButtonEvent;
use caseta_listener::caseta::{CasetaConnection, CasetaConnectionError};
use caseta_listener::configuration::get_caseta_hub_settings;
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);
const REMOTE_WATCHER_LOOP_SLEEP_DURATION: Duration = Duration::from_millis(500);

type RemoteWatcherDb = HashMap<RemoteId, Arc<RemoteWatcher>>;

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let formatting_layer = BunyanFormattingLayer::new(
        "caseta_listener".into(),
        std::io::stdout
    );

    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    set_global_default(subscriber).expect("Failed to set subscriber");
    watch_caseta_events().await
}

#[instrument]
async fn watch_caseta_events() -> Result<()> {
    let caseta_hub_settings = get_caseta_hub_settings().unwrap();

    let caseta_address = caseta_hub_settings.caseta_host;
    let port = caseta_hub_settings.caseta_port;
    let tcp_socket_provider = DefaultTcpSocketProvider::new(caseta_address, port);
    let mut connection = CasetaConnection::new(caseta_hub_settings, &tcp_socket_provider);
    connection.initialize()
        .await?;

    let mut remote_watchers : RemoteWatcherDb = HashMap::new();

    loop {
        let contents = connection.await_message().await;
        match contents {
            Ok(ButtonEvent { remote_id, button_id, button_action }) => {
                let button_key = format!("{}-{}-{}", remote_id, button_id, button_action);
                debug!(
                    remote_id=%remote_id,
                    button_id=%button_id,
                    button_action=%button_action,
                    button_key=button_key.as_str(),
                    "Observed a button event: {}",
                    button_key
                );

                match remote_watchers.entry(remote_id) {
                    Entry::Occupied(mut entry) => {
                        let remote_watcher = entry.get();
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        if remote_history.is_finished() {
                            let remote_watcher = Arc::new(RemoteWatcher::new(remote_id, button_id));
                            remote_watcher.remote_history.lock().unwrap().increment(button_id, &button_action);
                            entry.insert(remote_watcher.clone());
                            tokio::spawn(remote_watcher_loop(remote_watcher));
                        } else {
                            remote_history.increment(button_id, &button_action)
                        }
                    }
                    Entry::Vacant(entry) => {
                        if let ButtonAction::Release = button_action {
                            continue
                        }
                        let remote_watcher = Arc::new(RemoteWatcher::new(remote_id, button_id));
                        let remote_history = remote_watcher.remote_history.clone();
                        let mut remote_history = remote_history.lock().unwrap();
                        remote_history.increment(button_id, &button_action);
                        entry.insert(remote_watcher.clone());
                        tokio::spawn(remote_watcher_loop(remote_watcher));
                    }
                }
            },
            Ok(unexpected_contents) => warn!(message_contents=%unexpected_contents, "got an unexpected message type: {}", unexpected_contents),
            Err(CasetaConnectionError::Disconnected) => {
                info!("looks like our caseta connection was disconnected, so we're gonna create a new one!");
                connection = CasetaConnection::new(get_caseta_hub_settings().unwrap(), &tcp_socket_provider);
                connection.initialize().await?;
            }
            Err(other_caseta_connection_err) => {
                error!(caseta_connection_error=%other_caseta_connection_err, "there was a problem with the caseta connection");
                break Err(anyhow!("there was an issue with the caseta connection {:?} ", other_caseta_connection_err))
            }
        }
    }
}

#[instrument(skip(watcher), fields(remote_id=watcher.remote_id))]
async fn remote_watcher_loop(watcher: Arc<RemoteWatcher>) {
    let remote_id = watcher.remote_id;
    let button_id = watcher.button_id;
    debug!(remote_id=remote_id, "started tracking remote");
    sleep(DOUBLE_CLICK_WINDOW).await;

    {
        let history = watcher.remote_history.clone();
        let mut locked_history = history.lock().unwrap();
        let button_state = &locked_history.button_state;

        debug!(remote_id=remote_id, button_id=%button_id, "first pass at evaluating button state");
        if button_state.is_some() {
            let button_state = button_state.as_ref().unwrap();
            match button_state {
                ButtonState::FirstPressAwaitingRelease => {
                    // perform the long press started action here
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "a long press has started but not finished");
                }
                ButtonState::FirstPressAndFirstRelease => {
                    // perform the single press action
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "a single press has finished");
                    locked_history.finished = true;
                }
                ButtonState::SecondPressAwaitingRelease => {
                    // this is kind of a no-op -- we're waiting for this button to be released so that
                    // we can perform a double press action
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "we're waiting for a double press to finish");
                }
                ButtonState::SecondPressAndSecondRelease => {
                    //perform the double press action
                    debug!(remote_id=remote_id, button_id=%button_id, button_state=%button_state, "a double press has finished");
                    locked_history.finished = true;
                }
            }
        } else {
            warn!(remote_id=remote_id, button_id=%button_id, "there was no initial button state for this button, which is unusual to say the least")
            // todo: should this be an exceptional condition that short-circuits?
        }
        if locked_history.is_finished() {
            return;
        }
    }

    loop {
        sleep(REMOTE_WATCHER_LOOP_SLEEP_DURATION).await;
        let history = watcher.remote_history.clone();
        let mut locked_history = history.lock().unwrap();
        let button_state = locked_history.button_state.as_ref().expect("button state should have been set by now.");
        match button_state {
            ButtonState::FirstPressAndFirstRelease => {
                // a long press has finished here;
                locked_history.finished = true;
                debug!(remote_id=%remote_id, button_id=%button_id, "a long press has just finished")
            }
            ButtonState::FirstPressAwaitingRelease => {
                // a long press is still ongoing here. continue onward
                debug!(remote_id=%remote_id, button_id=%button_id, "a long press is still ongoing here");
                // there might be action depending on the button. E.G. do we increase/decrease the lights?
            }
            ButtonState::SecondPressAwaitingRelease => {
                // a double press is still ongoing here. we're just waiting for the release, so nothing to see here.
            }
            ButtonState::SecondPressAndSecondRelease => {
                // a double press has finished here!
                locked_history.finished = true;
                debug!(remote_id=%remote_id, button_id=%button_id, "a double press has just finished")
            }
        }
        if locked_history.is_finished() {
            return
        }
    }
}
