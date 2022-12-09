use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Debug, Clone)]
pub struct TurnLightGroupOnOrOff {
    pub on: LightGroupOn,
}

impl TurnLightGroupOnOrOff {
    pub const ON: TurnLightGroupOnOrOff = TurnLightGroupOnOrOff {
        on: LightGroupOn { on: true },
    };
    pub const OFF: TurnLightGroupOnOrOff = TurnLightGroupOnOrOff {
        on: LightGroupOn { on: false },
    };
}

#[derive(Deserialize, Debug, Clone)]
pub struct LightGroupDimming {
    pub brightness: f32,
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

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "rtype", content = "rid")]
#[serde(rename_all = "snake_case")]
pub enum HueReference {
    Device(Uuid),
    GroupedLight(Uuid),
    Room(Uuid),
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
