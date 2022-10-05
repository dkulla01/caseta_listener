use ordered_float::OrderedFloat;
use uuid::Uuid;

use serde_derive::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct LightGroup {
    pub data: Vec<LightGroupData>
}

#[derive(Deserialize, Debug, Clone)]
pub struct LightGroupData {
    pub id: Uuid,
    pub on: LightGroupOn,
    pub dimming: LightGroupDimming

}

#[derive(Deserialize, Debug, Clone)]
pub struct LightGroupOn {
    pub on: bool
}

#[derive(Deserialize, Debug, Clone)]
pub struct LightGroupDimming {
    pub  brightness: f32
}

#[derive(Deserialize, Debug, Clone)]
pub struct HueLightResponse {
    pub data: Vec<HueLight>
}

#[derive(Deserialize, Debug, Clone)]
pub struct HueLight {
    pub id: Uuid,
    pub owner: HueReference,
    pub on: LightGroupOn,
    pub color: Option<Color>,
}

#[derive(Debug, Deserialize, Clone, Hash, PartialEq, Eq)]
pub struct Color {
    xy: ColorCoordinates
}

#[derive(Debug, Deserialize, Clone, Hash, PartialEq, Eq)]
pub struct ColorCoordinates {
    x: OrderedFloat<f32>,
    y: OrderedFloat<f32>
}

#[derive(Debug, Deserialize, Clone)]
pub struct HueRoomResponse {
    pub data: Vec<HueRoom>
}

#[derive(Debug, Deserialize, Clone)]
pub struct HueRoom {
    pub id: Uuid,
    pub children: Vec<HueReference>,
    pub services: Vec<HueReference>,
    pub metadata: HueRoomMetadata

}

#[derive(Debug, Deserialize, Clone)]
pub struct HueRoomMetadata {
    pub name: String
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "rtype", content = "rid")]
#[serde(rename_all = "snake_case")]
pub enum HueReference {
    Device(Uuid),
    GroupedLight(Uuid)
}

#[cfg(test)]
mod tests {
    use ordered_float::OrderedFloat;
    use uuid::Uuid;

    use crate::client::model::hue::HueLightResponse;

    use crate::client::model::hue::HueReference;

    #[test]
    fn it_deserializes_a_hue_reference() {
        let reference_id = Uuid::new_v4();
        let hue_reference_text = r#"{"rid": "RID", "rtype": "device"}"#;
        let json = hue_reference_text.replace("RID", reference_id.to_string().as_str());

        let deserialized_reference: HueReference = serde_json::from_str(&json)
        .expect(format!("unable to deserialize {}", json).as_str());

        match deserialized_reference {
            HueReference::Device(id) => {
                assert_eq!(id, reference_id)
            },
            _ => {panic!("unable to deserialize device")}
        }
    }

    #[test]
    fn it_deserializes_an_ordered_float_into_light_response_body() {
        let light_color_x_value = OrderedFloat(2.34);
        let light_color_y_value = OrderedFloat(1.2);
        
        let response_text = r#"
        {
            "errors": [],
            "data": [
                {
                    "id": "3901798f-ee1e-4538-8e92-14920c06068c",
                    "id_v1": "/lights/27",
                    "owner": {
                        "rid": "222c7065-57a8-4b80-8d32-8e3d45b5ab79",
                        "rtype": "device"
                        },
                    "metadata": {
                        "name": "Nightstand color",
                        "archetype": "table_shade"
                    },
                    "on": {
                        "on": false
                    },
                    "dimming": {
                        "brightness": 62.99,
                        "min_dim_level": 0.20000000298023225
                    },
                    "dimming_delta": {},
                    "color_temperature": {
                        "mirek": null,
                        "mirek_valid": false,
                        "mirek_schema": {
                            "mirek_minimum": 153,
                            "mirek_maximum": 500
                        }
                    },
                    "color_temperature_delta": {},
                    "color": {
                        "xy": {
                            "x": LIGHT_COLOR_X_VALUE,
                            "y": LIGHT_COLOR_Y_VALUE
                        },
                        "gamut": {
                            "red": {
                                "x": 0.6915,
                                "y": 0.3083
                            },
                            "green": {
                                "x": 0.17,
                                "y": 0.7
                            },
                            "blue": {
                                "x": 0.1532,
                                "y": 0.0475
                            }
                        },
                        "gamut_type": "C"
                    },
                    "dynamics": {
                    "status": "none",
                    "status_values": [
                        "none",
                        "dynamic_palette"
                    ],
                    "speed": 0.0,
                    "speed_valid": false
                    },
                    "alert": {
                        "action_values": [
                            "breathe"
                        ]
                    },
                    "signaling": {},
                    "mode": "normal",
                    "effects": {
                        "status_values": [
                            "no_effect",
                            "candle",
                            "fire"
                        ],
                        "status": "no_effect",
                        "effect_values": [
                            "no_effect",
                            "candle",
                            "fire"
                        ]
                    },
                    "type": "light"
                } 
            ]
          }   
        "#;
        let response_text = response_text.replace("LIGHT_COLOR_X_VALUE", light_color_x_value.to_string().as_str());
        let response_text = response_text.replace("LIGHT_COLOR_Y_VALUE", light_color_y_value.to_string().as_str());
        let deserialized : HueLightResponse = serde_json::from_str(response_text.as_str())
        .expect("unable to deserialize");

        let hue_light = &deserialized.data[0];
        let color = hue_light.color.as_ref().unwrap();

        assert_eq!(color.xy.x, light_color_x_value);
        assert_eq!(color.xy.y, light_color_y_value);
        assert!(matches!(deserialized.data.len(), 1))
    }
}
