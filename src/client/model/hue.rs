use uuid::Uuid;

use serde_derive::Deserialize;

#[derive(Deserialize, Debug)]
pub struct LightGroup {
    pub data: Vec<LightGroupData>
}

#[derive(Deserialize, Debug)]
pub struct LightGroupData {
    pub id: Uuid,
    pub on: LightGroupOn,
    pub dimming: LightGroupDimming

}

#[derive(Deserialize, Debug)]
pub struct LightGroupOn {
    pub on: bool
}

#[derive(Deserialize, Debug)]
pub struct LightGroupDimming {
    pub  brightness: f32
}
