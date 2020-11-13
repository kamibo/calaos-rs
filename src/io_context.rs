use std::collections::HashMap;

use crate::io_config;
use crate::rules_config;

use tracing::warn;

use io_config::IoConfig;
use rules_config::RulesConfig;

pub struct InputContext<'a> {
    pub input: &'a io_config::Input,
    pub rules: Vec<&'a rules_config::Rule>,
}

pub type InputContextMap<'a> = HashMap<&'a str, InputContext<'a>>;

pub fn make_input_context_map<'a>(
    io: &'a IoConfig,
    rules_config: &'a RulesConfig,
) -> InputContextMap<'a> {
    let mut map = HashMap::new();

    for room in io.home.rooms.iter() {
        for input in room.inputs.iter() {
            if let Some(_) = map.insert(
                input.id.as_str(),
                InputContext {
                    input,
                    rules: Vec::new(),
                },
            ) {
                warn!("IO input ID {:?} is not unique", input.id);
            }
        }
    }

    for rule in rules_config.rules.iter() {
        for condition in rule.conditions.iter() {
            let id = &condition.input.id;

            if id.is_empty() {
                continue;
            }

            if let Some(context) = map.get_mut(id.as_str()) {
                context.rules.push(rule);
            } else {
                warn!(
                    "Rule {:?} condition refers to unknown ID {:?}",
                    rule.name, id
                );
            }
        }
    }

    map
}
