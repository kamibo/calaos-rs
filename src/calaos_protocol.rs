extern crate nom;
use nom::character::complete::digit0;

use std::error::Error;

#[derive(Debug, PartialEq)]
pub enum Request {
    WagoInt(u32),
    Discover,
}

named!(
    get_wago_int<u32>,
    preceded!(tag!(b"WAGO INT "), flat_map!(digit0, parse_to!(u32)))
);

named!(get_discover, tag!(b"CALAOS_DISCOVER"));

named!(
    get_any<Request>,
    alt!(
        get_wago_int => { |value: u32| Request::WagoInt(value) } |
        get_discover => { |_|  Request::Discover }
    )
);

pub fn parse_request(req: &str) -> Result<Request, Box<dyn Error>> {
    match get_any(req.as_bytes()) {
        Ok((_, value)) => Ok(value),
        Err(e) => Err(e.to_owned().into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unserialize_calaos_wago_int() {
        let msg = b"WAGO INT 10";
        assert_eq!(get_wago_int(msg), Ok((&msg[msg.len()..], 10)));
    }

    #[test]
    fn unserialize_calaos_discover() {
        let msg = b"CALAOS_DISCOVER";
        assert_eq!(get_discover(msg), Ok((&msg[msg.len()..], &msg[..])));
    }

    #[test]
    fn parse_request_ok() {
        assert_eq!(parse_request("WAGO INT 99 ").unwrap(), Request::WagoInt(99));
        assert_eq!(parse_request("WAGO INT 0").unwrap(), Request::WagoInt(0));
        assert_eq!(parse_request("CALAOS_DISCOVER").unwrap(), Request::Discover);
    }

    #[test]
    fn parse_request_err() {
        assert!(parse_request("ERROR").err().unwrap().to_string().len() > 0);
    }
}
