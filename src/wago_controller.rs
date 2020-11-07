use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::sleep;

use tracing::{debug, info};

pub async fn run(
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    heartbeat_period: Duration,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(local_addr).await?;

    async_send_heartbeat(&socket, remote_addr, heartbeat_period, &is_running).await?;

    Ok(())
}

async fn async_send_heartbeat(
    socket: &UdpSocket,
    remote_addr: SocketAddr,
    heartbeat_period: Duration,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    info!("Start heartbeat service");

    socket.send_to(b"WAGO_GET_INFO", remote_addr).await?;

    let mut data = vec![0u8; 100];
    let (_, addr) = socket.recv_from(&mut data).await?;

    info!("Using IP {:?} as server IP", socket.local_addr()?.ip());

    let set_server_ip_cmd = format!("WAGO_SET_SERVER_IP {}", addr.ip());

    debug!("Set server ip command \"{}\"", set_server_ip_cmd);

    while *is_running {
        socket.send_to(b"WAGO_HEARTBEAT", remote_addr).await?;

        socket
            .send_to(set_server_ip_cmd.as_bytes(), remote_addr)
            .await?;
        debug!("Heartbeat sent");

        sleep(heartbeat_period).await;
    }

    Ok(())
}
