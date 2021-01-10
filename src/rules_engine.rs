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
    mut rx_input: BroadcastIODataRx,
    mut _rx_output: BroadcastIODataRx,
    tx_output_command: BroadcastIODataTx,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
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
