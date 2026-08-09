//! Shared benchmark catalogue for Criterion targets and repository automation.

use clap::{ValueEnum, builder::PossibleValue};

pub const PACKAGE_NAME: &str = "gtl-benchmarks";

/// Returns a benchmark fixture value or terminates the benchmark process with context.
pub fn require<T, Error>(result: Result<T, Error>, context: &str) -> T
where
    Error: std::fmt::Display,
{
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("benchmark setup failed while {context}: {error}");
            std::process::exit(1);
        }
    }
}

const VIEWER_RENDER_FAST_CASES: &[BenchmarkCase] = &[
    BenchmarkCase::ViewerRenderDiffDocumentShell45k,
    BenchmarkCase::ViewerRenderDiffDocumentShell115Files,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Benchmark {
    AppStateRecordRender,
    ViewCache,
    ViewerRender,
}

impl Benchmark {
    const ALL: [Self; 3] = [
        Self::AppStateRecordRender,
        Self::ViewCache,
        Self::ViewerRender,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppStateRecordRender => "app-state-record-render",
            Self::ViewCache => "view-cache",
            Self::ViewerRender => "viewer-render",
        }
    }

    pub const fn cargo_target(self) -> &'static str {
        match self {
            Self::AppStateRecordRender => "app_state_record_render",
            Self::ViewCache => "view_cache",
            Self::ViewerRender => "viewer_render",
        }
    }

    pub const fn sample_size(self) -> usize {
        match self {
            Self::ViewerRender => 10,
            Self::AppStateRecordRender | Self::ViewCache => 100,
        }
    }

    pub const fn fast_cases(self) -> &'static [BenchmarkCase] {
        match self {
            Self::ViewerRender => VIEWER_RENDER_FAST_CASES,
            Self::AppStateRecordRender | Self::ViewCache => &[],
        }
    }
}

impl std::fmt::Display for Benchmark {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl ValueEnum for Benchmark {
    fn value_variants<'variants>() -> &'variants [Self] {
        &Self::ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.as_str()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenchmarkCase {
    AppStateRecordRender,
    ViewCacheViewWeight45k,
    ViewCacheViewReplace45k,
    ViewerRenderRawArtifact,
    ViewerRenderRawArtifactSplitFull,
    ViewerRenderDiffDocumentShell45k,
    ViewerRenderMaterializedChunks45k,
    ViewerRenderDiffDocumentShell115Files,
    ViewerRenderMaterializedChunks115Files,
}

impl BenchmarkCase {
    const ALL: [Self; 9] = [
        Self::AppStateRecordRender,
        Self::ViewCacheViewWeight45k,
        Self::ViewCacheViewReplace45k,
        Self::ViewerRenderRawArtifact,
        Self::ViewerRenderRawArtifactSplitFull,
        Self::ViewerRenderDiffDocumentShell45k,
        Self::ViewerRenderMaterializedChunks45k,
        Self::ViewerRenderDiffDocumentShell115Files,
        Self::ViewerRenderMaterializedChunks115Files,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppStateRecordRender => "app-state-record-render",
            Self::ViewCacheViewWeight45k => "view-cache/view-weight/45k",
            Self::ViewCacheViewReplace45k => "view-cache/view-replace/45k",
            Self::ViewerRenderRawArtifact => "raw-artifact",
            Self::ViewerRenderRawArtifactSplitFull => "raw-artifact-split-full",
            Self::ViewerRenderDiffDocumentShell45k => "diff-document-shell-45k",
            Self::ViewerRenderMaterializedChunks45k => "materialized-chunks-45k",
            Self::ViewerRenderDiffDocumentShell115Files => "diff-document-shell-115-files",
            Self::ViewerRenderMaterializedChunks115Files => "materialized-chunks-115-files",
        }
    }

    pub const fn benchmark(self) -> Benchmark {
        match self {
            Self::AppStateRecordRender => Benchmark::AppStateRecordRender,
            Self::ViewCacheViewWeight45k | Self::ViewCacheViewReplace45k => Benchmark::ViewCache,
            Self::ViewerRenderRawArtifact
            | Self::ViewerRenderRawArtifactSplitFull
            | Self::ViewerRenderDiffDocumentShell45k
            | Self::ViewerRenderMaterializedChunks45k
            | Self::ViewerRenderDiffDocumentShell115Files
            | Self::ViewerRenderMaterializedChunks115Files => Benchmark::ViewerRender,
        }
    }
}

impl std::fmt::Display for BenchmarkCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl ValueEnum for BenchmarkCase {
    fn value_variants<'variants>() -> &'variants [Self] {
        &Self::ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.as_str()))
    }
}
