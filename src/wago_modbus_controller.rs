use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;

use tokio_modbus::client::tcp;

use tracing::*;

pub async fn run(
    remote_addr: SocketAddr,
    mut rx: mpsc::Receiver<u32>,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    use tokio_modbus::prelude::Writer;

    let mut ctx = tcp::connect(remote_addr).await?;

    while *is_running {
        if let Some(var) = rx.recv().await {
            trace!("Send {:?}", var);
            let address = u16::try_from(var | 0x1000)?;
            ctx.write_single_coil(address, true).await?;
            sleep(Duration::from_secs(3)).await;
            ctx.write_single_coil(address, false).await?;
        }
    }

    Ok(())
}
