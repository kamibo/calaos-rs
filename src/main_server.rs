use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::str;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use tracing::*;

use crate::calaos_protocol;
use crate::io_config;

use io_config::InputKind;
use io_config::IoConfig;

const MAX_DATAGRAM_SIZE: usize = 65_507;

pub async fn run<'a>(
    local_addr: SocketAddr,
    io_config: &'a IoConfig,
    tx: mpsc::Sender<&'a str>,
    is_running: &bool,
) -> Result<(), Box<dyn Error + 'a>> {
    let input_var_map = make_input_var_map(io_config);
    let socket = UdpSocket::bind(local_addr).await?;

    info!("Start read {:?}", socket);

    while *is_running {
        match async_udp_read(&socket).await? {
            calaos_protocol::Request::WagoInt(data) => {
                if let Some(input) = input_var_map.get(&data.var) {
                    info!(
                        "Received wago data {:?} input {:?} (id: {:?})",
                        data, input.name, input.id
                    );
                    tx.send(input.id.as_str()).await?;
                } else {
                    warn!("Received unknown wago var {:?}", data.var);
                }
            }
            calaos_protocol::Request::Discover => {
                // TODO
            }
        }
    }

    Ok(())
}

fn make_input_var_map<'a>(io: &'a IoConfig) -> HashMap<u32, &'a io_config::Input> {
    let mut map = HashMap::new();

    for room in &io.home.rooms {
        for input in &room.inputs {
            match &input.kind {
                InputKind::WIDigitalBP(io) => {
                    if let Some(_) = map.insert(io.var, input) {
                        warn!("Same IO var {} used multiple is not supported", io.var);
                    }
                }
                _ => {}
            }
        }
    }

    map
}

async fn async_udp_read(socket: &UdpSocket) -> Result<calaos_protocol::Request, Box<dyn Error>> {
    let mut data = vec![0u8; MAX_DATAGRAM_SIZE];
    let (len, addr) = socket.recv_from(&mut data).await?;
    let data_str = str::from_utf8(&data[..len])?;
    trace!("Received {:?}", data_str);
    let request = calaos_protocol::parse_request(data_str)?;
    debug!("Received {} bytes from {}: {:?}", len, addr, request);

    Ok(request)
}
