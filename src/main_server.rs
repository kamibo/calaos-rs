use std::error::Error;
use std::net::SocketAddr;
use std::str;

use tokio::net::UdpSocket;

use tracing::{debug, info};

use crate::calaos_protocol;

const MAX_DATAGRAM_SIZE: usize = 65_507;

pub async fn run(local_addr: SocketAddr, is_running: &bool) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(local_addr).await?;

    async_udp_read(&socket, &is_running).await?;

    Ok(())
}

async fn async_udp_read(socket: &UdpSocket, is_running: &bool) -> Result<(), Box<dyn Error>> {
    info!("Start read");

    while *is_running {
        let mut data = vec![0u8; MAX_DATAGRAM_SIZE];
        let (len, addr) = socket.recv_from(&mut data).await?;
        let request = calaos_protocol::parse_request(str::from_utf8(&data)?)?;
        debug!("Received {} bytes from {}: {:?}", len, addr, request);
    }

    Ok(())
}
