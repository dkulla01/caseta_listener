use std::collections::HashMap;
use std::env;
use config::{Config, ConfigError};
use crate::config::caseta_remote::{CasetaRemote, RemoteId};

use serde_derive::Deserialize;
use uuid::Uuid;

const SCENE_CONFIGURATION_FILE_NAME_ENV_VAR: &str = "CASETA_LISTENER_SCENE_CONFIG_FILE";
const DEFAULT_SCENE_CONFIGURATION_FILE_NAME: &str = "caseta_listener_scenes.yaml";

pub type Topology = HashMap<RemoteId, (CasetaRemote, Room)>;
pub type CurrentSceneCache = HashMap<Uuid, Vec<Device>>;


pub struct SceneCacheEntry {
    pub room_id: Uuid
}
#[derive(Deserialize, Debug)]
pub struct HomeConfiguration {
    pub rooms: Vec<Room>
}

#[derive(Deserialize, Debug, Clone)]
pub struct Room {
    pub name: String,
    pub room_id: Uuid,
    pub grouped_light_room_id: Uuid,
    pub scenes: Vec<Scene>,
    pub remotes: Vec<RemoteId>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Scene {
    // todo: need a way to convert this into a scene cache entry
    // and I'm not totally sure what that will look like
    name: String,
    devices: Vec<Device>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Device {
    HueScene {id: Uuid, name: String},
    NanoleafLightPanels {name: String, on: bool, effect: String},
    WemoOutlet {name: String, on: bool}
}
pub fn get_room_configurations() -> Result<HomeConfiguration, ConfigError> {
    let configuration_file_name = match env::var(SCENE_CONFIGURATION_FILE_NAME_ENV_VAR) {
        Ok(filename) => filename,
        _ => String::from(DEFAULT_SCENE_CONFIGURATION_FILE_NAME)
    };
    let home_configuration_builder = Config::builder()
        .add_source(config::File::with_name(configuration_file_name.as_str()));
    home_configuration_builder.build().unwrap().try_deserialize()
}

#[cfg(test)]
mod tests {
    use crate::config::scene::*;
    use spectral::prelude::*;

    #[test]
    fn it_deserializes() {
        let living_room_configuration = r#"
            name: "Living Room"
            room_id: 0c329b86-a7fb-4765-8fdd-2e87f37da685
            grouped_light_room_id: ba8c44e4-0229-4888-8eeb-ce4a3d48cca8
            remotes: [2, 3]
            scenes:
            - devices:
              - id: a3011bb2-dd50-4fd9-b143-7ea03f367088
                name: warm_reading_light_scene_0
                type: hue_scene
              - name: Fireplace
                'on': true
                type: wemo_outlet
              - name: "Office Shapes"
                internal_name: LightPanels 01:23:AF
                'on': true
                effect: "cozy red"
                type: nanoleaf_light_panels
              name: white_warmth
            "#;
        let room : Room = serde_yaml::from_str(living_room_configuration)
            .expect("unable to deserialize scene");
        assert_that(&room.name).is_equal_to(String::from("Living Room"));
        assert_that(&room.scenes).has_length(1);
        assert_that(&room.scenes[0].name).is_equal_to(String::from("white_warmth"));
        assert_that(&room.scenes[0].devices).has_length(3);

        assert!(matches!(room.scenes[0].devices[0], Device::HueScene{..}));
        assert!(matches!(room.scenes[0].devices[1], Device::WemoOutlet{..}));
        assert!(matches!(room.scenes[0].devices[2], Device::NanoleafLightPanels{..}));
    }
}
