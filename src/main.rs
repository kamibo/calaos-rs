#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate nom;

mod calaos_protocol;
mod io_config;
mod io_context;
mod main_server;
mod rules_config;
mod rules_engine;
mod wago_controller;
mod wago_modbus_controller;

use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use tokio;
use tokio::signal;
use tokio::sync::mpsc;

use tracing::*;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let file_io = match env::args().nth(1) {
        Some(file) => file,
        None => return Err("Missing file io".into()),
    };

    let file_rules = match env::args().nth(2) {
        Some(file) => file,
        None => return Err("Missing file rules".into()),
    };

    info!("Start calaos-rs server");

    let io_config = io_config::read_from_file(Path::new(&file_io))?;
    let rules_config = rules_config::read_from_file(Path::new(&file_rules))?;

    let input_map = io_context::make_input_context_map(&io_config, &rules_config);
    let output_map = io_context::make_output_context_map(&io_config);

    let local_addr: SocketAddr = "0.0.0.0:4646".parse()?;
    let mut should_run = true;

    let modbus_remote_addr: SocketAddr = "192.168.1.8:502".parse()?;

    let (input_evt_tx, input_evt_rx) = mpsc::channel::<&str>(100);
    let (output_evt_tx, output_evt_rx) = mpsc::channel::<&str>(100);

    tokio::select! {
      _ = main_server::run(local_addr, &io_config, input_evt_tx, &should_run) => {
      },
      res = io_context::run_input_controllers(&io_config, &should_run) => {
          if let Err(error) = res {
              error!("Error controller {:?}", error);
          }
      },
      _ = wago_modbus_controller::run(modbus_remote_addr, output_evt_rx, &output_map, &should_run) => {
      },
      _ = rules_engine::run(input_evt_rx, output_evt_tx, &input_map, &output_map) => {
      },
      _ = signal::ctrl_c() => {
          info!("Shutdown signal received");
          // Can be written before read
          #[allow(unused_assignments)]
          {
            should_run = false;
          }
      }
    }

    info!("End calaos-rs server");

    Ok(())
}
