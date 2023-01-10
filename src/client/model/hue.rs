use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct HueResponse<T, E = ()> {
    pub data: Vec<T>,
    pub errors: Vec<E>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GroupedLight {
    pub id: Uuid,
    pub on: LightGroupOn,
    pub dimming: LightGroupDimming,
    pub owner: HueReference,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LightGroupOn {
    pub on: bool,
}

impl LightGroupOn {
    pub const ON: LightGroupOn = LightGroupOn { on: true };
    pub const OFF: LightGroupOn = LightGroupOn { on: false };
}

#[derive(TypedBuilder, Serialize, Debug)]
pub struct GroupedLightPutBody {
    on: LightGroupOn,
    #[builder(default=None, setter(strip_option))]
    #[serde(skip_serializing_if = "Option::is_none")]
    dimming: Option<LightGroupDimming>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RecallSceneAction {
    Active,
    Static,
}

#[derive(Serialize, Debug)]
pub struct RecallSceneBody {
    items: ActionPut,
    recall: RecallSceneOptions,
}

impl RecallSceneBody {
    pub fn new(brightness: Option<f32>) -> Self {
        let options = RecallSceneOptions::builder()
            .action(RecallSceneAction::Static)
            .dimming(brightness.map(LightGroupDimming::new))
            .build();
        Self {
            items: ActionPut::TURN_ON,
            recall: options,
        }
    }
}

#[derive(TypedBuilder, Serialize, Debug)]
struct RecallSceneOptions {
    action: RecallSceneAction,

    #[serde(skip_serializing_if = "Option::is_none")]
    dimming: Option<LightGroupDimming>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LightGroupDimming {
    pub brightness: f32,
}

impl LightGroupDimming {
    pub fn new(brightness: f32) -> Self {
        Self { brightness }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct ActionPut {
    target: HueReference,
    action: LightGroupOn,
}

impl ActionPut {
    const TURN_ON: ActionPut = ActionPut {
        target: HueReference::EMPTY,
        action: LightGroupOn::ON,
    };
}

#[derive(Debug, Deserialize, Clone)]
pub struct HueRoom {
    pub id: Uuid,
    pub children: Vec<HueReference>,
    pub services: Vec<HueReference>,
    pub metadata: HueObjectMetadata,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HueObjectMetadata {
    pub name: String,
    pub archtype: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "rtype", content = "rid")]
#[serde(rename_all = "snake_case")]
pub enum HueReference {
    Device(Uuid),
    GroupedLight(Uuid),
    Room(Uuid),

    #[serde(rename = "")]
    Empty(String),
}

impl HueReference {
    pub const EMPTY: HueReference = HueReference::Empty(String::new());
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::client::model::hue::HueReference;

    #[test]
    fn it_deserializes_a_hue_reference() {
        let reference_id = Uuid::new_v4();
        let hue_reference_text = r#"{"rid": "RID", "rtype": "device"}"#;
        let json = hue_reference_text.replace("RID", reference_id.to_string().as_str());

        let deserialized_reference: HueReference =
            serde_json::from_str(&json).expect(format!("unable to deserialize {}", json).as_str());

        match deserialized_reference {
            HueReference::Device(id) => {
                assert_eq!(id, reference_id)
            }
            _ => {
                panic!("unable to deserialize device")
            }
        }
    }
}
