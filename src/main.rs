use std::fs::File;

use args::Command;
use clap::Parser;
use log::{debug, LevelFilter};
use simplelog::{Config, WriteLogger};
mod args;
mod client;
mod models;
mod server;
mod tui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = args::Cli::try_parse()?;
    let verbosity = cli.verbose as usize;
    let level_filter = match &verbosity {
        0 => LevelFilter::Off,
        1 => LevelFilter::Warn,
        2 => LevelFilter::Info,
        _ => LevelFilter::Debug,
    };

    match cli.command() {
        Command::Client(mode) => {
            stderrlog::new()
                .verbosity(verbosity)
                .module(module_path!())
                .init()
                .unwrap();

            debug!("Starting client");
            client::Client::from(mode).run().await?;
        }
        Command::Server => {
            WriteLogger::init(
                level_filter,
                Config::default(),
                File::create("patchpal.log").unwrap(),
            )?;

            debug!("Starting server");
            server::Server::new().run().await?;
        }
    }

    Ok(())
}
