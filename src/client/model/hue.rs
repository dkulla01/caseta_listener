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

#[derive(Deserialize, Debug)]
pub struct HueLightResponse {
    data: Vec<HueLight>
}

#[derive(Deserialize, Debug)]
pub struct HueLight {
    pub id: Uuid,
    pub owner: Owner,
    pub on: LightGroupOn,
    pub color: Option<Color>,
}

#[derive(Deserialize, Debug)]
pub struct Owner {
    #[serde(rename = "rid")]
    pub id: Uuid,

    #[serde(rename = "rtype")]
    pub owner_type: String
}

#[derive(Debug, Deserialize, Clone)]
pub struct Color {
    xy: ColorCoordinates
}

#[derive(Debug, Deserialize, Clone)]
pub struct ColorCoordinates {
    x: f32,
    y: f32
}

#[cfg(test)]
mod tests {
    use crate::client::model::hue::HueLightResponse;

    #[test]
    fn it_deserializes_a_hue_light_response_body() {
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
                            "x": 0.5529,
                            "y": 0.2549
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

        let deserialized : HueLightResponse = serde_json::from_str(response_text)
        .expect("unable to deserialize");

        assert!(matches!(deserialized.data.len(), 1))
    }
}
