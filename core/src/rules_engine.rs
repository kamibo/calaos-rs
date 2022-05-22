use std::error::Error;

use tracing::*;

use crate::calaos_json_protocol;
use crate::config;
use crate::io_context;
use crate::io_value;

use io_context::BroadcastIODataRx;
use io_context::IODataAction;
use io_context::IODataCmd;
use io_context::IORequest;
use io_context::InputSharedContextMap;
use io_context::MpscIODataActionTx;
use io_context::MpscIODataCmdRx;
use io_context::OutputSharedContextMap;

use calaos_json_protocol::HomeData;

use io_value::IOAction;
use io_value::IOValue;

use config::io::IoConfig;
use config::rules::Action;
use config::rules::ConditionKind;
use config::rules::Operator;

pub async fn run<'a>(
    io_config: &IoConfig,
    rx_input: MpscIODataCmdRx,
    rx_output: BroadcastIODataRx,
    tx_output_command: MpscIODataActionTx,
    input_map: &InputSharedContextMap<'a>,
    output_map: &OutputSharedContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    let res = tokio::try_join!(
        handle_input(
            io_config,
            rx_input,
            tx_output_command,
            input_map,
            output_map,
        ),
        handle_output_feedback(rx_output, output_map)
    );

    // Flatten result as res type is Result<((), ()), ...>
    if let Err(err) = res {
        return Err(err);
    }

    Ok(())
}

async fn handle_output_feedback<'a>(
    mut rx_output: BroadcastIODataRx,
    output_map: &OutputSharedContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    loop {
        let io_data = rx_output.recv().await?;
        if let Some(context) = output_map.get(io_data.id.as_str()) {
            trace!("Output feedback set {:?}", io_data);
            io_context::write_io_value(&context.value, io_data.value);
        } else {
            error!("Output feedback unknown {:?}", io_data);
        }
    }
}

async fn handle_input<'a>(
    io_config: &IoConfig,
    mut rx_input: MpscIODataCmdRx,
    tx_output_command: MpscIODataActionTx,
    input_map: &InputSharedContextMap<'a>,
    output_map: &OutputSharedContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    loop {
        let io_command_opt = rx_input.recv().await;

        if io_command_opt.is_none() {
            break;
        }

        let io_command = io_command_opt.unwrap();
        debug!("Received IO command ({:?})", io_command);

        match io_command {
            IODataCmd::Action(io_action) => {
                /*
                 * New command received
                 * Either it is an input then try to match some rules
                 * Or forward the value to output controllers
                 */

                if let Some(context) = input_map.get(io_action.id.as_str()) {
                    let value = match io_action.action {
                        IOAction::SetValue(v) => v,
                        _ => {
                            error!("Ignoring unexpected input command ({:?})", io_action);
                            continue;
                        }
                    };

                    io_context::write_io_value(&context.value, value);

                    for rule in &context.rules {
                        if should_exec(&rule.conditions, input_map) {
                            for action in &rule.actions {
                                exec_action(action, &tx_output_command, output_map).await?;
                            }
                        }
                    }
                } else {
                    tx_output_command.send(io_action).await?;
                }
            }
            IODataCmd::Request(request) => match request {
                IORequest::GetAllData(handler) => {
                    debug!("Home data requested");
                    handler.answer(HomeData::new(io_config, input_map, output_map));
                }
            },
        }
    }

    Ok(())
}

fn should_exec<'a>(conditions: &[ConditionKind], input_map: &InputSharedContextMap<'a>) -> bool {
    for condition in conditions {
        match condition {
            ConditionKind::Start => continue,
            ConditionKind::Standard { input } => {
                let ref_input = input_map.get(input.id.as_str());

                if ref_input.is_none() {
                    debug!("No reference input for {:?}", input);
                    return false;
                }

                if let Ok(lock) = ref_input.unwrap().value.read() {
                    let ref_value = &*lock;

                    match ref_value {
                        None => return false,
                        Some(value) => {
                            let eval = match input.operator {
                                Operator::Equal => *value == input.value,
                                Operator::NotEqual => *value != input.value,
                                Operator::Greater => *value > input.value,
                                Operator::GreaterOrEqual => *value >= input.value,
                                Operator::Lower => *value < input.value,
                                Operator::LowerOrEqual => *value <= input.value,
                            };

                            if !eval {
                                trace!("Condition rejected");
                                return false;
                            }
                        }
                    }
                } else {
                    warn!("Cannot lock the input value");
                }
            }
        }
    }

    // No condition => consider ok
    true
}

async fn exec_action<'a>(
    action: &Action,
    tx_output_command: &MpscIODataActionTx,
    output_map: &OutputSharedContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    let ref_value = get_ref_value(action.output.id.as_str(), output_map);

    if ref_value.is_none() {
        warn!(
            "Action ignored as there is no reference value ({:?})",
            action
        );
        return Ok(());
    }

    tx_output_command
        .send(IODataAction::new(
            action.output.id.clone(),
            action.output.val.clone(),
        ))
        .await?;

    Ok(())
}

fn get_ref_value<'a>(id: &str, output_map: &OutputSharedContextMap<'a>) -> Option<IOValue> {
    if let Some(output) = &output_map.get(id) {
        return output.value.read().unwrap().clone();
    }

    None
}
