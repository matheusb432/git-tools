//! `gtl diff live`: validate, persist, and optionally open live views through `gtl-server`.

use anyhow::Context as _;
use gtl_wire::v1;

use crate::server_client::ServerClient;

/// Saves one repository or every eligible managed repository as live views.
pub fn run(path: Option<String>) -> anyhow::Result<()> {
    let client = ServerClient::connect()?;
    match path {
        Some(path) => run_single(&client, &path),
        None => run_managed(&client),
    }
}

fn run_single(client: &ServerClient, path: &str) -> anyhow::Result<()> {
    let response = client.save_and_present_live_view(v1::SaveAndPresentLiveViewRequest {
        path: std::path::absolute(path)?.to_string_lossy().into_owned(),
        open_viewer: open_viewer_requested(),
    })?;
    let result = response
        .result
        .context("gtl-server returned no live-view save result")?;
    saved_from_response(result)?;
    finish_optional_presentation(response.presentation)
}

fn run_managed(client: &ServerClient) -> anyhow::Result<()> {
    let response =
        client.save_and_present_project_live_views(v1::SaveAndPresentProjectLiveViewsRequest {
            open_viewer: open_viewer_requested(),
        })?;
    print_notes(&response.notes)?;
    if response.results.is_empty() {
        println!("diff live: no managed repos with commits to compare");
        return Ok(());
    }

    let mut saved = 0_usize;
    for result in response.results {
        match saved_from_response(result) {
            Ok(()) => saved += 1,
            Err(error) => eprintln!("diff live: {}", crate::error_text(&error)),
        }
    }
    if saved == 0 {
        anyhow::bail!("diff live: no live view could be saved");
    }
    finish_optional_presentation(response.presentation)
}

fn saved_from_response(response: v1::SaveLiveViewResponse) -> anyhow::Result<()> {
    match response
        .outcome
        .context("gtl-server returned no live-view outcome")?
    {
        v1::save_live_view_response::Outcome::Saved(saved) => {
            print_notes(&response.notes)?;
            match v1::SaveLiveViewDisposition::try_from(saved.disposition) {
                Ok(
                    v1::SaveLiveViewDisposition::Created | v1::SaveLiveViewDisposition::Refreshed,
                ) => Ok(()),
                Ok(v1::SaveLiveViewDisposition::Unspecified) | Err(_) => {
                    anyhow::bail!("gtl-server returned an invalid live-view disposition")
                }
            }
        }
        v1::save_live_view_response::Outcome::Rejected(rejection) => {
            anyhow::bail!(rejection.detail)
        }
    }
}

fn finish_optional_presentation(presentation: Option<v1::DiffPresentation>) -> anyhow::Result<()> {
    let Some(presentation) = presentation else {
        return Ok(());
    };
    crate::diff_viewer_client::finish_presentation(v1::PresentDiffResponse {
        presentation: Some(presentation),
    })
    .map(|_| ())
}

fn open_viewer_requested() -> bool {
    !crate::viewer::no_open_requested()
}

fn print_notes(notes: &[v1::Note]) -> anyhow::Result<()> {
    for note in notes {
        match v1::NoteLevel::try_from(note.level) {
            Ok(v1::NoteLevel::Info) => println!("{}", note.text),
            Ok(v1::NoteLevel::Warning) => eprintln!("{}", note.text),
            Ok(v1::NoteLevel::Error) => anyhow::bail!(note.text.clone()),
            Ok(v1::NoteLevel::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid live-view note level")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_save_preserves_the_server_message() {
        let result = saved_from_response(v1::SaveLiveViewResponse {
            notes: Vec::new(),
            outcome: Some(v1::save_live_view_response::Outcome::Rejected(
                v1::SaveLiveViewRejection {
                    code: v1::SaveLiveViewRejectionCode::DirectoryNotFound as i32,
                    detail: "repository is unavailable".into(),
                },
            )),
        });

        assert_eq!(result.unwrap_err().to_string(), "repository is unavailable");
    }
}
