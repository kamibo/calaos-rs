extern crate nom;
use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::character::complete;
use nom::combinator::map;
use nom::IResult;

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

fn get_wago_int(input: &str) -> IResult<&str, WagoData> {
    let (input, _) = tag("WAGO INT ")(input)?;
    let (input, var_str) = complete::digit0(input)?;
    let (input, _) = tag(" ")(input)?;
    let (input, value_str) = complete::digit0(input)?;

    // TODO handle errors
    let var: u32 = var_str.parse().unwrap();
    let value: u32 = value_str.parse().unwrap();

    Ok((input, WagoData { var, value }))
}

fn get_discover(input: &str) -> IResult<&str, ()> {
    let (input, _) = tag("CALAOS_DISCOVER")(input)?;

    Ok((input, ()))
}

fn get_any(input: &str) -> IResult<&str, Request> {
    alt((
        map(get_wago_int, Request::WagoInt),
        map(get_discover, |_| Request::Discover),
    ))(input)
}

pub fn parse_request(req: &str) -> Result<Request, Box<dyn Error>> {
    match get_any(req) {
        Ok((_, value)) => Ok(value),
        Err(e) => Err(Box::new(e.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unserialize_calaos_wago_int() {
        let msg = "WAGO INT 10 1";
        assert_eq!(get_wago_int(msg), Ok(("", WagoData { var: 10, value: 1 })));
    }

    #[test]
    fn unserialize_calaos_discover() {
        let msg = "CALAOS_DISCOVER";
        assert_eq!(get_discover(msg), Ok(("", ())));
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
        assert!(!parse_request("ERROR").err().unwrap().to_string().is_empty());
    }
}
