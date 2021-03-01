extern crate serde;

use std::convert::TryFrom;

use crate::io_config;

use io_config::Input;
use io_config::IoConfig;
use io_config::Output;
use io_config::Room;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "msg")]
pub enum Request {
    #[serde(rename = "login")]
    Login { data: LoginData },
    #[serde(rename = "get_home")]
    GetHome,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "msg")]
pub enum Response {
    #[serde(rename = "login")]
    Login { data: Success },
    #[serde(rename = "get_home")]
    GetHome { data: HomeData },
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct LoginData {
    cn_user: String,
    cn_pass: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Success {
    #[serde(serialize_with = "serialize_bool_to_string")]
    success: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct HomeData {
    home: Vec<RoomData>,
    cameras: CameraData,
    audio: AudioData,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RoomData {
    name: String,
    #[serde(rename = "type", default)]
    typer: String,
    hits: String,
    items: Vec<RoomIOData>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum IOData {
    Input(Input),
    Output(Output),
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RoomIOData {
    #[serde(flatten)]
    io_data: IOData,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CameraData {}

#[derive(Debug, Serialize, PartialEq)]
pub struct AudioData {}

fn make_rooms(io_config: &IoConfig) -> Vec<RoomData> {
    io_config
        .home
        .rooms
        .iter()
        .map(|room| RoomData {
            name: room.name.clone(),
            typer: room.typer.clone(),
            hits: room.hits.to_string(),
            items: make_room_ios(room),
        })
        .collect()
}

fn make_room_ios(room: &Room) -> Vec<RoomIOData> {
    let mut ios : Vec<RoomIOData> = Vec::with_capacity(room.inputs.len() + room.outputs.len());

    ios.extend(room
        .inputs
        .iter()
        .map(|input| make_room_input(input))
        .collect::<Vec<RoomIOData>>());

    ios.extend(room
        .outputs
        .iter()
        .map(|output| make_room_output(output))
        .collect::<Vec<RoomIOData>>());

    ios
}

fn make_room_input(input: &Input) -> RoomIOData {
    RoomIOData {
        io_data: IOData::Input(input.clone()),
    }
}

fn make_room_output(output: &Output) -> RoomIOData {
    RoomIOData {
        io_data: IOData::Output(output.clone()),
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

impl HomeData {
    pub fn new(io_config: &IoConfig) -> Self {
        Self {
            home: make_rooms(io_config),
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

impl TryFrom<&str> for Request {
    type Error = serde_json::error::Error;

    fn try_from(msg: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(msg)
    }
}

pub fn to_json_string(result: &Response) -> Result<String, serde_json::error::Error> {
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
