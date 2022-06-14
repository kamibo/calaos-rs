use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::hash::Hash;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::RwLock;
use std::time::Duration;

use crate::calaos_json_protocol;
use crate::config;
use crate::io::time_controller;
use crate::io::wago_controller;
use crate::io::wago_modbus_controller;
use crate::io_value;

use futures::future::select_all;

use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use tracing::*;

use calaos_json_protocol::HomeData;

use config::io::InputKind;
use config::io::IoConfig;
use config::io::OutputKind;

use config::rules::ConditionKind;
use config::rules::RulesConfig;

use io_value::IOAction;
use io_value::IOValue;

#[derive(Debug)]
pub struct InputSharedContext<'a> {
    pub input: &'a config::io::Input,
    pub rules: Vec<&'a config::rules::Rule>,
    pub value: Box<RwLock<Option<IOValue>>>,
}

#[derive(Debug)]
pub struct InputContext<'a> {
    pub input: &'a config::io::Input,
    pub value: Option<IOValue>,
}

#[derive(Debug)]
pub struct OutputSharedContext<'a> {
    pub output: &'a config::io::Output,
    pub value: Box<RwLock<Option<IOValue>>>,
}

#[derive(Debug)]
pub struct OutputContext<'a> {
    pub output: &'a config::io::Output,
    pub value: Option<IOValue>,
}

pub type InputSharedContextMap<'a> = HashMap<&'a str, InputSharedContext<'a>>;
pub type OutputSharedContextMap<'a> = HashMap<&'a str, OutputSharedContext<'a>>;

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
}

#[derive(Debug)]
pub struct AnyRequest<Q, A> {
    request: Q,
    sender: oneshot::Sender<A>,
}

impl<Q, A> AnyRequest<Q, A> {
    pub fn new(req: Q) -> (Self, oneshot::Receiver<A>) {
        let (tx, rx) = oneshot::channel::<A>();
        (
            Self {
                request: req,
                sender: tx,
            },
            rx,
        )
    }

    pub fn answer(self, data: A) {
        _ = self.sender.send(data)
    }

    pub fn get(&self) -> &Q {
        &self.request
    }
}

pub type HomeDataRequest = AnyRequest<(), HomeData>;

#[derive(Debug)]
pub enum IORequest {
    GetAllData(HomeDataRequest),
}

#[derive(Debug)]
pub enum IODataCmd {
    Action(IODataAction),
    Request(IORequest),
}

const CHANNEL_CAPACITY: usize = 10_000;

pub type BroadcastIODataRx = broadcast::Receiver<IOData>;
pub type BroadcastIODataTx = broadcast::Sender<IOData>;

pub type MpscIODataActionRx = mpsc::Receiver<IODataAction>;
pub type MpscIODataActionTx = mpsc::Sender<IODataAction>;

pub type MpscIODataCmdRx = mpsc::Receiver<IODataCmd>;
pub type MpscIODataCmdTx = mpsc::Sender<IODataCmd>;

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

pub struct SingleSubChannel<T> {
    tx: mpsc::Sender<T>,
    rx: Option<mpsc::Receiver<T>>,
}

impl<T> SingleSubChannel<T> {
    pub fn subscribe(&mut self) -> Option<mpsc::Receiver<T>> {
        self.rx.take()
    }

    pub fn advertise(&self) -> mpsc::Sender<T> {
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

fn make_singlesub_channel<T>() -> SingleSubChannel<T> {
    let (tx, rx) = mpsc::channel::<T>(CHANNEL_CAPACITY);
    SingleSubChannel { tx, rx: Some(rx) }
}

pub fn make_iodataaction_mpsc_channel() -> SingleSubChannel<IODataAction> {
    make_singlesub_channel()
}

pub type OutputIODataActionRx = mpsc::Receiver<IODataAction>;
pub type OutputIODataActionTx = mpsc::Sender<IODataAction>;

pub fn make_iodatacmd_mpsc_channel() -> SingleSubChannel<IODataCmd> {
    make_singlesub_channel()
}

pub type OutputIODataCmdRx = mpsc::Receiver<IODataCmd>;
pub type OutputIODataCmdTx = mpsc::Sender<IODataCmd>;

fn make_iodataaction_output_channel() -> (OutputIODataActionTx, OutputIODataActionRx) {
    mpsc::channel::<IODataAction>(CHANNEL_CAPACITY)
}

pub fn make_input_context_map<'a>(
    io: &'a IoConfig,
    rules_config: &'a RulesConfig,
) -> InputSharedContextMap<'a> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
            if map
                .insert(
                    input.id.as_str(),
                    InputSharedContext {
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

pub fn make_output_context_map(io: &IoConfig) -> OutputSharedContextMap<'_> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for output in &room.outputs {
            if map
                .insert(
                    output.id.as_str(),
                    OutputSharedContext {
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

pub async fn run_input_controllers(
    io_config: &IoConfig,
    input_map: &InputSharedContextMap<'_>,
    tx_command: MpscIODataCmdTx,
) -> Result<(), Box<dyn Error>> {
    let mut futures: Vec<PinDynFuture> = vec![];

    for (input_config, ids) in make_input_controller_map(io_config) {
        fn filter_input<'a>(
            input_map: &InputSharedContextMap<'a>,
            ids: &[String],
        ) -> InputContextMap<'a> {
            let mut res = HashMap::new();
            for id in ids {
                let entry = input_map.get_key_value(id.as_str()).unwrap();
                res.insert(
                    *entry.0,
                    InputContext {
                        input: entry.1.input,
                        value: entry.1.value.read().unwrap().clone(),
                    },
                );
            }
            res
        }

        let instance_input_map = filter_input(input_map, &ids);

        futures.push(make_input_instance(
            input_config,
            tx_command.clone(),
            instance_input_map,
        ));
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
    Time,
}

fn make_input_instance(
    config: InputControllerConfig,
    tx_command: MpscIODataCmdTx,
    input_map: InputContextMap,
) -> PinDynFuture {
    match config {
        InputControllerConfig::Wago(config) => Box::pin(wago_controller::run(config)),
        InputControllerConfig::Time => Box::pin(time_controller::run(tx_command, input_map)),
    }
}

fn make_input_controller_map(io: &IoConfig) -> HashMap<InputControllerConfig, Vec<String>> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
            let config = match &input.kind {
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

                    InputControllerConfig::Wago(config)
                }
                InputKind::InputTime(_) => InputControllerConfig::Time,
                InputKind::Scenario => continue, // nothing to do
                InputKind::MySensorsInputAnalog | InputKind::MySensorsInputTemp => continue, // TODO
            };

            append_map(config, &mut map, input.id.clone());
        }
    }

    map
}

pub async fn run_output_controllers(
    io_config: &IoConfig,
    output_map: &OutputSharedContextMap<'_>,
    rx: MpscIODataActionRx,
    tx_feedback: BroadcastIODataTx,
) -> Result<(), Box<dyn Error>> {
    let mut futures: Vec<PinDynFuture> = vec![];

    type ReverseMap = HashMap<String, OutputIODataActionTx>;

    let mut reverse_map: ReverseMap = HashMap::new();

    for (output_config, ids) in make_output_controller_map(io_config) {
        let (tx, rx) = make_iodataaction_output_channel();

        fn filter_output<'a>(
            output_map: &OutputSharedContextMap<'a>,
            ids: &[String],
        ) -> OutputContextMap<'a> {
            let mut res = HashMap::new();
            for id in ids {
                let entry = output_map.get_key_value(id.as_str()).unwrap();
                res.insert(
                    *entry.0,
                    OutputContext {
                        output: entry.1.output,
                        value: entry.1.value.read().unwrap().clone(),
                    },
                );
            }
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
        mut rx: MpscIODataActionRx,
        reverse_map: ReverseMap,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let io_data_opt = rx.recv().await;

            match io_data_opt {
                None => break,
                Some(io_data) => {
                    trace!("Received new output to dispatch {:?}", io_data);

                    if let Some(tx) = reverse_map.get(&io_data.id) {
                        tx.send(io_data).await?;
                    } else {
                        warn!("Cannot dispatch {:?} to output controller", io_data);
                    }
                }
            }
        }

        Ok(())
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
        for output in &room.outputs {
            let config = match &output.kind {
                OutputKind::WODigital(io) => {
                    let remote_addr =
                        SocketAddr::new(io.host.parse().unwrap(), io.port.parse().unwrap());
                    OutputControllerConfig::Wago(remote_addr)
                }
                OutputKind::WOShutter(io) => {
                    let remote_addr =
                        SocketAddr::new(io.host.parse().unwrap(), io.port.parse().unwrap());
                    OutputControllerConfig::Wago(remote_addr)
                }

                // TODO
                _ => continue,
            };

            append_map(config, &mut map, output.id.clone());
        }
    }

    map
}

fn append_map<T: Eq + Hash>(config: T, map: &mut HashMap<T, Vec<String>>, id: String) {
    if let Some(context) = map.get_mut(&config) {
        context.push(id);
    } else {
        map.insert(config, vec![id]);
    }
}
