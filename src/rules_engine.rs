use std::error::Error;

use tracing::*;

use crate::io_context;
use crate::rules_config;

use io_context::BroadcastIODataRx;
use io_context::BroadcastIODataTx;
use io_context::IOData;
use io_context::IOValue;
use io_context::InputContextMap;
use io_context::OutputContextMap;

use rules_config::Action;

pub async fn run<'a>(
    rx_input: BroadcastIODataRx,
    rx_output: BroadcastIODataRx,
    tx_output_command: BroadcastIODataTx,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
    should_run: &bool,
) -> Result<(), Box<dyn Error + 'a>> {
    let res = tokio::try_join!(
        handle_input(
            rx_input,
            tx_output_command,
            input_map,
            output_map,
            should_run
        ),
        handle_output_feedback(rx_output, output_map, should_run)
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
    should_run: &bool,
) -> Result<(), Box<dyn Error + 'a>> {
    while *should_run {
        let io_data = rx_output.recv().await?;
        if let Some(context) = output_map.get(io_data.id.as_str()) {
            trace!("Output feedback set {:?}", io_data);
            let mut value_write = context.value.write().unwrap();
            *value_write = Some(io_data.value);
        } else {
            error!("Output feedback unknown {:?}", io_data);
        }
    }

    Ok(())
}
async fn handle_input<'a>(
    mut rx_input: BroadcastIODataRx,
    tx_output_command: BroadcastIODataTx,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
    should_run: &bool,
) -> Result<(), Box<dyn Error + 'a>> {
    while *should_run {
        let input_io_data = rx_input.recv().await?;
        if let Some(context) = input_map.get(input_io_data.id.as_str()) {
            // TODO handle conditions
            for rule in &context.rules {
                debug!("TODO exec condition rule {:?} ", rule.name);

                for action in &rule.actions {
                    exec_action(action, &tx_output_command, output_map)?;
                }
            }
        } else {
            debug!("No rule for {:?}", input_io_data);
        }
    }

    Ok(())
}

fn exec_action<'a>(
    action: &Action,
    tx_output_command: &BroadcastIODataTx,
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

    // only consider toggle here TODO

    let new_value = match ref_value.unwrap() {
        IOValue::Bool(v) => IOValue::Bool(!v),
        _ => {
            // TODO
            error!("Value kind not handle");
            return Ok(());
        }
    };

    tx_output_command.send(IOData {
        id: action.output.id.clone(),
        value: new_value,
    })?;

    Ok(())
}

fn get_ref_value<'a>(id: &str, output_map: &OutputContextMap<'a>) -> Option<IOValue> {
    if let Some(output) = &output_map.get(id) {
        return output.value.read().unwrap().clone();
    }

    None
}
