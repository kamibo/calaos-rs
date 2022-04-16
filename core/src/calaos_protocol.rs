extern crate nom;
use nom::character::complete::digit0;

use std::error::Error;

#[derive(Debug, PartialEq)]
pub struct WagoData {
    pub var: u32,
    pub value: u32,
}

#[derive(Debug, PartialEq)]
pub enum Request {
    WagoInt(WagoData),
    Discover,
}

named!(
    get_wago_int<(u32, u32)>,
    tuple!(
        preceded!(tag!(b"WAGO INT "), flat_map!(digit0, parse_to!(u32))),
        preceded!(tag!(b" "), flat_map!(digit0, parse_to!(u32)))
    )
);

named!(get_discover, tag!(b"CALAOS_DISCOVER"));

named!(
    get_any<Request>,
    alt!(
        get_wago_int => { |(var, value)| Request::WagoInt(WagoData{var, value}) } |
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
        let msg = b"WAGO INT 10 1";
        assert_eq!(get_wago_int(msg), Ok((&msg[msg.len()..], (10, 1))));
    }

    #[test]
    fn unserialize_calaos_discover() {
        let msg = b"CALAOS_DISCOVER";
        assert_eq!(get_discover(msg), Ok((&msg[msg.len()..], &msg[..])));
    }

    #[test]
    fn parse_request_ok() {
        assert_eq!(
            parse_request("WAGO INT 99 1").unwrap(),
            Request::WagoInt(WagoData { var: 99, value: 1 })
        );
        assert_eq!(
            parse_request("WAGO INT 0 0").unwrap(),
            Request::WagoInt(WagoData { var: 0, value: 0 })
        );
        assert_eq!(parse_request("CALAOS_DISCOVER").unwrap(), Request::Discover);
    }

    #[test]
    fn parse_request_err() {
        assert!(parse_request("ERROR").err().unwrap().to_string().len() > 0);
    }
}
