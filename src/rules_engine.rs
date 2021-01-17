use std::error::Error;

use tracing::*;

use crate::io_context;

use io_context::BroadcastIODataRx;
use io_context::BroadcastIODataTx;
use io_context::IOData;
use io_context::IOValue;
use io_context::InputContextMap;
use io_context::OutputContextMap;

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
    _output_map: &OutputContextMap<'a>,
    should_run: &bool,
) -> Result<(), Box<dyn Error + 'a>> {
    while *should_run {
        let input_io_data = rx_input.recv().await?;
        if let Some(context) = input_map.get(input_io_data.id.as_str()) {
            // TODO handle conditions
            for rule in context.rules.iter() {
                debug!("TODO exec condition rule {:?} ", rule.name);

                for action in rule.actions.iter() {
                    tx_output_command.send(IOData {
                        id: action.output.id.clone(),
                        value: IOValue::Bool(true),
                    })?;
                }
            }
        } else {
            debug!("No rule for {:?}", input_io_data);
        }
    }

    Ok(())
}
