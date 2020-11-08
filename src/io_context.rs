use std::collections::HashMap;

use crate::io_config;
use crate::rules_config;

use tracing::warn;

pub struct InputContext<'a> {
    pub input: &'a io_config::Input,
    pub rules: Vec<&'a rules_config::Rule>,
}

pub fn make_input_context_map<'a>(
    io: &'a io_config::IoConfig,
    rules_config: &'a rules_config::RulesConfig,
) -> HashMap<&'a str, InputContext<'a>> {
    let mut map: HashMap<&str, InputContext> = HashMap::new();

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
