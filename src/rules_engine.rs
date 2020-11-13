use std::error::Error;

use tokio::sync::mpsc;

use tracing::*;

use crate::io_context::InputContextMap;

pub async fn run<'a>(
    mut rx: mpsc::Receiver<&str>,
    tx: mpsc::Sender<u32>,
    input_map: &InputContextMap<'a>,
) -> Result<(), Box<dyn Error>> {
    while let Some(input_id) = rx.recv().await {
        if let Some(context) = input_map.get(input_id) {
            // TODO handle conditions
            for rule in context.rules.iter() {
                debug!("TODO exec rule {:?} ", rule.name);
                // TEST
                tx.send(23).await?;

                //for action in rule.actions.iter() {
                //}
            }
        } else {
            debug!("No rule for {:?}", input_id);
        }
    }

    Ok(())
}
