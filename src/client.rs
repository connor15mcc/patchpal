use std::{path::PathBuf, process::Command};

use anyhow::{bail, Context};
use futures_util::{SinkExt, StreamExt};
use git2::Repository;
use log::{debug, info, warn};
use octocrab::Octocrab;
use prost::Message as _;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    args::{ClientMode, GithubClientArgs, LocalClientArgs},
    models::{patch_response::Status, Patch, PatchResponse},
};

const SERVER_URL: &str = "ws://127.0.0.1:8443";

#[derive(Clone, Debug)]
pub struct Client {
    mode: Mode,
    metadata: Option<String>,
}

impl From<ClientMode> for Client {
    fn from(mode: ClientMode) -> Self {
        let mode_enum = match (mode.local, mode.github) {
            (Some(local_args), None) => Mode::Local(local_args.into()),
            (None, Some(github_args)) => Mode::Github(github_args.into()),
            _ => Mode::Local(Local::default()),
        };

        Client {
            mode: mode_enum,
            metadata: mode.metadata,
        }
    }
}

#[derive(Clone, Debug)]
enum Mode {
    Local(Local),
    Github(Github),
}

#[derive(Clone, Debug, Default)]
struct Local {
    path: PathBuf,
}

impl From<LocalClientArgs> for Local {
    fn from(args: LocalClientArgs) -> Self {
        Local {
            path: args.path.unwrap_or(".".into()),
        }
    }
}

#[derive(Clone, Debug)]
struct Github {
    owner: String,
    repo: String,
    pr: u64,
}

impl From<GithubClientArgs> for Github {
    fn from(args: GithubClientArgs) -> Self {
        let (owner, repo) = args
            .repo
            .split_once("/")
            .expect("was validated at CLI parsetime");
        Github {
            owner: owner.to_string(),
            repo: repo.to_string(),
            pr: args.pr_number,
        }
    }
}

impl Client {
    pub async fn run(&self) -> anyhow::Result<()> {
        let unified_patch = match &self.mode {
            Mode::Local(Local { path }) => {
                let absolute_path = std::env::current_dir()?.join(path);
                let repo = Repository::open(absolute_path)?;
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
                diff_str
            }
            Mode::Github(Github { owner, repo, pr }) => {
                let gh_auth = Command::new("gh")
                    .arg("auth")
                    .arg("token")
                    .output()
                    .context("Couldn't use `gh` to auth. Make sure it's installed and auth'ed.")?;
                let auth_token = String::from_utf8(gh_auth.stdout)?;
                let auth_token = auth_token.trim();
                debug!("fetched auth token");

                let octocrab = Octocrab::builder().user_access_token(auth_token).build()?;
                debug!("authed w/ github");
                let diff = octocrab.pulls(owner, repo).get_patch(*pr).await?;
                debug!("fetched pull request diff");
                diff
            }
        };

        let (ws_stream, _) = connect_async(SERVER_URL)
            .await
            .context("(is the server listening?)")
            .expect("Failed to connect");
        info!("WebSocket handshake has been successfully completed");
        let (mut ws_tx, mut ws_rx) = ws_stream.split();
        let patch = Patch {
            metadata: self.metadata.clone(),
            patch: unified_patch,
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
}
