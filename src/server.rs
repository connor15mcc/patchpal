use std::{env, io::Error as IoError, net::SocketAddr};

use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use prost::Message as _;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::mpsc::{channel, Receiver, Sender},
};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

use crate::{
    models::Patch,
    tui::{self, PatchRequest},
};

pub struct Server;

impl Server {
    pub fn new() -> Self {
        Server
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        // arbitrarily decided: should think about this more
        // can maybe even just use oneshot channel
        let (tx, rx) = channel::<PatchRequest>(10);

        let tui = tokio::spawn(run_tui(token.clone(), rx));
        let patch = tokio::spawn(run_patch_server(token.clone(), tx));
        // TODO: this should be a join since we want both to get a chance to shutdown gracefully
        tokio::select! {
            // ctrl_c is handled in TUI event loop bc of raw mode
            //_ = token.cancelled() => {
            //    info!("Token cancelled");
            //},
            _ = tui => {},
            _ = patch => {},
        }
        Ok(())
    }
}

async fn run_tui(token: CancellationToken, rx: Receiver<PatchRequest>) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = tui::App::new(rx);
    app.run(&token, &mut terminal).await?;
    token.cancel();
    Ok(())
}

async fn run_patch_server(
    token: CancellationToken,
    tx: Sender<PatchRequest>,
) -> Result<(), IoError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8443".to_string());

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    loop {
        tokio::select! {
            cxn = listener.accept() => {
                if let Ok((stream, addr)) = cxn {
                    info!("Accepted listener as {}", addr);
                    tokio::spawn(handle_connection(token.clone(), stream, addr, tx.clone()));
                }
            }
            _ = token.cancelled() => {
                info!("Shutting down from signal");
                return Ok(())
            }
        }
    }
}

async fn handle_connection(
    token: CancellationToken,
    raw_stream: TcpStream,
    addr: SocketAddr,
    tx: Sender<PatchRequest>,
) {
    info!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    info!("WebSocket connection established: {}", addr);

    let (mut outgoing, mut incoming) = ws_stream.split();

    loop {
        tokio::select! {
            msg = incoming.next() => {
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        let patch = Patch::decode(b).unwrap();
                        info!("Received a message from {}: {:?}", addr, patch.metadata);
                        let (response_tx, mut response_rx) = channel(1);
                        let request = PatchRequest::try_from((patch, response_tx)).expect("patches should all be valid");
                        tx.send(request).await.unwrap();
                        info!("Sent state update from addr {}", addr);

                        select! {
                            response = response_rx.recv() => {
                                info!("Received state update");
                                match response {
                                    None => info!("Empty update, channel closed"),
                                    Some(response) => {
                                        outgoing.send(response.encode_to_vec().into()).await.expect("failed to send");
                                        info!("Sent response: {:?}", response);
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        info!("{} disconnected", &addr);
                        return
                    }
                    _ => {
                        warn!("Not sure how to handle... (got non-binary message)");
                    }
                }
            }
            _ = token.cancelled() => {
                info!("Closing stream");
                let _ = outgoing.close().await;
                return
            }
        }
    }
}
