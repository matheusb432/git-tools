use dioxus::prelude::{document, spawn};
use gtl_contracts::viewer::{
    ViewerDiffChunk, ViewerDiffChunkContinuation, ViewerDiffDocument, ViewerDiffMaterialization,
};
use serde::{Deserialize, Serialize};

use crate::{
    app::DIFF_ISLAND_CSS, entities::diffs::view_identity_value, shared::bridge::ClientApiError,
};

const MOUNT_SCRIPT: &str = r#"
const prepared = await dioxus.recv();
const host = document.getElementById("viewer-diff-island");
if (!host || !window.GtlDiffIsland) throw new Error("diff island unavailable");
return window.GtlDiffIsland.mount(host, prepared);
"#;

const REPLACE_SCRIPT: &str = r#"
const prepared = await dioxus.recv();
if (!window.GtlDiffIsland) throw new Error("diff island unavailable");
return window.GtlDiffIsland.replace(prepared);
"#;

const APPEND_CHUNK_SCRIPT: &str = r#"
const [chain, chunk] = await dioxus.recv();
if (!window.GtlDiffIsland) throw new Error("diff island unavailable");
return window.GtlDiffIsland.appendChunk(chain, chunk);
"#;

const SCROLL_TO_FILE_SCRIPT: &str = r"
const targetId = await dioxus.recv();
return window.GtlDiffIsland?.scrollToFile(targetId) ?? false;
";

const SET_FILES_FOLDED_SCRIPT: &str = r"
const folded = await dioxus.recv();
window.GtlDiffIsland?.setFilesFolded(folded);
return null;
";

const DESTROY_SCRIPT: &str = r"
window.GtlDiffIsland?.destroy();
return null;
";

const OPEN_FILE_EVENT_SCRIPT: &str = r#"
await new Promise((resolve) => requestAnimationFrame(resolve));
const host = document.getElementById("viewer-diff-island");
if (!host) {
    dioxus.send({ status: "unavailable" });
    return;
}
const handler = (event) => {
    const path = event.detail?.path;
    if (typeof path === "string") {
        dioxus.send({ status: "open_file", payload: { path } });
    }
};
host.addEventListener("gtl:open-diff-file", handler);
dioxus.send({ status: "ready" });
try {
    await dioxus.recv();
} finally {
    host.removeEventListener("gtl:open-diff-file", handler);
}
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffIslandChain {
    view_identity: String,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DiffIslandAppendResult {
    Appended,
    Complete,
    Stale,
    TargetMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct DiffIslandOpenFile {
    pub(crate) path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedDocument<'document> {
    view_identity: String,
    html: &'document str,
    style_href: String,
    materialization: DiffIslandMaterialization,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DiffIslandMaterialization {
    Complete,
    Loading,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffIslandChunk<'chunk> {
    view_identity: String,
    target_id: &'chunk str,
    html: &'chunk str,
    row_count: usize,
    continuation: bool,
}

#[derive(Deserialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case")]
enum OpenFileEvent {
    Ready,
    OpenFile(DiffIslandOpenFile),
    Unavailable,
}

pub(crate) struct DiffIslandBridge;

impl DiffIslandBridge {
    pub(crate) async fn mount(
        document: &ViewerDiffDocument,
    ) -> Result<DiffIslandChain, ClientApiError> {
        Self::install(MOUNT_SCRIPT, document).await
    }

    pub(crate) async fn replace(
        document: &ViewerDiffDocument,
    ) -> Result<DiffIslandChain, ClientApiError> {
        Self::install(REPLACE_SCRIPT, document).await
    }

    async fn install(
        script: &'static str,
        document: &ViewerDiffDocument,
    ) -> Result<DiffIslandChain, ClientApiError> {
        let materialization = match document.materialization {
            ViewerDiffMaterialization::Complete => DiffIslandMaterialization::Complete,
            ViewerDiffMaterialization::Loading { .. } => DiffIslandMaterialization::Loading,
        };
        let payload = PreparedDocument {
            view_identity: view_identity_value(document.identity),
            html: &document.html,
            style_href: DIFF_ISLAND_CSS.to_string(),
            materialization,
        };
        let evaluator = document::eval(script);
        evaluator
            .send(payload)
            .map_err(|_| ClientApiError::Unavailable)?;
        evaluator
            .join::<DiffIslandChain>()
            .await
            .map_err(|_| ClientApiError::Unavailable)
    }

    pub(crate) async fn append_chunk(
        chain: &DiffIslandChain,
        chunk: &ViewerDiffChunk,
    ) -> Result<DiffIslandAppendResult, ClientApiError> {
        let payload = DiffIslandChunk {
            view_identity: view_identity_value(chunk.identity),
            target_id: &chunk.target_id,
            html: &chunk.html,
            row_count: chunk.row_count,
            continuation: matches!(chunk.continuation, ViewerDiffChunkContinuation::More),
        };
        let evaluator = document::eval(APPEND_CHUNK_SCRIPT);
        evaluator
            .send((chain, payload))
            .map_err(|_| ClientApiError::Unavailable)?;
        evaluator
            .join::<DiffIslandAppendResult>()
            .await
            .map_err(|_| ClientApiError::Unavailable)
    }

    pub(crate) fn scroll_to_file(target_id: String) {
        spawn(async move {
            let evaluator = document::eval(SCROLL_TO_FILE_SCRIPT);
            if evaluator.send(target_id).is_ok() {
                let _ = evaluator.join::<bool>().await;
            }
        });
    }

    pub(crate) fn set_files_folded(folded: bool) {
        spawn(async move {
            let evaluator = document::eval(SET_FILES_FOLDED_SCRIPT);
            if evaluator.send(folded).is_ok() {
                let _ = evaluator.join::<()>().await;
            }
        });
    }

    pub(crate) fn destroy() {
        document::eval(DESTROY_SCRIPT);
    }

    pub(crate) async fn listen_for_open_files<Ready, Handler>(
        mut on_ready: Ready,
        mut on_open_file: Handler,
    ) -> Result<(), ClientApiError>
    where
        Ready: FnMut(),
        Handler: FnMut(DiffIslandOpenFile),
    {
        let mut evaluator = document::eval(OPEN_FILE_EVENT_SCRIPT);
        loop {
            match evaluator
                .recv::<OpenFileEvent>()
                .await
                .map_err(|_| ClientApiError::Unavailable)?
            {
                OpenFileEvent::Ready => on_ready(),
                OpenFileEvent::OpenFile(event) => on_open_file(event),
                OpenFileEvent::Unavailable => return Err(ClientApiError::Unavailable),
            }
        }
    }
}
