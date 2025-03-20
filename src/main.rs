use args::{ClientMode, Command};
use clap::Parser;
mod args;
mod client;
mod server;

fn main() -> anyhow::Result<()> {
    let cli = args::Cli::try_parse()?;

    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    match cli.verbose {
        0 => println!("Debug mode is off"),
        1 => println!("Debug mode is kind of on"),
        2 => println!("Debug mode is on"),
        _ => println!("Don't be crazy"),
    }

    // TODO: setup verbosity to stderrlog
    // stderrlog::new()
    //    .verbosity(3)
    //    .module(module_path!())
    //    .init()
    //    .unwrap();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command() {
        Command::Client(mode) => match mode {
            ClientMode {
                local: Some(args),
                github: None,
                metadata,
            } => {
                println!("local mode! {:?} w metadata: {:?}", args, metadata)
            }
            ClientMode {
                local: None,
                github: Some(args),
                metadata,
            } => {
                println!("github mode! {:?} w metadata: {:?}", args, metadata)
            }
            _ => unreachable!(),
        },
        Command::Server => {
            println!("server mode!")
        }
    }

    // Continued program logic goes here...
    Ok(())
}
