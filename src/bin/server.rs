//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts WebSocket connections,
//! reads lines from those connections, and broadcasts the lines to all other
//! connected clients.
//!
//! You can test this out by running:
//!
//!     cargo run --example server 127.0.0.1:12345
//!
//! And then in another window run:
//!
//!     cargo run --example client ws://127.0.0.1:12345/
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

use std::{env, io::Error as IoError, net::SocketAddr};

use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use patchpal::{models::patchpal::Patch, tui};
use prost::Message as _;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{channel, Receiver, Sender},
};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

async fn handle_connection(
    token: CancellationToken,
    raw_stream: TcpStream,
    addr: SocketAddr,
    tx: Sender<Patch>,
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
                        info!("Received a message from {}: {}", addr, patch.metadata);
                        tx.send(patch).await.unwrap();
                        info!("Sent state update from addr {}", addr);
                    }
                    None => {
                        info!("{} disconnected", &addr);
                    }
                    _ => {
                        warn!("Not sure how to handle... (got non-binary message)");
                    }
                }
            }
            _ = token.cancelled() => {
                info!("Closing stream");
                let _ = outgoing.close();
                return
            }
        }
    }
}

async fn run_patch_server(token: CancellationToken, tx: Sender<Patch>) -> Result<(), IoError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

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

async fn run_tui(token: CancellationToken, mut rx: Receiver<Patch>) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = tui::App::default();
    app.run(&token, &mut terminal, &mut rx).await?;
    token.cancel();
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli_log::init_cli_log!();

    let token = CancellationToken::new();
    // arbitrarily decided: should think about this more
    // can maybe even just use oneshot channel
    let (tx, rx) = channel::<Patch>(10);

    let tui = tokio::spawn(run_tui(token.clone(), rx));
    let patch = tokio::spawn(run_patch_server(token.clone(), tx));
    tokio::select! {
        // ctrl_c is handled in TUI event loop bc of raw mode
        _ = token.cancelled() => {
            info!("Token cancelled");
        },
        _ = tui => {},
        _ = patch => {},
    }
    Ok(())
}
