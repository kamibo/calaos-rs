use futures_util::{SinkExt, StreamExt};

use std::error::Error;
use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncWrite};

use tokio::net::{TcpListener, TcpStream};

use tokio_native_tls::{TlsAcceptor, TlsStream};

use tracing::*;

pub async fn run(addr: SocketAddr, tls_acceptor: TlsAcceptor) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(&addr).await?;
    info!("Websocket listening on: {}", addr);

    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(accept_connection(stream, peer, tls_acceptor.clone()));
    }
}

async fn accept_connection(stream: TcpStream, peer: SocketAddr, tls_acceptor: TlsAcceptor) {
    type Error = tokio_tungstenite::tungstenite::Error;

    let tls_stream_res = tls_acceptor.accept(stream).await;

    if let Err(e) = tls_stream_res {
        error!("Error accepting connection: {}", e);
        return;
    }

    if let Err(e) = handle_connection(peer, tls_stream_res.unwrap()).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
            err => error!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection<T: AsyncRead + AsyncWrite + Unpin>(
    peer: SocketAddr,
    stream: TlsStream<T>,
) -> tokio_tungstenite::tungstenite::Result<()> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    info!("New WebSocket connection: {}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        if msg.is_text() || msg.is_binary() {
            trace!("Message received on webscoket ({:?}) : {:?}", peer, msg);
            ws_stream.send(msg).await?;
        }
    }

    Ok(())
}
