#[macro_use]
extern crate clap;
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

use clap::App;
use clap::Arg;

use tokio;
use tokio::signal;
use tokio::sync::mpsc;

use tracing::*;
use tracing_subscriber::FmtSubscriber;

const PROJECT_NAME: &'static str = env!("CARGO_PKG_NAME");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let arg_io_configfile = "io_config";
    let arg_rules_configfile = "rules_config";

    let arg_matches = App::new(PROJECT_NAME)
        .about("Experimental calaos server")
        .author(crate_authors!())
        .version(crate_version!())
        .arg(
            Arg::new(arg_io_configfile)
                .about("IO config file")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new(arg_rules_configfile)
                .about("Rules config file")
                .required(true)
                .index(2),
        )
        .get_matches();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let file_io = arg_matches.value_of(arg_io_configfile).unwrap();
    let file_rules = arg_matches.value_of(arg_rules_configfile).unwrap();

    info!("Start {} server", PROJECT_NAME);

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

    info!("End {} server", PROJECT_NAME);

    Ok(())
}
