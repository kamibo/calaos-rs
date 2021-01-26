extern crate serde;

use std::convert::AsRef;
use std::convert::From;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(from = "String")]
pub enum IOValue {
    Bool(bool),
    Int(i32),
    String(String),
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
            _ => Self::String(String::from(value_str)),
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
    }
}
