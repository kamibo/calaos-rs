extern crate serde;

use std::convert::AsRef;
use std::convert::From;

#[derive(Debug, PartialEq, Clone)]
pub enum ShutterState {
    Up,
    Down,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(from = "String", into = "String")]
pub enum IOValue {
    Bool(bool),
    Int(i32),
    String(String),
    Shutter(ShutterState),
}

const UP_STR: &str = "up";
const DOWN_STR: &str = "down";

impl std::fmt::Display for ShutterState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match *self {
            ShutterState::Up => UP_STR,
            ShutterState::Down => DOWN_STR,
        };

        write!(f, "{}", s)
    }
}

impl<T: AsRef<str>> From<T> for IOValue {
    fn from(value: T) -> Self {
        // ... manually parsing string
        let value_str = value.as_ref();

        if let Ok(int_value) = value_str.parse::<i32>() {
            return Self::Int(int_value);
        }

        match value_str {
            "false" => Self::Bool(false),
            "true" => Self::Bool(true),
            UP_STR => Self::Shutter(ShutterState::Up),
            DOWN_STR => Self::Shutter(ShutterState::Down),
            _ => Self::String(String::from(value_str)),
        }
    }
}

impl From<IOValue> for String {
    fn from(value: IOValue) -> Self {
        String::from(&value)
    }
}

impl From<&IOValue> for String {
    fn from(value: &IOValue) -> Self {
        match value {
            IOValue::Bool(b) => b.to_string(),
            IOValue::Int(i) => i.to_string(),
            IOValue::String(s) => s.to_string(),
            IOValue::Shutter(s) => s.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_string_iovalue() {
        assert_eq!(IOValue::from("true"), IOValue::Bool(true));
        assert_eq!(IOValue::from("false"), IOValue::Bool(false));
        assert_eq!(IOValue::from("TRUE"), IOValue::String("TRUE".to_string()));
        assert_eq!(IOValue::from("0"), IOValue::Int(0));
        assert_eq!(IOValue::from("1"), IOValue::Int(1));
        assert_eq!(IOValue::from("-10"), IOValue::Int(-10));
        assert_eq!(IOValue::from("up"), IOValue::Shutter(ShutterState::Up));
        assert_eq!(IOValue::from("down"), IOValue::Shutter(ShutterState::Down));
    }

    #[test]
    fn parse_iovalue_string() {
        assert_eq!(String::from(IOValue::Bool(true)), "true");
        assert_eq!(String::from(IOValue::Bool(false)), "false");
        assert_eq!(String::from(IOValue::String("str".to_string())), "str");
        assert_eq!(String::from(IOValue::Int(0)), "0");
        assert_eq!(String::from(IOValue::Int(1)), "1");
        assert_eq!(String::from(IOValue::Int(-10)), "-10");
        assert_eq!(String::from(IOValue::Shutter(ShutterState::Up)), "up");
        assert_eq!(String::from(IOValue::Shutter(ShutterState::Down)), "down");
    }
}
