#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate nom;

mod calaos_protocol;
mod io;
mod io_config;
mod io_context;
mod io_value;
mod main_server;
mod rules_config;
mod rules_engine;
mod websocket_server;

use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;

use clap::App;
use clap::Arg;

use tokio::signal;

use tokio_native_tls::*;

use tracing::*;
use tracing_subscriber::FmtSubscriber;

const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");

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

    let der = include_bytes!("cert.p12");
    let cert = native_tls::Identity::from_pkcs12(der, "extra")?;
    let websocket_tls = TlsAcceptor::from(native_tls::TlsAcceptor::builder(cert).build()?);
    let websocket_addr: SocketAddr = "0.0.0.0:8080".parse()?; // 443

    let (input_evt_tx, input_evt_rx) = io_context::make_iodata_broadcast_channel();
    let (output_evt_tx, output_evt_rx) = io_context::make_iodata_broadcast_channel();
    let (output_feedback_evt_tx, output_feedback_evt_rx) =
        io_context::make_iodata_broadcast_channel();

    tokio::select! {
      _ = main_server::run(local_addr, &io_config, input_evt_tx) => {
      },
      res = io_context::run_input_controllers(&io_config) => {
          if let Err(error) = res {
              error!("Error input controller {:?}", error);
          }
      },
      res = io_context::run_output_controllers(&io_config, &output_map, output_evt_rx, output_feedback_evt_tx) => {
          if let Err(error) = res {
              error!("Error output controller {:?}", error);
          }
      },
      res = rules_engine::run(input_evt_rx, output_feedback_evt_rx, output_evt_tx, &input_map, &output_map) => {
          if let Err(error) = res {
              error!("Error rules engine {:?}", error);
          }
      },
      res = websocket_server::run(websocket_addr, websocket_tls) => {
          if let Err(error) = res {
              error!("Error websocket server {:?}", error);
          }
      },
      _ = signal::ctrl_c() => {
          info!("Shutdown signal received");
      },
    }

    info!("End {} server", PROJECT_NAME);

    Ok(())
}
