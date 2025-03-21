use args::Command;
use clap::Parser;
use log::debug;
mod args;
mod client;
mod models;
mod server;
mod tui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = args::Cli::try_parse()?;

    stderrlog::new()
        .verbosity(cli.verbose as usize)
        .module(module_path!())
        .init()
        .unwrap();

    match cli.command() {
        Command::Client(mode) => {
            debug!("Starting client");
            client::Client::from(mode).run().await?;
        }
        Command::Server => {
            debug!("Starting server");
            server::Server::new().run().await?;
        }
    }

    Ok(())
}
