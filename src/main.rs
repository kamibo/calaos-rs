#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate nom;

mod calaos_protocol;
mod io_config;
mod main_server;
mod rules_config;
mod wago_controller;

use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use tokio;
use tokio::signal;

use tracing::{info, Level};
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

    let io_config = io_config::read_from_file(Path::new(&file_io));
    let rules_config = rules_config::read_from_file(Path::new(&file_rules));

    let local_addr: SocketAddr = "0.0.0.0:4646".parse()?;
    let mut should_run = true;

    let wago_local_addr: SocketAddr = "0.0.0.0:0".parse()?;
    let wago_remote_addr: SocketAddr = "192.168.1.8:4646".parse()?;
    let wago_heartbeat_period = Duration::from_secs(2);

    tokio::select! {
      _ = main_server::run(local_addr, &should_run) => {
      },
      _ = wago_controller::run(wago_local_addr, wago_remote_addr, wago_heartbeat_period, &should_run) => {
      },
      _ = signal::ctrl_c() => {
          info!("Shutdown signal received");
          should_run = false;
      }
    }

    info!("End calaos-rs server");

    Ok(())
}
