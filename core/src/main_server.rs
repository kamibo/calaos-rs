use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::str;
use std::time::Duration;
use std::time::Instant;
// use std::sync::Arc;

use tokio::net::UdpSocket;

use tracing::*;

use crate::calaos_protocol;
use crate::config;
use crate::io_context;
use crate::io_value;

use config::io::Input;
use config::io::InputKind;
use config::io::IoConfig;

use io_context::IODataCmd;
use io_context::MpscIODataCmdTx;
use io_context::{BroadcastIODataTx, IOData, IODataAction};

use io_value::IOAction;
use io_value::IOValue;

// MQTT is initialized in server main; no direct use here

const MAX_DATAGRAM_SIZE: usize = 65_507;

pub async fn run<'a>(
    local_addr: SocketAddr,
    io_config: &'a IoConfig,
    tx: MpscIODataCmdTx,
    tx_feedback: BroadcastIODataTx,
) -> Result<(), Box<dyn Error + 'a>> {
    let input_var_map = make_input_var_map(io_config);
    let socket = UdpSocket::bind(local_addr).await?;

    info!("Start read {:?}", socket);
    let mut last_change: HashMap<u32, Instant> = HashMap::new();

    // MQTT setup occurs in the binary (server/src/main.rs)

    loop {
        match async_udp_read(&socket).await? {
            (calaos_protocol::Request::WagoInt(data), _) => {
                if let Some(input) = input_var_map.get(&data.var) {
                    info!(
                        "Received wago data {:?} input {:?} (id: {:?})",
                        data, input.name, input.id
                    );

                    let data_bool = to_bool(data.value);
                    // Always broadcast raw input state for MQTT/Websocket
                    let _ =
                        tx_feedback.send(IOData::new(input.id.clone(), IOValue::Bool(data_bool)));

                    let value = match input.kind {
                        InputKind::WIDigitalBP(_) => IOValue::Bool(data_bool),
                        InputKind::WIDigitalLong(_) => {
                            if data_bool {
                                last_change.insert(data.var, Instant::now());
                                continue;
                            } else {
                                let instant =
                                    last_change.remove(&data.var).unwrap_or_else(Instant::now);

                                if (Instant::now() - instant) < Duration::from_millis(400) {
                                    IOValue::Int(1)
                                } else {
                                    IOValue::Int(2)
                                }
                                // TODO emit Int(0) after 250ms
                            }
                        }
                        InputKind::WIDigitalTriple(_) => {
                            // TODO Handle triple press button
                            continue;
                        }
                        _ => {
                            warn!("Unexpected input type: {:?}", input);
                            continue;
                        }
                    };

                    tx.send(IODataCmd::Action(IODataAction::new(
                        input.id.clone(),
                        IOAction::SetValue(value),
                    )))
                    .await?;
                } else {
                    warn!("Received unknown wago var {:?}", data.var);
                }
            }
            (calaos_protocol::Request::Discover, remote_addr) => {
                socket.send_to(b"CALAOS_IP 127.0.0.1", remote_addr).await?;
            }
        }
    }
}

fn to_bool(value: u32) -> bool {
    value != 0
}

fn make_input_var_map(io: &IoConfig) -> HashMap<u32, &Input> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
            #[allow(clippy::single_match)]
            match &input.kind {
                InputKind::WIDigitalBP(io) | InputKind::WIDigitalLong(io) => {
                    if map.insert(io.var, input).is_some() {
                        warn!("Same IO var {} used multiple is not supported", io.var);
                    }
                }
                _ => {}
            }
        }
    }

    map
}

async fn async_udp_read(
    socket: &UdpSocket,
) -> Result<(calaos_protocol::Request, SocketAddr), Box<dyn Error>> {
    let mut data = vec![0u8; MAX_DATAGRAM_SIZE];
    let (len, addr) = socket.recv_from(&mut data).await?;
    let data_str = str::from_utf8(&data[..len])?;
    trace!("Received {:?}", data_str);
    let request = calaos_protocol::parse_request(data_str)?;
    debug!("Received {} bytes from {}: {:?}", len, addr, request);

    Ok((request, addr))
}

#[cfg_attr(not(feature = "net-tests"), ignore)]
#[tokio::test]
async fn discover_test() {
    // Bind to an ephemeral local port to avoid conflicts in CI/test environments
    let probe = UdpSocket::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .expect("Probe bind failed");
    let local_addr: SocketAddr = probe.local_addr().expect("No local addr");
    drop(probe);
    let io_config: IoConfig = Default::default();
    let channel = io_context::make_iodatacmd_mpsc_channel();
    let feedback = io_context::make_iodata_broadcast_channel();

    debug!("Local addr {:?}", local_addr);

    async fn test(local_addr: &SocketAddr) {
        let socket = UdpSocket::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap())
            .await
            .expect("Bad address");
        socket.connect(local_addr).await.expect("Connect issue");
        socket.send(b"CALAOS_DISCOVER").await.expect("Send issue");
        let mut data = vec![0u8; MAX_DATAGRAM_SIZE];
        let (len, _) = socket.recv_from(&mut data).await.expect("Read error");
        let data_str = str::from_utf8(&data[..len]).expect("Bad string");
        assert_eq!(data_str, "CALAOS_IP 127.0.0.1");
    }

    tokio::select! {
        res = run(local_addr, &io_config, channel.advertise(), feedback.advertise()) => {
            assert!(res.is_ok());
        },
        _ = test(&local_addr) => {
        }
    }
}
