use anyhow::Context as _;
use gtl_wire::v1;

use super::diff::DiffOutcome;
use crate::{github_cli::GhApi, server_client::ServerClient};

/// Returns the server's cached copy, or sends the GitHub request the server names and stores the
/// answer.
pub(crate) fn resolve(
    client: &ServerClient,
    request: &v1::ResolveRemoteDiffRequest,
) -> anyhow::Result<v1::StoredRemoteDiff> {
    use v1::resolve_remote_diff_response::Resolution;
    match client
        .resolve_remote_diff(request.clone())?
        .resolution
        .context("gtl-server returned no remote diff resolution")?
    {
        Resolution::Stored(stored) => Ok(stored),
        Resolution::Fetch(github_request) => {
            let response = GhApi::default().get(&github_request)?;
            client
                .store_remote_diff(
                    v1::GitHubApiResponseHead {
                        origin: request.origin.clone(),
                        range: request.range.clone(),
                        status: u32::from(response.status),
                        rate_limit_remaining: response.rate_limit_remaining,
                    },
                    &response.body,
                )?
                .stored
                .context("gtl-server returned no stored remote diff")
        }
    }
}

pub fn run(
    request: &v1::ResolveRemoteDiffRequest,
    name: Option<&str>,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let name = name.map(str::to_owned);
    super::present(
        raw,
        |client| {
            let stored = resolve(client, request)?;
            client.present_text_diff(v1::PresentTextDiffRequest {
                text_id: stored.text_id,
                label: stored.label,
                name: name.clone(),
            })
        },
        |client| {
            let stored = resolve(client, request)?;
            client.render_text_diff(v1::RenderTextDiffRequest {
                text_id: stored.text_id,
                label: stored.label,
                name: name.clone(),
            })
        },
    )
}
