extern crate serde;

use std::convert::From;
use std::convert::TryFrom;

use crate::event;
use crate::io_config;
use crate::io_context;
use crate::io_value;

use io_config::Input;
use io_config::IoConfig;
use io_config::Output;
use io_config::Room;

use io_context::InputContextMap;
use io_context::OutputContextMap;

use io_value::IOValue;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "msg")]
pub enum Request {
    Pong {
        data: Vec<u8>,
    },
    #[serde(rename = "login")]
    Login {
        data: LoginData,
    },
    #[serde(rename = "get_home")]
    GetHome,
    #[serde(rename = "set_state")]
    SetState {
        data: SetStateData,
    },
    // Internal request
    #[serde(skip)]
    Event {
        kind: event::Event,
        data: io_context::IOData,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "msg")]
pub enum Response {
    #[serde(rename = "login")]
    Login { data: Success },
    #[serde(rename = "get_home")]
    GetHome { data: HomeData },
    #[serde(rename = "set_state")]
    SetState { data: Success },
    #[serde(rename = "event")]
    Event { data: EventData },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct LoginData {
    cn_user: String,
    cn_pass: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SetStateData {
    id: String,
    value: IOValue,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Success {
    #[serde(serialize_with = "serialize_bool_to_string")]
    success: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct HomeData {
    home: Vec<RoomData>,
    cameras: CameraData,
    audio: AudioData,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct RoomData {
    name: String,
    #[serde(rename = "type", default)]
    typer: String,
    hits: String,
    items: Vec<RoomIOData>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct EventData {
    event_raw: String,
    #[serde(rename = "type", default)]
    typer: i32,
    type_str: String,
    data: io_context::IOData,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum IOData {
    Input(Input),
    Output(Output),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct RoomIOData {
    #[serde(flatten)]
    io_data: IOData,
    var_type: String,
    state: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CameraData {}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioData {}

fn make_rooms<'a>(
    io_config: &IoConfig,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
) -> Vec<RoomData> {
    io_config
        .home
        .rooms
        .iter()
        .map(|room| RoomData {
            name: room.name.clone(),
            typer: room.typer.clone(),
            hits: room.hits.to_string(),
            items: make_room_ios(room, input_map, output_map),
        })
        .collect()
}

fn make_room_ios<'a>(
    room: &Room,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
) -> Vec<RoomIOData> {
    let mut ios: Vec<RoomIOData> = Vec::with_capacity(room.inputs.len() + room.outputs.len());

    ios.extend(
        room.inputs
            .iter()
            .filter_map(|input| make_room_input(input, input_map))
            .collect::<Vec<RoomIOData>>(),
    );

    ios.extend(
        room.outputs
            .iter()
            .filter_map(|output| make_room_output(output, output_map))
            .collect::<Vec<RoomIOData>>(),
    );

    ios
}

fn make_room_input<'a>(input: &Input, input_map: &InputContextMap<'a>) -> Option<RoomIOData> {
    if let Some(input_context) = input_map.get(input.id.as_str()) {
        let value_opt = input_context.value.read().unwrap();

        let value = match &*value_opt {
            Some(v) => v,
            None => return None,
        };

        return Some(make_room_io_data(IOData::Input(input.clone()), value));
    }

    None
}

fn make_room_output<'a>(output: &Output, output_map: &OutputContextMap<'a>) -> Option<RoomIOData> {
    if let Some(output_context) = output_map.get(output.id.as_str()) {
        let value_opt = output_context.value.read().unwrap();

        let value = match &*value_opt {
            Some(v) => v,
            None => return None,
        };

        return Some(make_room_io_data(IOData::Output(output.clone()), value));
    }

    None
}

fn make_room_io_data(io_data: IOData, value: &IOValue) -> RoomIOData {
    RoomIOData {
        io_data,
        var_type: make_io_value_type(value),
        state: String::from(value),
    }
}

fn make_io_value_type(value: &IOValue) -> String {
    match value {
        // Note: "int" does not exist in the json Calaos protocl
        IOValue::Int(_) => "float".to_string(),
        IOValue::Bool(_) => "bool".to_string(),
        _ => "string".to_string(),
    }
}

fn serialize_bool_to_string<S>(value: &bool, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::ser::Serializer,
{
    if *value {
        serializer.serialize_str("true")
    } else {
        serializer.serialize_str("false")
    }
}

impl LoginData {
    pub fn new(user: String, pass: String) -> Self {
        Self {
            cn_user: user,
            cn_pass: pass,
        }
    }
}

impl HomeData {
    pub fn new<'a>(
        io_config: &IoConfig,
        input_map: &InputContextMap<'a>,
        output_map: &OutputContextMap<'a>,
    ) -> Self {
        Self {
            home: make_rooms(io_config, input_map, output_map),
            cameras: CameraData {},
            audio: AudioData {},
        }
    }
}

impl Success {
    pub fn new(value: bool) -> Self {
        Self { success: value }
    }
}

impl EventData {
    pub fn new(kind: event::Event, data: io_context::IOData) -> Self {
        let kind_str: &'static str = kind.into();

        Self {
            event_raw: format!(
                "{}, id:{} state:{}",
                kind_str,
                data.id,
                String::from(&data.value)
            ),
            typer: kind as i32,
            type_str: String::from(kind_str),
            data,
        }
    }
}

impl TryFrom<&str> for Request {
    type Error = serde_json::error::Error;

    fn try_from(msg: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(msg)
    }
}

impl From<SetStateData> for io_context::IOData {
    fn from(data: SetStateData) -> Self {
        Self::new(data.id, data.value)
    }
}

pub fn to_json_string<T: serde::Serialize>(result: &T) -> Result<String, serde_json::error::Error> {
    serde_json::to_string(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unserialize_login() {
        let msg = r#"{
            "data": {
                "cn_pass": "any_pass",
                "cn_user": "any_user"
            },
            "msg": "login"
        }"#;

        let request_data: Request = serde_json::from_str(msg).expect("error unserialzation");
        assert_eq!(
            request_data,
            Request::Login {
                data: LoginData {
                    cn_user: "any_user".to_string(),
                    cn_pass: "any_pass".to_string()
                }
            }
        );
    }

    #[test]
    fn unserialize_get_home() {
        let msg = r#"{
            "data": {
            },
            "msg": "get_home"
        }"#;

        let request_data: Request = serde_json::from_str(msg).expect("error unserialzation");
        assert_eq!(request_data, Request::GetHome {});
    }

    #[test]
    fn serialize_login_response() {
        let response = Response::Login {
            data: Success::new(true),
        };
        let json: serde_json::Value = serde_json::to_value(&response).expect("error serialization");
        let expected_json = serde_json::json!({
            "msg": "login",
            "data" : {
                "success": "true"
            }
        }
        );

        assert_eq!(json, expected_json);
    }
}
