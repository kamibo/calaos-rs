use futures_util::{SinkExt, StreamExt};

use std::convert::TryFrom;
use std::error::Error;
use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncWrite};

use tokio::net::{TcpListener, TcpStream};

use tokio_native_tls::{TlsAcceptor, TlsStream};

use tracing::*;

use crate::calaos_json_protocol;

use calaos_json_protocol::Request;
use calaos_json_protocol::Response;
use calaos_json_protocol::Success;

pub async fn run(addr: SocketAddr, tls_acceptor: TlsAcceptor) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(&addr).await?;
    info!("Websocket listening on: {}", addr);

    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(accept_connection(stream, peer, tls_acceptor.clone()));
    }
}

async fn accept_connection(stream: TcpStream, peer: SocketAddr, tls_acceptor: TlsAcceptor) {
    let tls_stream_res = tls_acceptor.accept(stream).await;

    if let Err(e) = tls_stream_res {
        error!("Error accepting connection: {}", e);
        return;
    }

    if let Err(e) = handle_connection(peer, tls_stream_res.unwrap()).await {
        error!("Error processing connection: {}", e);
    }
}

async fn handle_connection<T: AsyncRead + AsyncWrite + Unpin>(
    peer: SocketAddr,
    stream: TlsStream<T>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    type Message = tokio_tungstenite::tungstenite::protocol::Message;
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    info!("New WebSocket connection: {}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;

        trace!("Message received on websocket ({:?}) : {:?}", peer, msg);

        let request = match msg {
            Message::Text(msg_str) => Request::try_from(msg_str.as_str())?,
            Message::Binary(..) => {
                return Err("Binary message not supported".into());
            }
            Message::Ping(data) => {
                trace!("Websocket ping received ({:?})", peer);
                ws_stream.send(Message::Pong(data)).await?;
                continue;
            }
            Message::Pong(..) => {
                return Err("Unexpected pong message received".into());
            }
            Message::Close(..) => {
                debug!("Websocket closed by peer ({:?})", peer);
                return Ok(());
            }
        };

        let response = match request {
            Request::Login { .. } => {
                debug!("Login request received");
                Response::Login {
                    data: Success::new(true),
                }
            }
        };

        ws_stream
            .send(Message::Text(calaos_json_protocol::to_json_string(
                &response,
            )?))
            .await?;
        trace!("Response sent ({:?}) : {:?}", peer, response);
    }

    Ok(())
}
