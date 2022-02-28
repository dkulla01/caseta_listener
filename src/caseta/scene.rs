use crate::caseta::message::RemoteId;

use serde_derive::Deserialize;

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
    HueWhiteAndColorAmbiance {id: String, name: String, color: ColorSetting},
    NanoleafShapes {name: String, color: ColorSetting},
    WemoOutlet {name: String, on: bool}
}

#[derive(Deserialize, Debug)]
enum ColorSetting {
    #[serde(rename(deserialize = "xy"))]
    XYColor(XYColorCoordinates),

    #[serde(rename(deserialize = "scene"))]
    SceneNameColor(String)
}

#[derive(Deserialize, Debug)]
struct XYColorCoordinates {
    x: f32,
    y: f32
}

impl XYColorCoordinates {
    fn new(x: f32, y: f32) -> XYColorCoordinates {
        XYColorCoordinates {x, y}
    }
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
              - color:
                  xy:
                    x: 0.4575
                    y: 0.4099
                id: a3011bb2-dd50-4fd9-b143-7ea03f367088
                name: Ceiling
                type: hue_white_and_color_ambiance
              - name: Fireplace
                'on': true
                type: wemo_outlet
              - name: "Office Shapes"
                'on': true
                color:
                  scene: "cozy red"
                type: nanoleaf_shapes
              name: white_warmth
            "#;
        let room : Room = serde_yaml::from_str(living_room_configuration)
            .expect("unable to deserialize scene");
        assert_that(&room.name).is_equal_to(String::from("Living Room"));
        assert_that(&room.scenes).has_length(1);
        assert_that(&room.scenes[0].name).is_equal_to(String::from("white_warmth"));
        assert_that(&room.scenes[0].devices).has_length(3);

        assert!(matches!(room.scenes[0].devices[0], Device::HueWhiteAndColorAmbiance{..}));
        assert!(matches!(room.scenes[0].devices[1], Device::WemoOutlet{..}));
        assert!(matches!(room.scenes[0].devices[2], Device::NanoleafShapes{..}));
    }
}