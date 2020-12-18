use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;

use tokio_modbus::client::tcp;

use tracing::*;

use crate::io_config;
use crate::io_context;

use io_config::OutputKind;
use io_context::OutputContextMap;

use tokio_modbus::prelude::Writer;

pub async fn run<'a>(
    remote_addr: SocketAddr,
    mut rx: mpsc::Receiver<String>,
    output_map: &OutputContextMap<'a>,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    let mut modbus_client = tcp::connect(remote_addr).await?;

    while *is_running {
        if let Some(var) = rx.recv().await {
            if let Some(ctx) = output_map.get(var.as_str()) {
                match &ctx.output.kind {
                    OutputKind::WODigital(io) => {
                        switch_var(&mut modbus_client, io.var).await?;
                    }
                    OutputKind::WOVolet(io) => {
                        switch_var(&mut modbus_client, io.var_up).await?;
                    }
                    _ => {
                        warn!("Output {:?} not implemented", var);
                    } // TODO
                }
            } else {
                warn!("ID {:?} is not valid output", var);
            }
        }
    }

    Ok(())
}

async fn switch_var(modbus_client: &mut dyn Writer, var: u32) -> Result<(), Box<dyn Error>> {
    trace!("Send {:?}", var);
    let address = u16::try_from(var | 0x1000)?;
    modbus_client.write_single_coil(address, true).await?;
    sleep(Duration::from_secs(3)).await;
    modbus_client.write_single_coil(address, false).await?;

    Ok(())
}
