use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Instant;

use tokio_modbus::client::tcp;

use tracing::*;

use crate::io_config;
use crate::io_context;
use crate::io_value;

use io_config::OutputKind;
use io_config::WagoIOUpDown;

use io_context::BroadcastIODataTx;
use io_context::IOData;
use io_context::OutputContextMap;
use io_context::OutputIODataRx;

use io_value::IOValue;
use io_value::ShutterState;

use tokio_modbus::prelude::Reader;
use tokio_modbus::prelude::Writer;

pub async fn run(
    remote_addr: SocketAddr,
    mut rx: OutputIODataRx,
    tx_feedback: BroadcastIODataTx,
    mut output_map: OutputContextMap<'_>,
) -> Result<(), Box<dyn Error>> {
    info!("Starting wago modbus ({:?})", remote_addr);
    debug!("Output handling ids: {:?}", output_map.keys());

    let mut modbus_client = tcp::connect(remote_addr).await?;
    let mut wait_task: HashMap<String, Instant> = HashMap::new();

    for (&id, context) in &mut output_map {
        match &context.output.kind {
            OutputKind::WODigital(io) => {
                debug!("Ask read var {:?}", id);
                let value = IOValue::Bool(read_var(&mut modbus_client, io.var).await?);
                send_feedback(&tx_feedback, context, String::from(id), value)?;
            }
            OutputKind::WOShutter(io) => {
                // Consider arbitrary value for shutter initialization
                stop_shutter(&mut modbus_client, io).await?;
                send_feedback(
                    &tx_feedback,
                    context,
                    String::from(id),
                    IOValue::Shutter(ShutterState::Up),
                )?;
            }
            _ => {}
        }
    }

    loop {
        tokio::select! {
            io_data_opt = rx.recv() => {
                if io_data_opt.is_none() {
                    return Ok(())
                }
                let io_data = io_data_opt.unwrap();
                let id = io_data.id.clone();
                let ctx_opt = output_map.get_mut(id.as_str());

                if ctx_opt.is_none() {
                    warn!("Ignoring invalid ID {:?}", id);
                    continue;
                }

                let ctx = ctx_opt.unwrap();
                let current_value = ctx.value.read().unwrap().clone();

                match &ctx.output.kind {
                    OutputKind::WODigital(io) => match io_data.value {
                        IOValue::Bool(value) => {
                            set_var(&mut modbus_client, io.var, value).await?;
                            send_feedback(&tx_feedback, ctx, id, IOValue::Bool(value))?;
                        }
                        _ => {
                            warn!("Cannot handle value");
                        }
                    },
                    OutputKind::WOShutter(io) => {
                        if current_value.is_some() && current_value.unwrap() == io_data.value {
                            continue;
                        }

                        match &io_data.value {
                            IOValue::Shutter(target_state) => {
                                let (var_on, var_off) = match target_state {
                                    ShutterState::Up => (io.var_up, io.var_down),
                                    ShutterState::Down => (io.var_down, io.var_up),
                                };

                                set_var_off(&mut modbus_client, var_off).await?;
                                set_var_on(&mut modbus_client, var_on).await?;
                                wait_task.insert(id, Instant::now() + io.time);
                            }
                            _ => {
                                warn!("Cannot handle value {:?} for {:?}", io_data.value, io)
                            }
                        }
                    }
                    _ => {
                        warn!("Output {:?} not implemented", io_data);
                    } // TODO
                }
            },
            _ = wait_for_instant(wait_task.values().min()) => {
                let now = Instant::now();
                for (id, instant) in &wait_task {
                    if now > *instant {
                        continue;
                    }

                    let ctx = output_map.get_mut(id.as_str()).unwrap();
                    match &ctx.output.kind {
                        OutputKind::WOShutter(io) => {
                            stop_shutter(&mut modbus_client, io).await?;
                            // TODO update state
                        }
                        _ => {
                            warn!("Unexpected task");
                        }
                }

                }
            }
        }
    }
}

async fn wait_for_instant(opt_instant: Option<&Instant>) {
    if opt_instant.is_none() {
        futures::future::pending().await
    } else {
        tokio::time::sleep_until((*opt_instant.unwrap()).into()).await
    }
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
async fn set_var_off(modbus_client: &mut dyn Writer, var: u32) -> Result<(), Box<dyn Error>> {
    set_var(modbus_client, var, false).await
}

async fn set_var_on(modbus_client: &mut dyn Writer, var: u32) -> Result<(), Box<dyn Error>> {
    set_var(modbus_client, var, true).await
}

async fn stop_shutter(
    modbus_client: &mut dyn Writer,
    io: &WagoIOUpDown,
) -> Result<(), Box<dyn Error>> {
    set_var(modbus_client, io.var_down, false).await?;
    set_var(modbus_client, io.var_up, false).await?;
    Ok(())
}

fn send_feedback<'a>(
    tx_feedback: &BroadcastIODataTx,
    context: &mut io_context::OutputContext<'a>,
    id: String,
    value: IOValue,
) -> Result<(), Box<dyn Error>> {
    *context.value.get_mut().unwrap() = Some(value.clone());
    tx_feedback.send(IOData::new(id, value))?;
    Ok(())
}
