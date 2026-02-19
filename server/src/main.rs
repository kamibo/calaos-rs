use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use calaos_core::*;

use clap::crate_authors;
use clap::crate_version;
use clap::Parser;

use tokio::signal;

use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
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
    // MQTT options
    #[arg(long, default_value = "localhost", help = "MQTT broker host")]
    mqtt_host: String,
    #[arg(long, default_value_t = 1883, help = "MQTT broker port")]
    mqtt_port: u16,
    #[arg(long, help = "MQTT username (optional)")]
    mqtt_username: Option<String>,
    #[arg(long, help = "MQTT password (optional)")]
    mqtt_password: Option<String>,
    #[arg(long, default_value = "homeassistant", help = "MQTT discovery prefix")]
    mqtt_discovery_prefix: String,
    #[arg(long, default_value = "calaos", help = "MQTT node id (topic root)")]
    mqtt_node_id: String,
    #[arg(long, default_value_t = 30, help = "MQTT keep alive in seconds")]
    mqtt_keep_alive_sec: u64,
    #[arg(long, default_value_t = false, help = "Ignore input devices")]
    no_input: bool,
    #[arg(long, default_value_t = false, help = "Ignore output devices")]
    no_output: bool,
}

fn load_certs(path: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
    // Accept either a single PEM file or a directory containing PEMs
    let try_files: Vec<PathBuf> = if path.is_dir() {
        let mut candidates = vec![path.join("fullchain.pem"), path.join("cert.pem")];
        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry in read_dir.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("pem") {
                    candidates.push(p);
                }
            }
        }
        candidates
    } else {
        vec![path.to_path_buf()]
    };

    for p in try_files {
        if let Ok(f) = File::open(&p) {
            let mut reader = BufReader::new(f);
            let collected: io::Result<Vec<_>> = certs(&mut reader).collect();
            match collected {
                Ok(c) if !c.is_empty() => return Ok(c),
                _ => continue,
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no valid certificate PEM found",
    ))
}

fn load_keys(path: &Path) -> io::Result<PrivateKeyDer<'static>> {
    // Accept either a single PEM file or a directory containing PEMs
    let try_files: Vec<PathBuf> = if path.is_dir() {
        let mut candidates = vec![path.join("privkey.pem"), path.join("key.pem")];
        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry in read_dir.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("pem") {
                    candidates.push(p);
                }
            }
        }
        candidates
    } else {
        vec![path.to_path_buf()]
    };

    for p in try_files {
        if let Ok(f) = File::open(&p) {
            let mut reader = BufReader::new(f);
            // Try PKCS#8 first
            if let Some(key) = pkcs8_private_keys(&mut reader).next() {
                return key
                    .map(Into::into)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e));
            }

            // Re-open for RSA parse since reader is consumed
            if let Ok(f2) = File::open(&p) {
                let mut reader2 = BufReader::new(f2);
                let mut it = rsa_private_keys(&mut reader2);
                if let Some(key) = it.next() {
                    return key
                        .map(Into::into)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e));
                }
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no valid private key PEM found",
    ))
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
        Err(e) => {
            error!(
                "Error reading IO config file: {} => {:?}",
                args.io_config.display(),
                e
            );
            return Err(e);
        }
    };

    info!("Reading rules file {}", args.rules_config.display());
    let rules_config = match config::rules::read_from_file(&args.rules_config) {
        Ok(rules) => rules,
        Err(e) => {
            error!(
                "Error reading rules config file: {} => {:?}",
                args.rules_config.display(),
                e
            );
            return Err(e);
        }
    };

    info!("Making context");
    let io_config_arc = Arc::new(io_config);
    let input_map = io_context::make_input_context_map(&io_config_arc, &rules_config);
    let output_map = io_context::make_output_context_map(&io_config_arc);

    let local_addr: SocketAddr = "0.0.0.0:4646".parse()?;
    let websocket_tls: Option<tokio_rustls::TlsAcceptor> =
        if let Some(ref ssl_dir) = args.ssl_config_dir {
            let ssl_config_path = Path::new(ssl_dir);
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

    // Start MQTT client (Home Assistant discovery + command handling)
    let mqtt_config = config::mqtt::MqttConfig {
        host: args.mqtt_host.clone(),
        port: args.mqtt_port,
        username: args.mqtt_username.clone(),
        password: args.mqtt_password.clone(),
        discovery_prefix: args.mqtt_discovery_prefix.clone(),
        node_id: args.mqtt_node_id.clone(),
        keep_alive: Duration::from_secs(args.mqtt_keep_alive_sec),
    };
    let mqtt_client = mqtt_client::MqttClient::new(
        mqtt_config,
        Arc::clone(&io_config_arc),
        output_cmd_channel.advertise(),
    )
    .await?;
    let mqtt_state_rx = output_feedback_evt_channel.subscribe();
    let (mqtt_shutdown_tx, mqtt_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let mqtt_handle = tokio::spawn(async move {
        if let Err(e) = mqtt_client.start(mqtt_state_rx, mqtt_shutdown_rx).await {
            error!("MQTT client error: {:?}", e);
        }
    });

    info!("Running...");

    tokio::select! {
        _ = main_server::run(local_addr, &io_config_arc, input_evt_channel.advertise(), output_feedback_evt_channel.advertise()) => {
        },
        res = io_context::run_input_controllers(&io_config_arc, &input_map, input_evt_channel.advertise()), if !args.no_input => {
            if let Err(error) = res {
                error!("Error input controller {:?}", error);
            }
        },
        res = io_context::run_output_controllers(&io_config_arc, &output_map, output_cmd_channel.subscribe().unwrap(), output_feedback_evt_channel.advertise()), if !args.no_output => {
            if let Err(error) = res {
                error!("Error output controller {:?}", error);
            }
        },
        res = rules_engine::run(&io_config_arc, input_evt_channel.subscribe().unwrap(), output_feedback_evt_channel.subscribe(), output_cmd_channel.advertise(), &input_map, &output_map) => {
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

    // Graceful shutdown: notify MQTT to publish per-entity offline and finish
    let _ = mqtt_shutdown_tx.send(());
    let _ = mqtt_handle.await;

    // Optional grace period to let other tasks wind down
    let shutdown_grace_ms: u64 = std::env::var("SHUTDOWN_GRACE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    if shutdown_grace_ms > 0 {
        info!("Shutdown grace period: {} ms", shutdown_grace_ms);
        tokio::time::sleep(std::time::Duration::from_millis(shutdown_grace_ms)).await;
    }

    info!("End {} server", PROJECT_NAME);

    Ok(())
}
