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
    use std::fs::File;
    use std::io::BufReader;
    use crate::caseta::scene::*;

    #[test]
    fn it_deserializes() {
        let living_room = File::open("src/living_room.json").expect("this file should always be here");
        let room : Room = serde_json::from_reader(BufReader::new(living_room)).expect("unable to deserialize scene");
    }
}