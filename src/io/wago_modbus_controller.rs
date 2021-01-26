use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::time::sleep;

use tokio_modbus::client::tcp;

use tracing::*;

use crate::io_config;
use crate::io_context;
use crate::io_value;

use io_config::OutputKind;
use io_context::BroadcastIODataTx;
use io_context::IOData;
use io_context::OutputContextMap;
use io_context::OutputIODataRx;

use io_value::IOValue;

use tokio_modbus::prelude::Reader;
use tokio_modbus::prelude::Writer;

pub async fn run(
    remote_addr: SocketAddr,
    mut rx: OutputIODataRx,
    tx_feedback: BroadcastIODataTx,
    mut output_map: OutputContextMap<'_>,
    is_running: &bool,
) -> Result<(), Box<dyn Error>> {
    info!("Starting wago modbus ({:?})", remote_addr);
    debug!("Output handling ids: {:?}", output_map.keys());

    let mut modbus_client = tcp::connect(remote_addr).await?;

    for (&id, context) in &mut output_map {
        #[allow(clippy::single_match)]
        match &context.output.kind {
            OutputKind::WODigital(io) => {
                debug!("Ask read var {:?}", id);
                let value = IOValue::Bool(read_var(&mut modbus_client, io.var).await?);
                *context.value.get_mut().unwrap() = Some(value.clone());
                tx_feedback.send(IOData {
                    id: String::from(id),
                    value,
                })?;
            }
            _ => {}
        }
    }

    while *is_running {
        if let Some(io_data) = rx.recv().await {
            let id = io_data.id.clone();

            if let Some(ctx) = output_map.get(id.as_str()) {
                match &ctx.output.kind {
                    OutputKind::WODigital(io) => match io_data.value {
                        IOValue::Bool(value) => {
                            set_var(&mut modbus_client, io.var, value).await?;
                            tx_feedback.send(IOData {
                                id,
                                value: IOValue::Bool(value),
                            })?;
                        }
                        _ => {
                            warn!("Cannot handle value");
                        }
                    },
                    OutputKind::WOVolet(io) => {
                        set_var(&mut modbus_client, io.var_up, true).await?;
                        sleep(Duration::from_secs(3)).await;
                        set_var(&mut modbus_client, io.var_up, false).await?;
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

async fn read_var(modbus_client: &mut dyn Reader, var: u32) -> Result<bool, Box<dyn Error>> {
    let address = u16::try_from(var + 0x200)?;
    let values = modbus_client.read_discrete_inputs(address, 1).await?;
    let result = values[0];
    trace!("Read coil address: {:?} value: {:?}", var, result);
    Ok(result)
}

async fn set_var(
    modbus_client: &mut dyn Writer,
    var: u32,
    value: bool,
) -> Result<(), Box<dyn Error>> {
    trace!("Send var: {:?}, value: {:?}", var, value);
    let address = u16::try_from(var | 0x1000)?;
    modbus_client.write_single_coil(address, value).await?;

    Ok(())
}
