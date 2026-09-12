use std::path::Path;

use anyhow::{Context as _, bail};
use gtl_wire::v1;

use crate::{
    cli::ManagedArgs,
    commands::managed::{PushPullResult, RepoSyncStatus},
    server_client::ServerClient,
};

pub(crate) fn run(root: &Path, args: ManagedArgs) -> anyhow::Result<()> {
    let response = ServerClient::connect()?.pull_repository(v1::PullRepositoryRequest {
        repository_path: root.to_string_lossy().into_owned(),
        dry_run: args.dry,
    })?;
    let result =
        PushPullResult::try_from(response.result.context("server returned no pull result")?)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    match result.status {
        RepoSyncStatus::Pulled | RepoSyncStatus::WouldPull | RepoSyncStatus::UpToDate => {
            if !args.json {
                println!("{}", crate::output::sentence(&result.detail));
            }
            Ok(())
        }
        RepoSyncStatus::Skip | RepoSyncStatus::Warn | RepoSyncStatus::Fail => bail!(result.detail),
        RepoSyncStatus::Pushed | RepoSyncStatus::WouldPush => {
            bail!("server returned an invalid pull status")
        }
    }
}
