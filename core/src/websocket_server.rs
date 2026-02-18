use futures::stream::FuturesUnordered;
use futures_util::SinkExt;
use futures_util::StreamExt;

use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;

use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;

use tokio::net::TcpListener;

use tokio_rustls::TlsAcceptor;

use tracing::*;

use crate::calaos_json_protocol;
use crate::event;
use crate::io_context;

use io_context::HomeDataRequest;
use io_context::IORequest;

use calaos_json_protocol::EventData;
use calaos_json_protocol::Request;
use calaos_json_protocol::Response;
use calaos_json_protocol::Success;

use io_context::BroadcastIODataRx;
use io_context::MpscIODataCmdTx;

trait WebsocketStream: AsyncRead + AsyncWrite + Unpin {}

impl WebsocketStream for tokio::net::TcpStream {}
impl WebsocketStream for tokio_rustls::server::TlsStream<tokio::net::TcpStream> {}

pub async fn run<'a, F>(
    addr: SocketAddr,
    tls_acceptor_opt: Option<TlsAcceptor>,
    mut make_feedback_rx: F,
    tx_output_command: MpscIODataCmdTx,
) -> Result<(), Box<dyn Error + 'a>>
where
    F: FnMut() -> BroadcastIODataRx,
{
    let listener = TcpListener::bind(&addr).await?;
    let mut sessions = FuturesUnordered::new();

    info!("Websocket listening on: {}", addr);

    loop {
        tokio::select! {
            res = listener.accept() => {
                let (stream, peer) = res?;
                let stream_res : Box<dyn WebsocketStream> = match &tls_acceptor_opt {
                    None => Box::new(stream),
                    Some(tls_acceptor) => {
                        match tls_acceptor.accept(stream).await {
                            Ok(s) => Box::new(s),
                            Err(e) => {
                                error!("Error accepting TLS connection: {}", e);
                                continue;
                            }
                        }
                    }
                };

                sessions.push(accept_connection(
                        stream_res,
                        peer,
                        make_feedback_rx(),
                        tx_output_command.clone(),
                ));
            },
            _ = sessions.select_next_some(), if !sessions.is_empty() => {}
        }
    }
}

// Create a new connection
async fn accept_connection<T: AsyncRead + AsyncWrite + Unpin>(
    stream: T,
    peer: SocketAddr,
    rx_feedback_evt: BroadcastIODataRx,
    tx_output_command: MpscIODataCmdTx,
) {
    if let Err(e) = handle_connection(peer, stream, rx_feedback_evt, tx_output_command).await {
        error!("Error processing connection: {}", e);
    }
}

async fn handle_connection<T>(
    peer: SocketAddr,
    stream: T,
    mut rx_feedback_evt: BroadcastIODataRx,
    tx_output_command: MpscIODataCmdTx,
) -> std::result::Result<(), Box<dyn std::error::Error>> where T: AsyncRead + AsyncWrite + Unpin {
    type Message = tokio_tungstenite::tungstenite::protocol::Message;
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    info!("New WebSocket connection: {}", peer);

    let mut authenticated = false; // gate events and commands until login

    loop {
        // Either process a client message, or (if authenticated) forward an IOChanged event
        let request = tokio::select! {
            msg_opt = ws_stream.next() => {
                let msg = match msg_opt {
                    None => break,
                    Some(res) => res?,
                };

                trace!("Message received on websocket ({:?}) : {:?}", peer, msg);

                match msg {
                    Message::Text(msg_str) => Request::try_from(msg_str.as_str())?,
                    Message::Binary(..) => {
                        return Err("Binary message not supported".into());
                    }
                    Message::Ping(data) => {
                        trace!("Websocket ping received ({:?})", peer);
                        Request::Pong{data}
                    }
                    Message::Pong(..) => {
                        return Err("Unexpected pong message received".into());
                    }
                    Message::Close(..) => {
                        debug!("Websocket closed by peer ({:?})", peer);
                        break;
                    }
                    Message::Frame(_) => { break; }
                }
            },
            io_data = rx_feedback_evt.recv(), if authenticated => {
                let data = io_data?;
                let response = Response::Event { data: EventData::new(event::Event::IOChanged, data) };
                let json_string = calaos_json_protocol::to_json_string(&response)?;
                ws_stream.send(Message::Text(json_string)).await?;
                continue;
            }
        };

        let response = match request {
            Request::Pong { data } => {
                ws_stream.send(Message::Pong(data)).await?;
                continue;
            }
            Request::Login { data } => {
                debug!("Login request received");
                let ok = verify_login(data.user(), data.pass());
                authenticated = ok;
                if !ok {
                    warn!("Websocket auth failed for user '{}', closing", data.user());
                }
                if !ok {
                    ws_stream.send(Message::Text(calaos_json_protocol::to_json_string(&Response::Login { data: Success::new(false) })?)).await?;
                    break;
                }
                Response::Login { data: Success::new(true) }
            }
            Request::GetHome => {
                if !authenticated { ws_stream.send(Message::Text(calaos_json_protocol::to_json_string(&Response::Login { data: Success::new(false) })?)).await?; break; }
                debug!("Get home received");

                let (request, rx) = HomeDataRequest::new(());
                tx_output_command
                    .send(io_context::IODataCmd::Request(IORequest::GetAllData(
                        request,
                    )))
                    .await?;
                Response::GetHome { data: rx.await? }
            }
            Request::SetState { data } => {
                if !authenticated { ws_stream.send(Message::Text(calaos_json_protocol::to_json_string(&Response::Login { data: Success::new(false) })?)).await?; break; }
                debug!("Set state request received {:?}", data);

                tx_output_command.send(data.into()).await?;

                Response::SetState {
                    data: Success::new(true),
                }
            }
            Request::Event { kind, data } => {
                debug!("Sending event {:?} / {:?}", kind, data);

                Response::Event {
                    data: EventData::new(kind, data),
                }
            }
        };

        let json_string = calaos_json_protocol::to_json_string(&response)?;

        trace!("Sending response ({:?}) : {:?}", peer, json_string);

        ws_stream.send(Message::Text(json_string)).await?;
    }

    Ok(())
}

fn verify_login(user: &str, pass: &str) -> bool {
    use std::env;
    match (env::var("WS_USER").ok(), env::var("WS_PASS").ok()) {
        (Some(u), Some(p)) => user == u && pass == p,
        // If not configured, allow any login
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::verify_login;

    #[test]
    fn verify_login_env_based() {
        std::env::set_var("WS_USER", "alice");
        std::env::set_var("WS_PASS", "secret");
        assert!(verify_login("alice", "secret"));
        assert!(!verify_login("alice", "wrong"));
        std::env::remove_var("WS_USER");
        std::env::remove_var("WS_PASS");
        // When not configured, allow
        assert!(verify_login("any", "any"));
    }
}

// Add MQTT support
// Path: core/src/mqtt_client.rs
// Compare this snippet from core/src/rules_engine.rs:
//    loop {
//    let io_command_opt = rx_input.recv().await;
//    if io_command_opt.is_none() {
//    break;
//    }
//      
//      let io_command = io_command_opt.unwrap();
//      debug!("Received IO command ({:?})", io_command);
//      
