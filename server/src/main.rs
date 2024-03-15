use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use calaos_core::*;

use clap::crate_authors;
use clap::crate_version;
use clap::Parser;

use tokio::signal;

use rustls_pemfile::certs;
use rustls_pemfile::rsa_private_keys;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

use tracing::*;
use tracing_subscriber::FmtSubscriber;

const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser)]
#[command(author = crate_authors!())]
#[command(version = crate_version!())]
#[command(about = "Experimental Calaos server", long_about = None)]
struct Args {
    #[arg(help = "IO config file")]
    io_config: PathBuf,
    #[arg(help = "Rules config file")]
    rules_config: PathBuf,
    #[arg(long, help = "SSL keys and cert directory")]
    ssl_config_dir: Option<PathBuf>,
    #[arg(long, default_value_t = false, help = "Ignore input devices")]
    no_input: bool,
    #[arg(long, default_value_t = false, help = "Ignore output devices")]
    no_output: bool,
}

fn load_certs(path: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
    certs(&mut BufReader::new(File::open(path)?)).collect()
}

fn load_keys(path: &Path) -> io::Result<PrivateKeyDer<'static>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .next()
        .unwrap()
        .map(Into::into)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("Start {} server", PROJECT_NAME);

    info!("Reading io file {}", args.io_config.display());
    let io_config = match config::io::read_from_file(&args.io_config) {
        Ok(io) => io,
        Err(e) => panic!("Error reading config file {:?}", e),
    };

    info!("Reading rules file {}", args.rules_config.display());
    let rules_config = match config::rules::read_from_file(&args.rules_config) {
        Ok(rules) => rules,
        Err(e) => panic!("Error reading rules file {:?}", e),
    };

    info!("Making context");
    let input_map = io_context::make_input_context_map(&io_config, &rules_config);
    let output_map = io_context::make_output_context_map(&io_config);

    let local_addr: SocketAddr = "0.0.0.0:4646".parse()?;
    let websocket_tls: Option<tokio_rustls::TlsAcceptor> = if args.ssl_config_dir.is_some() {
        let ssl_config_path = Path::new(args.ssl_config_dir.as_ref().unwrap());
        let certs = load_certs(ssl_config_path)?;
        let key = load_keys(ssl_config_path)?;
        let config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Some(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
    } else {
        None
    };
    let websocket_addr: SocketAddr = "0.0.0.0:8080".parse()?; // 443
    let websocket_addr_no_tls: SocketAddr = "0.0.0.0:5454".parse()?;

    let mut input_evt_channel = io_context::make_iodatacmd_mpsc_channel();
    let mut output_cmd_channel = io_context::make_iodataaction_mpsc_channel();
    let output_feedback_evt_channel = io_context::make_iodata_broadcast_channel();

    let make_feedback_rx = || output_feedback_evt_channel.subscribe();

    info!("Running...");

    tokio::select! {
      _ = main_server::run(local_addr, &io_config, input_evt_channel.advertise()) => {
      },
      res = io_context::run_input_controllers(&io_config, &input_map, input_evt_channel.advertise()), if !args.no_input => {
          if let Err(error) = res {
              error!("Error input controller {:?}", error);
          }
      },
      res = io_context::run_output_controllers(&io_config, &output_map, output_cmd_channel.subscribe().unwrap(), output_feedback_evt_channel.advertise()), if !args.no_output => {
          if let Err(error) = res {
              error!("Error output controller {:?}", error);
          }
      },
      res = rules_engine::run(&io_config, input_evt_channel.subscribe().unwrap(), output_feedback_evt_channel.subscribe(), output_cmd_channel.advertise(), &input_map, &output_map) => {
          if let Err(error) = res {
              error!("Error rules engine {:?}", error);
          }
      },
      res = websocket_server::run(websocket_addr, websocket_tls, make_feedback_rx, input_evt_channel.advertise()), if websocket_tls.is_some()=> {
          if let Err(error) = res {
              error!("Error websocket server {:?}", error);
          }
      },
      res = websocket_server::run(websocket_addr_no_tls, None, make_feedback_rx, input_evt_channel.advertise()) => {
          if let Err(error) = res {
              error!("Error websocket no tls server {:?}", error);
          }
      },
      _ = signal::ctrl_c() => {
          info!("Shutdown signal received");
      },
    }

    info!("End {} server", PROJECT_NAME);

    Ok(())
}
