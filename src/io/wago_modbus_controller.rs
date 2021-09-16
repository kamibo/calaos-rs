use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Instant;

use tracing::*;

use crate::io::modbus_client;
use crate::io_config;
use crate::io_context;
use crate::io_value;
use crate::task;

use io_config::OutputKind;
use io_config::WagoIOUpDown;

use io_context::BroadcastIODataTx;
use io_context::IOData;
use io_context::OutputContextMap;
use io_context::OutputIODataActionRx;

use io_value::IOAction;
use io_value::IOValue;
use io_value::ShutterState;

use task::Task;

use modbus_client::ModbusClient;

const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

pub async fn run(
    remote_addr: SocketAddr,
    mut rx: OutputIODataActionRx,
    tx_feedback: BroadcastIODataTx,
    mut output_map: OutputContextMap<'_>,
) -> Result<(), Box<dyn Error>> {
    info!("Starting wago modbus ({:?})", remote_addr);
    debug!("Output handling ids: {:?}", output_map.keys());

    let mut modbus_client = ModbusClient::connect(remote_addr).await?;
    let mut wait_task: HashMap<String, Task> = HashMap::new();

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
                debug!("New command received {:?}", io_data_opt);

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
                let current_value = &ctx.value;

                let new_value : Option<IOValue> = match io_data.action {
                    IOAction::SetValue(v) => Some(v),
                    IOAction::Toggle => {
                      if current_value.is_none() {
                          continue;
                      }

                      if let Some(v) = io_value::toggle(current_value.as_ref().unwrap()) {
                          Some(v)
                      } else {
                        continue;
                      }

                    },
                    IOAction::Stop => None
                };

                match &ctx.output.kind {
                    OutputKind::WODigital(io) => match new_value {
                        Some(IOValue::Bool(value)) => {
                            set_var(&mut modbus_client, io.var, value).await?;
                            send_feedback(&tx_feedback, ctx, id, IOValue::Bool(value))?;
                        }
                        _ => {
                            warn!("Cannot handle value");
                        }
                    },
                    OutputKind::WOShutter(io) => {
                        if current_value.is_some() && new_value.is_some() && *current_value == new_value {
                            continue;
                        }

                        match &new_value {
                            Some(IOValue::Shutter(target_state)) => {
                                let (var_on, var_off) = match target_state {
                                    ShutterState::Up => (io.var_up, io.var_down),
                                    ShutterState::Down => (io.var_down, io.var_up),
                                };

                                set_var_off(&mut modbus_client, var_off).await?;
                                set_var_off(&mut modbus_client, var_on).await?;
                                set_var_on(&mut modbus_client, var_on).await?;
                                wait_task.insert(id.clone(), Task::from_now(io.time, IOValue::Shutter(target_state.clone())));
                                send_feedback(&tx_feedback, ctx, id.clone(), IOValue::Shutter(target_state.clone()))?;
                            },
                            None => {
                                stop_shutter(&mut modbus_client, io).await?;
                            },
                            _ => {
                                warn!("Cannot handle value {:?} for {:?}", new_value, io)
                            }
                        }
                    }
                    _ => {
                        warn!("Output {:?} not implemented", id);
                    } // TODO
                }
            },
            _ = wait_for_task(wait_task.values().min()), if !wait_task.is_empty() => {
                let now = Instant::now();

                // Note: better using drain_filter when available
                for (id, task) in &wait_task {
                    if task.deadline > now {
                        continue;
                    }

                    let ctx = output_map.get_mut(id.as_str()).unwrap();
                    match &ctx.output.kind {
                        OutputKind::WOShutter(io) => {
                            stop_shutter(&mut modbus_client, io).await?;
                        }
                        _ => {
                            warn!("Unexpected task");
                        }
                    }
                }

                wait_task.retain(|_, task| task.deadline > now);
            },
            _ = tokio::time::sleep(KEEPALIVE_INTERVAL) => {
                modbus_keepalive(&mut modbus_client).await?;
            }
        }
    }
}

async fn wait_for_task(opt_task: Option<&Task>) {
    if opt_task.is_some() {
        tokio::time::sleep_until((opt_task.unwrap().deadline).into()).await
    }
}

async fn read_var(modbus_client: &mut ModbusClient, var: u32) -> Result<bool, Box<dyn Error>> {
    let address = u16::try_from(var + 0x200)?;
    let value = modbus_client.read_discrete_input(address).await?;
    trace!("Read coil address: {:?} value: {:?}", var, value);
    Ok(value)
}

async fn set_var(
    modbus_client: &mut ModbusClient,
    var: u32,
    value: bool,
) -> Result<(), Box<dyn Error>> {
    trace!("Send var: {:?}, value: {:?}", var, value);
    let address = u16::try_from(var | 0x1000)?;
    modbus_client.write_single_coil(address, value).await?;
    trace!("Var sent: {:?}, value: {:?}", var, value);
    Ok(())
}
async fn set_var_off(modbus_client: &mut ModbusClient, var: u32) -> Result<(), Box<dyn Error>> {
    set_var(modbus_client, var, false).await
}

async fn set_var_on(modbus_client: &mut ModbusClient, var: u32) -> Result<(), Box<dyn Error>> {
    set_var(modbus_client, var, true).await
}

async fn modbus_keepalive(modbus_client: &mut ModbusClient) -> Result<(), Box<dyn Error>> {
    modbus_client.read_discrete_inputs(0, 1).await?;
    trace!("Modbus keepalive");
    Ok(())
}

async fn stop_shutter(
    modbus_client: &mut ModbusClient,
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
    context.value = Some(value.clone());
    tx_feedback.send(IOData::new(id, value))?;
    Ok(())
}
