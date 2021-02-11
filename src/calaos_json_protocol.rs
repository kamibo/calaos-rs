extern crate serde;

use std::convert::TryFrom;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "msg")]
pub enum Request {
    #[serde(rename = "login")]
    Login { data: LoginData },
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "msg")]
pub enum Response {
    #[serde(rename = "login")]
    Login { data: Success },
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
