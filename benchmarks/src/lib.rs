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
    BenchmarkCase::ViewerRenderRawArtifact,
    BenchmarkCase::ViewerRenderRawArtifactSplitFull,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Benchmark {
    AppStateRecordRender,
    GrpcRequests,
    ParserSyntax,
    ViewCache,
    ViewerRender,
}

impl Benchmark {
    const ALL: [Self; 5] = [
        Self::AppStateRecordRender,
        Self::GrpcRequests,
        Self::ParserSyntax,
        Self::ViewCache,
        Self::ViewerRender,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppStateRecordRender => "app-state-record-render",
            Self::GrpcRequests => "grpc-requests",
            Self::ParserSyntax => "parser-syntax",
            Self::ViewCache => "view-cache",
            Self::ViewerRender => "viewer-render",
        }
    }

    pub const fn cargo_target(self) -> &'static str {
        match self {
            Self::AppStateRecordRender => "app_state_record_render",
            Self::GrpcRequests => "grpc_requests",
            Self::ParserSyntax => "parser_syntax",
            Self::ViewCache => "view_cache",
            Self::ViewerRender => "viewer_render",
        }
    }

    pub const fn sample_size(self) -> usize {
        match self {
            Self::ParserSyntax | Self::ViewerRender => 10,
            Self::AppStateRecordRender | Self::GrpcRequests | Self::ViewCache => 100,
        }
    }

    pub const fn fast_cases(self) -> &'static [BenchmarkCase] {
        match self {
            Self::ViewerRender => VIEWER_RENDER_FAST_CASES,
            Self::AppStateRecordRender
            | Self::GrpcRequests
            | Self::ParserSyntax
            | Self::ViewCache => &[],
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
    GrpcRequestsGetSettings,
    GrpcRequestsGetRepositoryStatus,
    GrpcRequestsPrepareDiffUnpushed,
    ParserSyntaxRust45k,
    ViewCacheViewWeight45k,
    ViewCacheViewReplace45k,
    ViewerRenderRawArtifact,
    ViewerRenderRawArtifactSplitFull,
}

impl BenchmarkCase {
    const ALL: [Self; 9] = [
        Self::AppStateRecordRender,
        Self::GrpcRequestsGetSettings,
        Self::GrpcRequestsGetRepositoryStatus,
        Self::GrpcRequestsPrepareDiffUnpushed,
        Self::ParserSyntaxRust45k,
        Self::ViewCacheViewWeight45k,
        Self::ViewCacheViewReplace45k,
        Self::ViewerRenderRawArtifact,
        Self::ViewerRenderRawArtifactSplitFull,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppStateRecordRender => "app-state-record-render",
            Self::GrpcRequestsGetSettings => "grpc-requests/get-settings",
            Self::GrpcRequestsGetRepositoryStatus => "grpc-requests/get-repository-status",
            Self::GrpcRequestsPrepareDiffUnpushed => "grpc-requests/prepare-diff-unpushed",
            Self::ParserSyntaxRust45k => "parser-syntax/rust-45k",
            Self::ViewCacheViewWeight45k => "view-cache/view-weight/45k",
            Self::ViewCacheViewReplace45k => "view-cache/view-replace/45k",
            Self::ViewerRenderRawArtifact => "raw-artifact",
            Self::ViewerRenderRawArtifactSplitFull => "raw-artifact-split-full",
        }
    }

    pub const fn benchmark(self) -> Benchmark {
        match self {
            Self::AppStateRecordRender => Benchmark::AppStateRecordRender,
            Self::GrpcRequestsGetSettings
            | Self::GrpcRequestsGetRepositoryStatus
            | Self::GrpcRequestsPrepareDiffUnpushed => Benchmark::GrpcRequests,
            Self::ParserSyntaxRust45k => Benchmark::ParserSyntax,
            Self::ViewCacheViewWeight45k | Self::ViewCacheViewReplace45k => Benchmark::ViewCache,
            Self::ViewerRenderRawArtifact | Self::ViewerRenderRawArtifactSplitFull => {
                Benchmark::ViewerRender
            }
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
