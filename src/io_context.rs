use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use crate::io::wago_controller;
use crate::io::wago_modbus_controller;
use crate::io_config;
use crate::rules_config;

use futures::future::select_all;

use tokio::sync::broadcast;
use tokio::sync::mpsc;

use tracing::*;

use io_config::InputKind;
use io_config::IoConfig;
use io_config::OutputKind;
use rules_config::RulesConfig;

pub struct InputContext<'a> {
    pub input: &'a io_config::Input,
    pub rules: Vec<&'a rules_config::Rule>,
}

#[derive(Debug, Clone)]
pub struct OutputContext<'a> {
    pub output: &'a io_config::Output,
    pub value: Option<IOValue>,
}

pub type InputContextMap<'a> = HashMap<&'a str, InputContext<'a>>;
pub type OutputContextMap<'a> = HashMap<&'a str, OutputContext<'a>>;

#[derive(Debug, Clone)]
pub enum IOValue {
    Bool(bool),
    String(String),
}

#[derive(Debug, Clone)]
pub struct IOData {
    pub id: String,
    pub value: IOValue,
}

const CHANNEL_CAPACITY: usize = 1000;

pub type BroadcastIODataRx = broadcast::Receiver<IOData>;
pub type BroadcastIODataTx = broadcast::Sender<IOData>;

pub fn make_iodata_broadcast_channel() -> (BroadcastIODataTx, BroadcastIODataRx) {
    broadcast::channel::<IOData>(CHANNEL_CAPACITY)
}

pub type OutputIODataRx = mpsc::Receiver<IOData>;
pub type OutputIODataTx = mpsc::Sender<IOData>;

pub fn make_iodata_output_channel() -> (OutputIODataTx, OutputIODataRx) {
    mpsc::channel::<IOData>(CHANNEL_CAPACITY)
}

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
            if let Some(_) = map.insert(
                output.id.as_str(),
                OutputContext {
                    output,
                    value: None,
                },
            ) {
                warn!("IO output ID {:?} is not unique", output.id);
            }
        }
    }

    map
}

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
                            // Use calaos special port
                            4646,
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

pub async fn run_output_controllers<'a>(
    io_config: &IoConfig,
    output_map: &OutputContextMap<'a>, // TODO FIX
    rx: BroadcastIODataRx,
    tx_feedback: BroadcastIODataTx,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>>>>> = vec![];

    type ReverseMap = HashMap<String, OutputIODataTx>;

    let mut reverse_map: ReverseMap = HashMap::new();

    for (output_config, ids) in make_output_controller_map(io_config) {
        let (tx, rx) = make_iodata_output_channel();

        fn filter_output<'a>(
            output_map: &OutputContextMap<'a>,
            ids: &Vec<String>,
        ) -> OutputContextMap<'a> {
            let mut res = output_map.clone();
            res.retain(|k, _| ids.iter().any(|x| k == x));
            res
        }

        let instance_output_map = filter_output(&output_map, &ids);

        futures.push(make_output_instance(
            output_config,
            rx,
            tx_feedback.clone(),
            instance_output_map,
            is_running,
        ));

        for id in ids {
            reverse_map.insert(id, tx.clone());
        }
    }

    async fn handle_rx(
        mut rx: BroadcastIODataRx,
        reverse_map: ReverseMap,
        is_running: &bool,
    ) -> Result<(), Box<dyn Error>> {
        while *is_running {
            let io_data = rx.recv().await?;

            trace!("Received new output to dispatch {:?}", io_data);

            if let &Some(tx) = &reverse_map.get(&io_data.id) {
                tx.send(io_data).await?;
            } else {
                warn!("Cannot dispatch {:?} to output controller", io_data);
            }
        }

        Ok(())
    }

    futures.push(Box::pin(handle_rx(rx, reverse_map, is_running)));

    let (res, _idx, _remaining_futures) = select_all(futures).await;

    res
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum OutputControllerConfig {
    Wago(SocketAddr),
}

fn make_output_instance<'a>(
    config: OutputControllerConfig,
    rx: OutputIODataRx,
    tx_feedback: BroadcastIODataTx,
    output_map: OutputContextMap<'a>,
    is_running: &'a bool,
) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>> {
    match config {
        OutputControllerConfig::Wago(socket_addr) => Box::pin(wago_modbus_controller::run(
            socket_addr,
            rx,
            tx_feedback,
            output_map,
            &is_running,
        )),
    }
}

fn make_output_controller_map(io: &IoConfig) -> HashMap<OutputControllerConfig, Vec<String>> {
    let mut map: HashMap<OutputControllerConfig, Vec<String>> = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.outputs {
            match &input.kind {
                OutputKind::WODigital(io) => {
                    // TODO remove unwrap and handle error
                    let remote_addr =
                        SocketAddr::new(io.host.parse().unwrap(), io.port.parse().unwrap());

                    let config = OutputControllerConfig::Wago(remote_addr);

                    if let Some(context) = map.get_mut(&config) {
                        context.push(input.id.clone());
                    } else {
                        map.insert(config, vec![input.id.clone()]);
                    }
                }
                // TODO
                _ => {}
            }
        }
    }

    map
}
