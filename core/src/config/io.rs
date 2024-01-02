extern crate serde;
extern crate serde_aux;
extern crate serde_xml_rs;

use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;
use std::time::Duration;

use serde::Deserializer;
use serde_aux::prelude::*;

pub fn deserialize_seconds_from_string<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let number = deserialize_number_from_string::<u64, D>(deserializer)?;
    Ok(Duration::from_secs(number))
}

pub fn deserialize_opt_number_from_string<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + serde::Deserialize<'de>,
    <T as FromStr>::Err: Display,
{
    let number = deserialize_number_from_string::<T, D>(deserializer)?;
    Ok(Some(number))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WagoIO {
    pub host: String,
    pub port: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub var: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WagoIOUpDown {
    pub host: String,
    pub port: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub var_up: u32,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub var_down: u32,
    #[serde(deserialize_with = "deserialize_seconds_from_string")]
    pub time: Duration,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Date {
    #[serde(default, deserialize_with = "deserialize_opt_number_from_string")]
    pub year: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_opt_number_from_string")]
    pub month: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_opt_number_from_string")]
    pub day: Option<u32>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub hour: u32,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub min: u32,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub sec: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum InputKind {
    InputTime(Date),
    WIDigitalBP(WagoIO),
    WIDigitalLong(WagoIO),
    WIDigitalTriple(WagoIO),
    MySensorsInputAnalog, // TODO
    MySensorsInputTemp,   // TODO
    #[serde(rename = "scenario")]
    Scenario, // TODO
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Input {
    pub name: String,
    pub id: String,
    pub io_type: String,
    #[serde(flatten)]
    pub kind: InputKind,
    pub gui_type: String,
    pub visible: bool,
    #[serde(default)]
    pub rw: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum OutputKind {
    HueOutputLightRGB, // TODO
    WODigital(WagoIO),
    #[serde(alias = "WOVolet")]
    WOShutter(WagoIOUpDown),
    MySensorsOutputShutterSmart, // TODO
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Output {
    pub name: String,
    pub id: String,
    pub io_type: String,
    #[serde(flatten)]
    pub kind: OutputKind,
    pub gui_type: String,
    pub visible: bool,
    #[serde(default)]
    pub rw: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Internal {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Room {
    pub name: String,
    #[serde(rename = "type", default)]
    pub typer: String,
    #[serde(rename = "input", default)]
    pub inputs: Vec<Input>,
    #[serde(rename = "output", default)]
    pub outputs: Vec<Output>,
    #[serde(rename = "internal", default)]
    pub internals: Vec<Internal>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub hits: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename = "home")]
pub struct Home {
    #[serde(rename = "room", default)]
    pub rooms: Vec<Room>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename = "ioconfig")]
pub struct IoConfig {
    pub home: Home,
}

pub fn read_from_file(path: &std::path::Path) -> Result<IoConfig, Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);

    Ok(serde_xml_rs::from_reader(reader).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_xml_rs::from_reader as xml_from_reader;

    #[test]
    fn unserialize_config() {
        let s = r#"
            <calaos:ioconfig xmlns:calaos="http://www.calaos.fr">
                <calaos:home>
                    <calaos:room name="kit" type="kitchen" hits="0">
                        <calaos:input enabled="true" gui_type="switch_long" host="192.168.1.1" id="input_0" io_type="input" name="switch" port="502" type="WIDigitalLong" var="3" visible="false" wago_841="true" />
                        <calaos:output enabled="true" gtype="light" gui_type="light" host="192.168.1.1" id="output_0" io_type="output" log_history="true" name="shutter 1" port="502" type="WODigital" var="36" visible="true" wago_841="true" />
                        <calaos:output enabled="true" gtype="light" gui_type="light" host="192.168.1.1" id="output_37" io_type="output" log_history="true" name="shutter 2" port="502" type="WODigital" var="10" visible="true" wago_841="true" />
                        <calaos:output enabled="true" gui_type="shutter" host="192.168.1.1" id="output_14" io_type="output" log_history="true" name="shutter 3" port="502" time="25" type="WOVolet" var_down="41" var_up="40" visible="true" wago_841="true" />
                    </calaos:room>

                </calaos:home>
            </calaos:ioconfig>
        "#;

        let config: IoConfig = xml_from_reader(s.as_bytes()).unwrap();
        let host = "192.168.1.1".to_string();
        let port = "502".to_string();

        assert_eq!(config.home.rooms.len(), 1);
        assert_eq!(config.home.rooms[0].name, "kit");
        assert_eq!(config.home.rooms[0].typer, "kitchen");
        assert_eq!(
            config.home.rooms[0].inputs[0],
            Input {
                name: "switch".to_string(),
                id: "input_0".to_string(),
                io_type: "input".to_string(),
                kind: InputKind::WIDigitalLong(WagoIO {
                    host: host.clone(),
                    port: port.clone(),
                    var: 3
                }),
                gui_type: "switch_long".to_string(),
                visible: false,
                rw: false,
            }
        );
        assert_eq!(
            config.home.rooms[0].outputs[0],
            Output {
                name: "shutter 1".to_string(),
                id: "output_0".to_string(),
                io_type: "output".to_string(),
                kind: OutputKind::WODigital(WagoIO {
                    host: host.clone(),
                    port: port.clone(),
                    var: 36
                }),
                gui_type: "light".to_string(),
                visible: true,
                rw: false,
            }
        );
        assert_eq!(
            config.home.rooms[0].outputs[1],
            Output {
                name: "shutter 2".to_string(),
                id: "output_37".to_string(),
                io_type: "output".to_string(),
                kind: OutputKind::WODigital(WagoIO {
                    host: host.clone(),
                    port: port.clone(),
                    var: 10
                }),
                gui_type: "light".to_string(),
                visible: true,
                rw: false,
            }
        );
        assert_eq!(
            config.home.rooms[0].outputs[2],
            Output {
                name: "shutter 3".to_string(),
                id: "output_14".to_string(),
                io_type: "output".to_string(),
                kind: OutputKind::WOShutter(WagoIOUpDown {
                    host,
                    port,
                    var_down: 41,
                    var_up: 40,
                    time: Duration::from_secs(25),
                }),
                gui_type: "shutter".to_string(),
                visible: true,
                rw: false,
            }
        );
    }
}
