extern crate serde;
extern crate serde_xml_rs;

use crate::io_value::IOAction;
use crate::io_value::IOValue;

use std::error::Error;
use std::fs::File;
use std::io::BufReader;

#[derive(Debug, Deserialize)]
pub struct Output {
    pub id: String,
    pub val: IOAction,
}

#[derive(Debug, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub typer: String,
    pub output: Output,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "oper")]
pub enum Operator {
    #[serde(rename = "==")]
    Equal,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Input {
    pub id: String,
    #[serde(rename = "oper", flatten)]
    pub operator: Operator,
    #[serde(rename = "val")]
    pub value: IOValue,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ConditionKind {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "standard")]
    Standard { input: Input },
}

#[derive(Debug, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(rename = "type")]
    pub typer: String,
    #[serde(rename = "condition")]
    pub conditions: Vec<ConditionKind>,
    #[serde(rename = "action")]
    pub actions: Vec<Action>,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "rules")]
pub struct RulesConfig {
    #[serde(rename = "rule")]
    pub rules: Vec<Rule>,
}

pub fn read_from_file(path: &std::path::Path) -> Result<RulesConfig, Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);

    Ok(serde_xml_rs::from_reader(reader).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_xml_rs::from_reader as xml_from_reader;

    #[test]
    fn unserialize_config() {
        let s = r##"
<calaos:rules xmlns:calaos="http://www.calaos.fr">
    <calaos:rule name="night corridor" type="corridor">
        <calaos:condition type="standard" trigger="true">
            <calaos:input id="input_0" oper="==" val="1" />
        </calaos:condition>
        <calaos:action type="standard">
            <calaos:output id="output_0" val="toggle" />
        </calaos:action>
    </calaos:rule>
    <calaos:rule name="bedroom 1 light" type="bedroom">
        <calaos:condition type="standard" trigger="true">
            <calaos:input id="input_1" oper="==" val="true" />
        </calaos:condition>
        <calaos:condition type="standard" trigger="true">
            <calaos:input id="input_2" oper="==" val="true" />
        </calaos:condition>
        <calaos:action type="standard">
            <calaos:output id="output_1" val="toggle" />
        </calaos:action>
    </calaos:rule>
</calaos:rules>
"##;

        let config: RulesConfig = xml_from_reader(s.as_bytes()).unwrap();

        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].name, "night corridor");
        let input = Input {
            id: String::from("input_0"),
            operator: Operator::Equal,
            value: IOValue::Int(1),
        };
        assert_eq!(
            config.rules[0].conditions[0],
            ConditionKind::Standard { input }
        );
        assert_eq!(config.rules[0].actions.len(), 1);
        assert_eq!(config.rules[0].actions[0].output.id, "output_0");
        assert_eq!(config.rules[0].actions[0].output.val, IOAction::Toggle);
    }
}
