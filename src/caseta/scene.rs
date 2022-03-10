use crate::caseta::message::RemoteId;

use serde_derive::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
struct Room {
    name: String,
    scenes: Vec<Scene>,
    remotes: Vec<RemoteId>,
}

#[derive(Deserialize, Debug)]
struct Scene {
    name: String,
    devices: Vec<Device>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Device {
    HueScene {id: Uuid, name: String},
    NanoleafLightPanels {name: String, on: bool, effect: String},
    WemoOutlet {name: String, on: bool}
}

#[cfg(test)]
mod tests {
    use crate::caseta::scene::*;
    use spectral::prelude::*;

    #[test]
    fn it_deserializes() {
        let living_room_configuration = r#"
            name: "Living Room"
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