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

use patchpal::{models::patchpal::Patch, tui};
use std::{env, io::Error as IoError, net::SocketAddr};

use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use log::info;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{channel, Sender},
};
use tokio_tungstenite::tungstenite::protocol::Message;

use prost::Message as _;

async fn handle_connection(raw_stream: TcpStream, addr: SocketAddr, tx: Sender<Patch>) {
    info!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    info!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (_tx, rx) = unbounded();

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(async |msg| {
        match msg {
            Message::Binary(b) => {
                let patch = Patch::decode(b).unwrap();
                info!("Received a message from {}: {}", addr, patch.metadata);
                tx.send(patch).await.unwrap();
                info!("Sent state update from addr {}", addr);

                future::ok(())
            }
            _ => {
                eprintln!("Not sure how to handle... (got non-binary message)");
                future::ok(())
            }
        }
        .await
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    info!("{} disconnected", &addr);
}

async fn run_patch_server(tx: Sender<Patch>) -> Result<(), IoError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        info!("Accepted listener as {}", addr);
        tokio::spawn(handle_connection(stream, addr, tx.clone()));
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    cli_log::init_cli_log!();

    let mut terminal = ratatui::init();
    // arbitrarily decided: should think about this more
    // can maybe even just use oneshot channel
    let (tx, mut rx) = channel::<Patch>(10);

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.spawn(run_patch_server(tx));

    let app_result = tui::App::default().run(&mut terminal, &mut rx);
    ratatui::restore();
    app_result?;
    Ok(())
    //run_tui().await;
    //cli_log::init_cli_log!();
    //
    //let (_patch_srv, _tui) =
    //    tokio::join!(tokio::spawn(run_patch_server()), tokio::spawn(run_tui()));
}
