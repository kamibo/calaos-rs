use std::error::Error;

use tracing::*;

use crate::io_context;
use crate::io_value;
use crate::rules_config;

use io_context::BroadcastIODataActionTx;
use io_context::BroadcastIODataRx;
use io_context::IODataAction;
use io_context::InputContextMap;
use io_context::OutputContextMap;

use io_value::IOValue;

use rules_config::Action;
use rules_config::ConditionKind;
use rules_config::Operator;

pub async fn run<'a>(
    rx_input: BroadcastIODataRx,
    rx_output: BroadcastIODataRx,
    tx_output_command: BroadcastIODataActionTx,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    let res = tokio::try_join!(
        handle_input(rx_input, tx_output_command, input_map, output_map,),
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
    output_map: &OutputContextMap<'a>,
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
    mut rx_input: BroadcastIODataRx,
    tx_output_command: BroadcastIODataActionTx,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    loop {
        let input_io_data = rx_input.recv().await?;
        debug!("Received input");
        if let Some(context) = input_map.get(input_io_data.id.as_str()) {
            io_context::write_io_value(&context.value, input_io_data.value);

            for rule in &context.rules {
                if should_exec(&rule.conditions, input_map) {
                    for action in &rule.actions {
                        exec_action(action, &tx_output_command, output_map)?;
                    }
                }
            }
        } else {
            debug!("No rule for {:?}", input_io_data);
        }
    }
}

fn should_exec<'a>(conditions: &[ConditionKind], input_map: &InputContextMap<'a>) -> bool {
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

fn exec_action<'a>(
    action: &Action,
    tx_output_command: &BroadcastIODataActionTx,
    output_map: &OutputContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    let ref_value = get_ref_value(action.output.id.as_str(), output_map);

    if ref_value.is_none() {
        warn!(
            "Action ignored as there is no reference value ({:?})",
            action
        );
        return Ok(());
    }

    tx_output_command.send(IODataAction::new(
        action.output.id.clone(),
        action.output.val.clone(),
    ))?;

    Ok(())
}

fn get_ref_value<'a>(id: &str, output_map: &OutputContextMap<'a>) -> Option<IOValue> {
    if let Some(output) = &output_map.get(id) {
        return output.value.read().unwrap().clone();
    }

    None
}
