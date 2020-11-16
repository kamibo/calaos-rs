use std::error::Error;

use tokio::sync::mpsc;

use tracing::*;

use crate::io_context;

use io_context::InputContextMap;
use io_context::OutputContextMap;

pub async fn run<'a>(
    mut rx: mpsc::Receiver<&str>,
    tx: mpsc::Sender<&'a str>,
    input_map: &InputContextMap<'a>,
    output_map: &OutputContextMap<'a>,
) -> Result<(), Box<dyn Error + 'a>> {
    while let Some(input_id) = rx.recv().await {
        if let Some(context) = input_map.get(input_id) {
            // TODO handle conditions
            for rule in context.rules.iter() {
                debug!("TODO exec condition rule {:?} ", rule.name);

                for action in rule.actions.iter() {
                    tx.send(action.output.id.as_str()).await?;
                }
            }
        } else {
            debug!("No rule for {:?}", input_id);
        }
    }

    Ok(())
}
