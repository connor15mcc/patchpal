//! A simple example of hooking up stdin/stdout to a WebSocket stream.
//!
//! This example will connect to a server specified in the argument list and
//! then forward all data read on stdin to the server, printing out all data
//! received on stdout.
//!
//! Note that this is not currently optimized for performance, especially around
//! buffer management. Rather it's intended to show an example of working with a
//! client.
//!
//! You can use this example together with the `server` example.

use std::env;

use futures_util::{future, pin_mut, StreamExt};
use indoc::indoc;
use patchpal::models::patchpal::Patch;
use prost::Message as _;
use tokio::io::AsyncReadExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[tokio::main]
async fn main() {
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| panic!("this program requires at least one argument"));

    let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
    tokio::spawn(read_stdin(stdin_tx));

    let (ws_stream, _) = connect_async(&url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");

    let (write, read) = ws_stream.split();

    let stdin_to_ws = stdin_rx.map(Ok).forward(write);
    let ws_to_stdout = { read.for_each(|_| async {}) };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
async fn read_stdin(tx: futures_channel::mpsc::UnboundedSender<Message>) {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf).await {
            Err(_) | Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);

        let patch = Patch {
            patch: indoc! {"
                diff --git a/hello-world.txt b/hello-world.txt
                new file mode 100644
                index 0000000..9721e49
                --- /dev/null
                +++ b/hello-world.txt
                @@ -0,0 +1,4 @@
                +Hello to all!
                +
                +And to all a goodnight
                +
                ",
            }
            .to_string(),
            metadata: String::from_utf8(buf.clone()).expect("should all be utf8"),
        };
        buf.clear();
        buf.reserve(patch.encoded_len());
        patch.encode(&mut buf).unwrap();

        tx.unbounded_send(Message::binary(buf)).unwrap();
    }
}
