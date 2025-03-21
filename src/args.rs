use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use regex::Regex;

#[derive(Parser, Debug)]
#[command(version, author, about)]
// TODO: support configuring port / url
pub struct Cli {
    /// enable additional log information
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub fn command(self) -> Command {
        self.command
            .unwrap_or(Command::Client(ClientMode::default()))
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// run the patchpal client
    Client(ClientMode),

    /// run the patchpal server
    Server,
}

#[derive(Args, Debug)]
#[group(id = "mode")]
pub struct ClientMode {
    /// operate on local patches
    #[command(flatten)]
    pub local: Option<LocalClientArgs>,

    /// operate on github PRs
    #[command(flatten)]
    pub github: Option<GithubClientArgs>,

    /// enable additional log information
    pub metadata: Option<String>,
}

impl Default for ClientMode {
    fn default() -> Self {
        ClientMode {
            local: Some(LocalClientArgs::default()),
            github: None,
            metadata: None,
        }
    }
}

#[derive(Args, Default, Debug)]
#[group(id = "local", conflicts_with_all = ["pr_number", "repo"])]
pub struct LocalClientArgs {
    /// path to the repo
    // this 'C' short flag matches git's behavior for changing git repo path
    #[arg(short = 'C', long, required = false)]
    pub path: Option<PathBuf>,
}

#[derive(Args, Debug)]
#[group(id = "github", requires_all = ["pr_number", "repo"])]
pub struct GithubClientArgs {
    /// branch identifier (PR number) that identifies a diff
    #[arg(short = 'n', long, required = false)]
    pub pr_number: u64,

    /// repo to check for a diff
    #[arg(short, long, value_parser = parse_repo, required = false)]
    pub repo: String,
}

/// Custom parser to ensure the repo string is in the format 'owner/repo'
fn parse_repo(repo: &str) -> Result<String, clap::Error> {
    let re = Regex::new(r"^(?P<owner>[^/]+)/(?P<repo>[^/]+)$").unwrap();
    if re.is_match(repo) {
        Ok(repo.to_string())
    } else {
        Err(clap::Error::raw(
            clap::error::ErrorKind::ValueValidation,
            "Repo must be in the format 'owner/repo' with exactly one '/'",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_parsing() {
        macro_rules! parses {
            ($input:expr) => {
                Cli::try_parse_from($input.split_whitespace()).unwrap();
            };
        }
        parses!("patchpal");
        parses!("patchpal -v");
        parses!("patchpal -vvv");
        parses!("patchpal server -v");
        parses!("patchpal server");
        parses!("patchpal client");
        parses!("patchpal client METADATA");
        parses!("patchpal client --path ../bar");
        parses!("patchpal client --repo foo/bar --pr-number 123");
        parses!("patchpal client --repo foo/bar --pr-number 123 METADATA");
        // ideally we could intuit the repo, but not yet:
        // parses!("patchpal client --pr-number 123");
    }

    #[test]
    fn successful_rejection() {
        macro_rules! fails {
            ($input:expr) => {
                Cli::try_parse_from($input.split_whitespace()).unwrap_err();
            };
        }
        fails!("patchpal server client");
        fails!("patchpal server --path ../bar");
        fails!("patchpal client --path ../bar --repo foo/bar");
        fails!("patchpal client --path ../bar --repo foo/bar --pr-number 123");
        // ideally we could intuit the repo, but not yet:
        fails!("patchpal client --pr-number 123");
    }
}
