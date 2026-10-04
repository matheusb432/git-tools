use std::io::Read as _;

use anyhow::{Context as _, bail};
use gtl_models::diffs::{DIFF_TEXT_BYTES_MAX, DiffText};
use gtl_wire::v1;

use super::diff::DiffOutcome;
use crate::cli::PatchSource;

const STDIN_LABEL: &str = "stdin";

pub(crate) struct PatchInput {
    pub(crate) text: DiffText,
    pub(crate) label: String,
}

pub(crate) fn read(source: &PatchSource) -> anyhow::Result<PatchInput> {
    let (bytes, label) = match source {
        PatchSource::Stdin => (
            read_bounded(std::io::stdin().lock())?,
            STDIN_LABEL.to_owned(),
        ),
        PatchSource::File(path) => {
            let file = std::fs::File::open(path)
                .with_context(|| format!("opening patch {}", path.display()))?;
            let label = path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            (
                read_bounded(file).with_context(|| format!("reading patch {}", path.display()))?,
                label,
            )
        }
    };
    let text = String::from_utf8(bytes).context("patch text is not valid UTF-8")?;
    Ok(PatchInput {
        text: DiffText::try_new(text)?,
        label,
    })
}

fn read_bounded(reader: impl std::io::Read) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(DIFF_TEXT_BYTES_MAX as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > DIFF_TEXT_BYTES_MAX {
        bail!(
            "patch text is larger than {} MiB",
            DIFF_TEXT_BYTES_MAX / (1024 * 1024)
        );
    }
    Ok(bytes)
}

pub fn run(source: &PatchSource, name: Option<&str>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let input = read(source)?;
    let name = name.map(str::to_owned);
    super::present(
        raw,
        |client| {
            let stored = client.store_diff_text(&input.text)?;
            client.present_text_diff(v1::PresentTextDiffRequest {
                text_id: stored.text_id,
                label: input.label.clone(),
                name: name.clone(),
            })
        },
        |client| {
            let stored = client.store_diff_text(&input.text)?;
            client.render_text_diff(v1::RenderTextDiffRequest {
                text_id: stored.text_id,
                label: input.label.clone(),
                name: name.clone(),
            })
        },
    )
}
