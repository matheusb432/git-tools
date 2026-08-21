//! Deterministic committed workload for desktop viewer scroll measurements.

use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod generate;
mod repository;
mod verify;

pub const FIXTURE_RELATIVE_PATH: &str = "benchmarks/fixtures/desktop-scroll";
pub const GIT_RANGE: &str = "HEAD~10..HEAD";
pub const IDENTITY_NAME: &str = "Desktop Scroll Fixture";
pub const IDENTITY_EMAIL: &str = "desktop-scroll@example.invalid";
pub const BASE_SUBJECT: &str = "fixture: establish desktop scroll source baseline";
pub const BASE_TIMESTAMP: &str = "2026-01-01T12:00:00+00:00";
pub const COMMIT_COUNT: usize = 10;
pub const DISTINCT_FILE_COUNT: usize = 50;
pub const FILE_TOUCH_COUNT_PER_COMMIT: usize = 7;
pub const FIXTURE_INPUT_BYTES_MAX: u64 = 2 * 1024 * 1024;
pub const PATCH_FILE_BYTES_MAX: u64 = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopScrollManifest {
    pub format_version: u32,
    pub fixture_name: String,
    pub repository_name: String,
    pub git_range: String,
    pub identity: FixtureIdentity,
    pub base: FixtureBase,
    pub workload: FixtureWorkload,
    pub bounds: FixtureBounds,
    pub file_types: BTreeMap<String, usize>,
    pub statuses: FixtureStatuses,
    pub commits: Vec<FixtureCommit>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureIdentity {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureBase {
    pub commit_id: String,
    pub subject: String,
    pub authored_at: String,
    pub source_file_count: usize,
    pub source_bytes: u64,
    pub tree_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureWorkload {
    pub commit_count: usize,
    pub distinct_file_count: usize,
    pub file_touch_count_total: usize,
    pub patch_series_bytes: u64,
    pub compact_diff_rows: usize,
    pub compact_diff_bytes: u64,
    pub full_context_diff_rows: usize,
    pub full_context_diff_bytes: u64,
    pub additions: u64,
    pub deletions: u64,
    pub head_commit_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureBounds {
    pub fixture_input_bytes_max: u64,
    pub patch_file_bytes_max: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureStatuses {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub renamed: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixtureCommit {
    pub sequence: usize,
    pub patch: String,
    pub patch_sha256: String,
    pub commit_id: String,
    pub subject: String,
    pub body: String,
    pub authored_at: String,
    pub file_touch_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopScrollFixtureEvidence {
    pub manifest: DesktopScrollManifest,
    pub fixture_input_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopScrollFixtureError {
    #[error("{operation} `{path}`")]
    FileSystem {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("decode desktop scroll manifest `{path}`")]
    DecodeManifest {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("encode desktop scroll manifest")]
    EncodeManifest {
        #[source]
        source: toml::ser::Error,
    },
    #[error("Git {operation} failed with exit {exit_code}: {stderr}")]
    Git {
        operation: String,
        exit_code: i32,
        stderr: String,
    },
    #[error("invalid desktop scroll fixture: {reason}")]
    Invalid { reason: String },
}

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/desktop-scroll")
}

pub fn refresh_fixture(
    root: &Path,
) -> Result<DesktopScrollFixtureEvidence, DesktopScrollFixtureError> {
    generate::refresh_fixture(root)
}

pub fn hydrate_fixture(
    root: &Path,
    destination: &Path,
) -> Result<DesktopScrollManifest, DesktopScrollFixtureError> {
    verify::hydrate_fixture(root, destination)
}

pub fn verify_fixture(
    root: &Path,
) -> Result<DesktopScrollFixtureEvidence, DesktopScrollFixtureError> {
    verify::verify_fixture(root)
}

pub(super) fn file_system_error<'path>(
    operation: &'static str,
    path: &'path Path,
) -> impl FnOnce(io::Error) -> DesktopScrollFixtureError + 'path {
    move |source| DesktopScrollFixtureError::FileSystem {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

pub(super) fn invalid(reason: impl Into<String>) -> DesktopScrollFixtureError {
    DesktopScrollFixtureError::Invalid {
        reason: reason.into(),
    }
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

pub(super) fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
