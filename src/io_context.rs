use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use crate::io_config;
use crate::rules_config;
use crate::wago_controller;

use futures::future::select_all;

use tracing::warn;

use io_config::InputKind;
use io_config::IoConfig;
use rules_config::RulesConfig;

pub struct InputContext<'a> {
    pub input: &'a io_config::Input,
    pub rules: Vec<&'a rules_config::Rule>,
}

pub struct OutputContext<'a> {
    pub output: &'a io_config::Output,
}

pub type InputContextMap<'a> = HashMap<&'a str, InputContext<'a>>;
pub type OutputContextMap<'a> = HashMap<&'a str, OutputContext<'a>>;

pub fn make_input_context_map<'a>(
    io: &'a IoConfig,
    rules_config: &'a RulesConfig,
) -> InputContextMap<'a> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
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

    for rule in &rules_config.rules {
        for condition in &rule.conditions {
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

pub fn make_output_context_map<'a>(io: &'a IoConfig) -> OutputContextMap<'a> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for output in &room.outputs {
            if let Some(_) = map.insert(output.id.as_str(), OutputContext { output }) {
                warn!("IO output ID {:?} is not unique", output.id);
            }
        }
    }

    map
}

//pub async fn run_input_controllers<'a>(io_config : &IoConfig, is_running: &'a bool) -> Result<(), Box<dyn Error + 'a>>
pub async fn run_input_controllers(
    io_config: &IoConfig,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>>>>> = vec![];

    for input_config in make_input_controller_set(io_config) {
        futures.push(make_input_instance(input_config, is_running));
    }

    let (res, _idx, _remaining_futures) = select_all(futures).await;

    res
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum InputControllerConfig {
    Wago(wago_controller::Config),
}

fn make_input_instance<'a>(
    config: InputControllerConfig,
    is_running: &'a bool,
) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>> {
    match config {
        InputControllerConfig::Wago(config) => Box::pin(wago_controller::run(config, &is_running)),
    }
}

fn make_input_controller_set(io: &IoConfig) -> HashSet<InputControllerConfig> {
    let mut set = HashSet::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
            match &input.kind {
                InputKind::WIDigitalBP(io)
                | InputKind::WIDigitalLong(io)
                | InputKind::WIDigitalTriple(io) => {
                    // TODO remove unwrap and handle error
                    let config = wago_controller::Config {
                        remote_addr: SocketAddr::new(
                            io.host.parse().unwrap(),
                            io.port.parse().unwrap(),
                        ),
                        heartbeat_period: Duration::from_secs(2),
                    };
                    set.insert(InputControllerConfig::Wago(config));
                }
                // TODO
                _ => {}
            }
        }
    }

    set
}
