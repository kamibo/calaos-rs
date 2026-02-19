use futures_util::SinkExt;
use futures_util::StreamExt;

use std::error::Error;
use std::net::SocketAddr;

use tokio::net::TcpStream;
use tokio::sync::mpsc;

use tokio_tungstenite::tungstenite::http::uri::Uri;
use tokio_tungstenite::tungstenite::protocol::Message;

use tracing::*;

use crate::calaos_json_protocol;

use calaos_json_protocol::Request;
use calaos_json_protocol::Response;

pub async fn run<'a>(
    addr: SocketAddr,
    uri: Uri,
    mut rx_command: mpsc::Receiver<Request>,
) -> Result<(), Box<dyn Error + 'a>> {
    let socket = TcpStream::connect(&addr).await?;

    //let uri = "blah".parse::<Uri>().unwrap();

    let (mut ws_stream, _) = tokio_tungstenite::client_async(uri, socket).await.unwrap();

    loop {
        let _response_opt: Option<Response> = tokio::select! {

          msg_opt = ws_stream.next() => {
              if msg_opt.is_none() {
                  break;
              }

              let msg = msg_opt.unwrap()?;

              trace!("Message received on websocket: {:?}", msg);

              match msg {
                  Message::Text(msg_str) => Some(Response::try_from(msg_str.as_str())?),
                  Message::Binary(..) => {
                      return Err("Binary message not supported".into());
                  }
                  Message::Ping(data) => {
                      trace!("Websocket ping received ({:?})", data);
                      None
                  }
                  Message::Pong(data) => {
                      info!("Websocket pong received ({:?})", data);
                      None
                  }
                  Message::Close(..) => {
                      info!("Websocket closed");
                      break;
                  }
                  Message::Frame(_) => { break; }
              }
          },
          request_opt = rx_command.recv() => {
              let json_request = calaos_json_protocol::to_json_string(&request_opt.unwrap())?;
              ws_stream.send(Message::Text(json_request)).await?;
              None

          }
        };
    }

    Ok(())
}
