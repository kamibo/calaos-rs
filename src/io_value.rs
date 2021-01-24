extern crate serde;

use std::convert::From;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(from = "String")]
pub enum IOValue {
    Bool(bool),
    Int(i32),
    String(String),
}

impl From<String> for IOValue {
    fn from(value: String) -> Self {
        // ... manually parsing string
        let value_str = value.as_str();

        if let Ok(int_value) = value_str.parse::<i32>() {
            return Self::Int(int_value);
        }

        match value_str {
            "false" => return Self::Bool(false),
            "true" => return Self::Bool(true),
            _ => return Self::String(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_string_iovalue() {
        assert_eq!(IOValue::from("true".to_string()), IOValue::Bool(true));
        assert_eq!(IOValue::from("false".to_string()), IOValue::Bool(false));
        assert_eq!(
            IOValue::from("TRUE".to_string()),
            IOValue::String("TRUE".to_string())
        );
        assert_eq!(IOValue::from("0".to_string()), IOValue::Int(0));
        assert_eq!(IOValue::from("1".to_string()), IOValue::Int(1));
        assert_eq!(IOValue::from("-10".to_string()), IOValue::Int(-10));
    }
}
