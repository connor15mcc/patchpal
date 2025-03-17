use std::{
    env,
    io::{self, Read},
};

use anyhow::bail;
use futures_util::{SinkExt, StreamExt};
use git2::Repository;
use log::{debug, info, warn};
use patchpal::models::patchpal::{patch_response::Status, Patch, PatchResponse};
use prost::Message as _;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    stderrlog::new()
        .verbosity(3)
        .module(module_path!())
        .init()
        .unwrap();

    // Read metadata from STDIN
    let mut metadata = String::new();
    io::stdin().read_to_string(&mut metadata)?;
    info!("Read metadata: {} bytes", metadata.len());

    // Open the current directory as a git repository
    let repo = Repository::open(".")?;
    info!("Successfully opened git repository");

    let index = repo.index()?;
    let diff = repo.diff_index_to_workdir(Some(&index), None)?;

    if diff.stats()?.files_changed() == 0 {
        debug!("Added:   {}", diff.stats()?.insertions());
        debug!("Deleted: {}", diff.stats()?.deletions());
        debug!("Changed: {}", diff.stats()?.files_changed());
        bail!("no files changed...")
    }

    let mut diff_str = String::new();
    diff.print(git2::DiffFormat::Patch, |_d, _h, l| {
        match l.origin() {
            '+' | '-' | ' ' => diff_str.push(l.origin()),
            _ => {}
        };
        diff_str.push_str(std::str::from_utf8(l.content()).expect("all utf-8"));
        true
    })?;

    debug!("\nDiff preview (first 10 lines):");
    debug!(
        "{}",
        diff_str.lines().take(10).collect::<Vec<&str>>().join("\n")
    );
    if diff_str.lines().count() > 10 {
        debug!("... ({} more lines)", diff_str.lines().count() - 10);
    }

    // Create websocket cxn
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8443".to_string());

    let (ws_stream, _) = connect_async(&url).await.expect("Failed to connect");
    info!("WebSocket handshake has been successfully completed");
    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    let patch = Patch {
        metadata,
        patch: diff_str,
    };
    ws_tx
        .send(patch.encode_to_vec().into())
        .await
        .expect("failed to send");
    info!("Sent patch to server");

    match ws_rx.next().await {
        Some(Ok(Message::Binary(b))) => {
            info!("got response from server");
            let response = PatchResponse::decode(b).unwrap();
            match response.status.try_into()? {
                Status::Accepted => return Ok(()),
                Status::Rejected => {
                    info!("patch was rejected!");
                    std::process::exit(1)
                }
                Status::Unknown => bail!("who knows..."),
            }
        }
        Some(Ok(Message::Close(_))) => {
            info!("Server disconnected.");
        }
        Some(Err(e)) => {
            warn!("Err on socket: {}", e);
        }
        _ => {
            warn!("Not sure how to handle... (got unexpected message)");
        }
    };

    Ok(())
}
