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
    HueColorBulb {id: String, color: String},
    NanoleafShapes{id: String, color: String}
}

#[cfg(test)]
mod tests {
    use crate::caseta::scene::*;
    use spectral::prelude::*;

    #[test]
    fn it_deserializes() {
        let living_room_configuration = r#"
           {
              "name": "living_room",
              "remotes": [1, 2],
              "scenes": [
                {
                  "name": "miami vice flamingo",
                  "devices": [
                    {
                      "id": "...",
                      "type": "hue_color_bulb",
                      "color": "this is where we'd set the color I suppose"
                    },
                    {
                      "id": "...",
                      "type": "nanoleaf_shapes",
                      "color": "this is where we'd specify the scene name"
                    },
                    {
                      "id": "...",
                      "type": "hue_color_bulb",
                      "color": "this is where we'd set the color."
                    }
                  ]
                }
              ]
            }
        "#;
        let room : Room = serde_json::from_str(living_room_configuration)
            .expect("unable to deserialize scene");
        assert_that(&room.name).is_equal_to(String::from("living_room"));
        assert_that(&room.scenes).has_length(1);
        assert_that(&room.scenes[0].devices).has_length(3);
    }
}