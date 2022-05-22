use futures::stream::FuturesUnordered;
use futures_util::SinkExt;
use futures_util::StreamExt;

use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;

use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;

use tokio::net::TcpListener;
use tokio::net::TcpStream;

use tokio_native_tls::TlsAcceptor;
use tokio_native_tls::TlsStream;

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

pub async fn run<'a, F>(
    addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
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
                sessions.push(accept_connection(
                        stream,
                        peer,
                        tls_acceptor.clone(),
                        make_feedback_rx(),
                        tx_output_command.clone(),
                ));
            },
            _ = sessions.select_next_some(), if !sessions.is_empty() => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_connection<'a>(
    stream: TcpStream,
    peer: SocketAddr,
    tls_acceptor: TlsAcceptor,
    rx_feedback_evt: BroadcastIODataRx,
    tx_output_command: MpscIODataCmdTx,
) {
    let tls_stream_res = tls_acceptor.accept(stream).await;

    if let Err(e) = tls_stream_res {
        error!("Error accepting connection: {}", e);
        return;
    }

    if let Err(e) = handle_connection(
        peer,
        tls_stream_res.unwrap(),
        rx_feedback_evt,
        tx_output_command,
    )
    .await
    {
        error!("Error processing connection: {}", e);
    }
}

async fn handle_connection<'a, T: AsyncRead + AsyncWrite + Unpin>(
    peer: SocketAddr,
    stream: TlsStream<T>,
    mut rx_feedback_evt: BroadcastIODataRx,
    tx_output_command: MpscIODataCmdTx,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    type Message = tokio_tungstenite::tungstenite::protocol::Message;
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    info!("New WebSocket connection: {}", peer);

    loop {
        let request = tokio::select! {

        msg_opt = ws_stream.next() => {
            if msg_opt.is_none() {
                break;
            }

            let msg = msg_opt.unwrap()?;

            trace!("Message received on websocket ({:?}) : {:?}", peer, msg);

            let request = match msg {
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
            };

            request
        },
        io_data = rx_feedback_evt.recv() => {
            Request::Event{
                kind: event::Event::IOChanged,
                data: io_data?,
            }
        }

        };

        let response = match request {
            Request::Pong { data } => {
                ws_stream.send(Message::Pong(data)).await?;
                continue;
            }
            Request::Login { .. } => {
                debug!("Login request received");
                // TODO check login
                Response::Login {
                    data: Success::new(true),
                }
            }
            Request::GetHome => {
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
