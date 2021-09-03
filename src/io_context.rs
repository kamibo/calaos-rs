use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::RwLock;
use std::time::Duration;

use crate::io::wago_controller;
use crate::io::wago_modbus_controller;
use crate::io_config;
use crate::io_value;
use crate::io_value::IOAction;
use crate::rules_config;

use futures::future::select_all;

use tokio::sync::broadcast;
use tokio::sync::mpsc;

use tracing::*;

use io_config::InputKind;
use io_config::IoConfig;
use io_config::OutputKind;

use rules_config::ConditionKind;
use rules_config::RulesConfig;

use io_value::IOValue;

pub struct InputContext<'a> {
    pub input: &'a io_config::Input,
    pub rules: Vec<&'a rules_config::Rule>,
    pub value: Box<RwLock<Option<IOValue>>>,
}

#[derive(Debug)]
pub struct OutputContext<'a> {
    pub output: &'a io_config::Output,
    pub value: Box<RwLock<Option<IOValue>>>,
}

impl<'a> Clone for OutputContext<'a> {
    fn clone(&self) -> Self {
        let value = self.value.read().unwrap().clone();

        OutputContext {
            output: <&io_config::Output>::clone(&self.output),
            value: Box::new(RwLock::new(value)),
        }
    }
}

pub type InputContextMap<'a> = HashMap<&'a str, InputContext<'a>>;
pub type OutputContextMap<'a> = HashMap<&'a str, OutputContext<'a>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IOData {
    pub id: String,
    #[serde(rename = "state")]
    pub value: IOValue,
}

impl IOData {
    pub fn new(id: String, value: IOValue) -> Self {
        Self { id, value }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IODataAction {
    pub id: String,
    #[serde(rename = "state")]
    pub action: IOAction,
}

impl IODataAction {
    pub fn new(id: String, action: IOAction) -> Self {
        Self { id, action }
    }

    pub fn from_value(id: String, value: IOValue) -> Self {
        Self {
            id,
            action: IOAction::SetValue(value),
        }
    }
}

const CHANNEL_CAPACITY: usize = 1000;

pub type BroadcastIODataRx = broadcast::Receiver<IOData>;
pub type BroadcastIODataTx = broadcast::Sender<IOData>;

pub type BroadcastIODataActionRx = broadcast::Receiver<IODataAction>;
pub type BroadcastIODataActionTx = broadcast::Sender<IODataAction>;

pub struct Channel<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone> Channel<T> {
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    pub fn advertise(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }
}

fn make_broadcast_channel<T: Clone>() -> Channel<T> {
    let (tx, _) = broadcast::channel::<T>(CHANNEL_CAPACITY);
    Channel { tx }
}

pub fn make_iodata_broadcast_channel() -> Channel<IOData> {
    make_broadcast_channel()
}

pub fn make_iodataaction_broadcast_channel() -> Channel<IODataAction> {
    make_broadcast_channel()
}

pub type OutputIODataActionRx = mpsc::Receiver<IODataAction>;
pub type OutputIODataActionTx = mpsc::Sender<IODataAction>;

fn make_iodataaction_output_channel() -> (OutputIODataActionTx, OutputIODataActionRx) {
    mpsc::channel::<IODataAction>(CHANNEL_CAPACITY)
}

pub fn make_input_context_map<'a>(
    io: &'a IoConfig,
    rules_config: &'a RulesConfig,
) -> InputContextMap<'a> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
            if map
                .insert(
                    input.id.as_str(),
                    InputContext {
                        input,
                        rules: Vec::new(),
                        value: Box::new(RwLock::new(None)),
                    },
                )
                .is_some()
            {
                warn!("IO input ID {:?} is not unique", input.id);
            }
        }
    }

    for rule in &rules_config.rules {
        for condition in &rule.conditions {
            let id = match &condition {
                ConditionKind::Start => {
                    // TODO
                    continue;
                }
                ConditionKind::Standard { input } => &input.id,
            };

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

pub fn make_output_context_map(io: &IoConfig) -> OutputContextMap<'_> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for output in &room.outputs {
            if map
                .insert(
                    output.id.as_str(),
                    OutputContext {
                        output,
                        value: Box::new(RwLock::new(None)),
                    },
                )
                .is_some()
            {
                warn!("IO output ID {:?} is not unique", output.id);
            }
        }
    }

    map
}

type PinDynFuture<'a> = Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>>;

pub async fn run_input_controllers(io_config: &IoConfig) -> Result<(), Box<dyn Error>> {
    let mut futures: Vec<PinDynFuture> = vec![];

    for input_config in make_input_controller_set(io_config) {
        futures.push(make_input_instance(input_config));
    }

    let (res, _idx, _remaining_futures) = select_all(futures).await;

    res
}

// Use interior mutability pattern to safely write value
pub fn write_io_value(target: &RwLock<Option<IOValue>>, value: IOValue) {
    let mut value_write = target.write().unwrap();
    *value_write = Some(value);
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum InputControllerConfig {
    Wago(wago_controller::Config),
}

fn make_input_instance<'a>(config: InputControllerConfig) -> PinDynFuture<'a> {
    match config {
        InputControllerConfig::Wago(config) => Box::pin(wago_controller::run(config)),
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

pub async fn run_output_controllers(
    io_config: &IoConfig,
    output_map: &OutputContextMap<'_>, // TODO FIX
    rx: BroadcastIODataActionRx,
    tx_feedback: BroadcastIODataTx,
) -> Result<(), Box<dyn Error>> {
    let mut futures: Vec<PinDynFuture> = vec![];

    type ReverseMap = HashMap<String, OutputIODataActionTx>;

    let mut reverse_map: ReverseMap = HashMap::new();

    for (output_config, ids) in make_output_controller_map(io_config) {
        let (tx, rx) = make_iodataaction_output_channel();

        fn filter_output<'a>(
            output_map: &OutputContextMap<'a>,
            ids: &[String],
        ) -> OutputContextMap<'a> {
            let mut res = (*output_map).clone();
            res.retain(|k, _| ids.iter().any(|x| k == x));
            res
        }

        let instance_output_map = filter_output(output_map, &ids);

        futures.push(make_output_instance(
            output_config,
            rx,
            tx_feedback.clone(),
            instance_output_map,
        ));

        for id in ids {
            reverse_map.insert(id, tx.clone());
        }
    }

    async fn handle_rx(
        mut rx: BroadcastIODataActionRx,
        reverse_map: ReverseMap,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let io_data = rx.recv().await?;

            trace!("Received new output to dispatch {:?}", io_data);

            if let Some(tx) = reverse_map.get(&io_data.id) {
                tx.send(io_data).await?;
            } else {
                warn!("Cannot dispatch {:?} to output controller", io_data);
            }
        }
    }

    futures.push(Box::pin(handle_rx(rx, reverse_map)));

    let (res, _idx, _remaining_futures) = select_all(futures).await;

    res
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum OutputControllerConfig {
    Wago(SocketAddr),
}

fn make_output_instance(
    config: OutputControllerConfig,
    rx: OutputIODataActionRx,
    tx_feedback: BroadcastIODataTx,
    output_map: OutputContextMap,
) -> PinDynFuture {
    match config {
        OutputControllerConfig::Wago(socket_addr) => Box::pin(wago_modbus_controller::run(
            socket_addr,
            rx,
            tx_feedback,
            output_map,
        )),
    }
}

fn make_output_controller_map(io: &IoConfig) -> HashMap<OutputControllerConfig, Vec<String>> {
    let mut map: HashMap<OutputControllerConfig, Vec<String>> = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.outputs {
            #[allow(clippy::single_match)]
            match &input.kind {
                OutputKind::WODigital(io) => {
                    let remote_addr =
                        SocketAddr::new(io.host.parse().unwrap(), io.port.parse().unwrap());
                    add_wago_output(&mut map, input.id.clone(), remote_addr);
                }
                OutputKind::WOShutter(io) => {
                    let remote_addr =
                        SocketAddr::new(io.host.parse().unwrap(), io.port.parse().unwrap());
                    add_wago_output(&mut map, input.id.clone(), remote_addr);
                }

                // TODO
                _ => {}
            }
        }
    }

    map
}

fn add_wago_output(
    map: &mut HashMap<OutputControllerConfig, Vec<String>>,
    id: String,
    remote_addr: SocketAddr,
) {
    // TODO remove unwrap and handle error

    let config = OutputControllerConfig::Wago(remote_addr);

    if let Some(context) = map.get_mut(&config) {
        context.push(id);
    } else {
        map.insert(config, vec![id]);
    }
}
