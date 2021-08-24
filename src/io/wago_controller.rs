use std::error::Error;
use std::net::SocketAddr;
use std::str;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::sleep;

use tracing::*;

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct Config {
    pub remote_addr: SocketAddr,
    pub heartbeat_period: Duration,
}

pub async fn run(wago_config: Config) -> Result<(), Box<dyn Error>> {
    info!("Start controller ({:?})", wago_config);
    let socket = UdpSocket::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).await?;
    socket.connect(wago_config.remote_addr).await?;

    async_send_heartbeat(
        &socket,
        wago_config.remote_addr,
        wago_config.heartbeat_period,
    )
    .await?;

    Ok(())
}

async fn async_send_heartbeat(
    socket: &UdpSocket,
    remote_addr: SocketAddr,
    heartbeat_period: Duration,
) -> Result<(), Box<dyn Error>> {
    info!("Start heartbeat service");

    let ip = socket.local_addr()?.ip();

    info!("Using IP {:?} as server IP", ip);

    print_info(socket).await?;

    let set_server_ip_cmd = format!("WAGO_SET_SERVER_IP {}", ip);

    debug!("Set server ip command \"{}\"", set_server_ip_cmd);

    loop {
        socket.send_to(b"WAGO_HEARTBEAT", remote_addr).await?;

        socket
            .send_to(set_server_ip_cmd.as_bytes(), remote_addr)
            .await?;
        debug!("Heartbeat sent");

        sleep(heartbeat_period).await;
    }
}

async fn print_info(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    socket.send(b"WAGO_GET_INFO").await?;
    let mut data = vec![0u8; 100];
    let len = socket.recv(&mut data).await?;
    let data_str = str::from_utf8(&data[..len - 1])?;

    debug!("Wago info {:?}", data_str);

    Ok(())
}
