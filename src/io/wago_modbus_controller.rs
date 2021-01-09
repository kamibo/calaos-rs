use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::time::sleep;

use tokio_modbus::client::tcp;

use tracing::*;

use crate::io_config;
use crate::io_context;

use io_config::OutputKind;
use io_context::IOValue;
use io_context::OutputContextMap;
use io_context::OutputIODataRx;

use tokio_modbus::prelude::Reader;
use tokio_modbus::prelude::Writer;

pub async fn run<'a>(
    remote_addr: SocketAddr,
    mut rx: OutputIODataRx,
    output_map: OutputContextMap<'a>,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    info!("Starting wago modbus ({:?})", remote_addr);
    debug!("Output handling ids: {:?}", output_map.keys());

    let mut modbus_client = tcp::connect(remote_addr).await?;

    for kv in &output_map {
        match &kv.1.output.kind {
            OutputKind::WODigital(io) => {
                debug!("Ask read var {:?}", kv.0);
                read_var(&mut modbus_client, io.var).await?;
            }
            _ => {}
        }
    }

    while *is_running {
        if let Some(io_data) = rx.recv().await {
            if let Some(ctx) = output_map.get(io_data.id.as_str()) {
                match &ctx.output.kind {
                    OutputKind::WODigital(io) => match io_data.value {
                        IOValue::Bool(value) => {
                            switch_var(&mut modbus_client, io.var, value).await?;
                        }
                        _ => {
                            warn!("Cannot handle value");
                        }
                    },
                    OutputKind::WOVolet(io) => {
                        switch_var(&mut modbus_client, io.var_up, true).await?;
                        sleep(Duration::from_secs(3)).await;
                        switch_var(&mut modbus_client, io.var_up, false).await?;
                    }
                    _ => {
                        warn!("Output {:?} not implemented", io_data);
                    } // TODO
                }
            } else {
                warn!("ID {:?} is not valid output", io_data);
            }
        }
    }

    Ok(())
}

async fn read_var(modbus_client: &mut dyn Reader, var: u32) -> Result<(), Box<dyn Error>> {
    let address = u16::try_from(var + 0x200)?;
    let values = modbus_client.read_discrete_inputs(address, 1).await?;
    debug!("Read coil address: {:?} value: {:?}", var, values[0]);
    Ok(())
}

async fn switch_var(
    modbus_client: &mut dyn Writer,
    var: u32,
    value: bool,
) -> Result<(), Box<dyn Error>> {
    trace!("Send var: {:?}, value: {:?}", var, value);
    let address = u16::try_from(var | 0x1000)?;
    modbus_client.write_single_coil(address, value).await?;

    Ok(())
}
