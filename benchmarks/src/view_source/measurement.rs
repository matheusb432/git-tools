use std::{env, fs, mem::MaybeUninit, path::Path, process::Command, sync::Arc, time::Instant};

use anyhow::{Context, Result, anyhow, ensure};
use gtl_application::{
    diffs::{
        DiffTarget, FetchFullContextDiff, FileStatus, FullContextDiffState, View,
        compute_diff::{self, ComputeDiff},
        fetch_full_context_diff,
    },
    ports::{UserSettingsLoadError, UserSettingsReader},
    viewer::session::CachedView,
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{
    diffs::DiffExclusions,
    git::GitRange,
    paths::RepositoryRoot,
    settings::{PushAllExclusions, UserSettings},
    viewer::{DiffDensity, DiffLayout, RenderOptions},
};
use sha2::{Digest, Sha256};

use super::{
    BENCHMARK_NAME, BenchmarkSource, CaseOrder, FixtureIdentity, LaunchFragment,
    OperationMeasurement, REPORT_FORMAT_VERSION, RunnerEnvironment, ViewSourceDescriptor,
    ViewSourceLaunch, ViewSourceOperation, ViewSourceProtocol, ViewSourceSample,
    ViewSourceWorkload,
    fixture::{MaterializedFixture, SPARSE_CHANGE_STRIDE, SPARSE_SOURCE_LINE_COUNT, materialize},
    validate_descriptor,
};

const COMMAND_OUTPUT_BYTES_MAX: usize = 64 * 1_024;

#[derive(Clone)]
struct CompactSettingsStore;

impl UserSettingsReader for CompactSettingsStore {
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
        Ok(UserSettings::new(
            None,
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Compact),
            gtl_models::viewer::ViewerKeybindings::default(),
            true,
            DiffExclusions::default(),
            PushAllExclusions::default(),
        ))
    }
}

pub fn describe() -> Result<ViewSourceDescriptor> {
    let temporary = tempfile::Builder::new()
        .prefix("view-source-describe-")
        .tempdir()
        .context("create view-source descriptor sandbox")?;
    let fixtures = inspect_fixtures(temporary.path())?;
    let descriptor = ViewSourceDescriptor {
        format_version: REPORT_FORMAT_VERSION,
        benchmark: BENCHMARK_NAME.to_owned(),
        profile: "release".to_owned(),
        fixtures,
        protocol: protocol(),
        runner: runner_environment()?,
    };
    validate_descriptor(&descriptor).context("validate view-source descriptor")?;
    Ok(descriptor)
}

pub fn measure_launch(
    launch: usize,
    source_commit: String,
    invocation: String,
) -> Result<LaunchFragment> {
    ensure!((1..=3).contains(&launch), "launch must be between 1 and 3");
    ensure!(
        source_commit.len() == 40 && source_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "source commit must be a full Git object ID"
    );
    let temporary = tempfile::Builder::new()
        .prefix(&format!("view-source-launch-{launch}-"))
        .tempdir()
        .context("create view-source launch sandbox")?;
    let source = HybridGitClient;
    let mut fixtures = Vec::new();
    let mut samples = Vec::new();
    for workload in ViewSourceWorkload::ALL {
        let materialized = materialize(
            workload,
            &temporary.path().join(format!("fixture-{workload}")),
        )
        .with_context(|| format!("materialize {workload} fixture"))?;
        let compact_seed = deferred_compact(&materialized, source)?;
        let (identity, sample) = measure_fixture(launch, &materialized, &compact_seed, source)?;
        fixtures.push(identity);
        samples.push(sample);
    }
    let descriptor = ViewSourceDescriptor {
        format_version: REPORT_FORMAT_VERSION,
        benchmark: BENCHMARK_NAME.to_owned(),
        profile: "release".to_owned(),
        fixtures,
        protocol: protocol(),
        runner: runner_environment()?,
    };
    validate_descriptor(&descriptor).context("validate measured view-source descriptor")?;
    Ok(LaunchFragment {
        source: BenchmarkSource {
            commit: source_commit,
            invocation,
            profile: "release".to_owned(),
        },
        descriptor,
        launch: ViewSourceLaunch { launch, samples },
    })
}

fn inspect_fixtures(root: &Path) -> Result<Vec<FixtureIdentity>> {
    let source = HybridGitClient;
    ViewSourceWorkload::ALL
        .into_iter()
        .map(|workload| {
            let fixture = materialize(workload, &root.join(format!("fixture-{workload}")))?;
            inspect_fixture(&fixture, source)
        })
        .collect()
}

fn inspect_fixture(
    fixture: &MaterializedFixture,
    source: HybridGitClient,
) -> Result<FixtureIdentity> {
    let compact = deferred_compact(fixture, source)?;
    let full = fetch_deferred_full_context(compact.clone(), source)
        .with_context(|| format!("load {} full-context reference", fixture.workload))?;
    ensure!(
        matches!(full.full_context, FullContextDiffState::Loaded),
        "{} full-context reference was not marked loaded",
        fixture.workload
    );
    fixture_identity(fixture, &compact, &full)
}

fn deferred_compact(fixture: &MaterializedFixture, source: HybridGitClient) -> Result<View> {
    let compact = compute_compact(fixture, source)?;
    ensure!(
        matches!(compact.full_context, FullContextDiffState::Deferred(_)),
        "{} compact fixture did not defer full-context source",
        fixture.workload
    );
    Ok(compact)
}

fn fixture_identity(
    fixture: &MaterializedFixture,
    compact: &View,
    full: &View,
) -> Result<FixtureIdentity> {
    let compact_semantic_sha256 = semantic_sha256(compact, false);
    let full_semantic_sha256 = semantic_sha256(full, true);
    Ok(FixtureIdentity {
        workload: fixture.workload,
        name: fixture.name.clone(),
        range: fixture.range.clone(),
        source_sha256: full_semantic_sha256.clone(),
        file_count: compact.files.len(),
        modified_file_count: compact
            .files
            .iter()
            .filter(|file| file.status() == FileStatus::Modified)
            .count(),
        compact_diff_rows: row_count(compact, false),
        compact_diff_bytes: byte_count(compact, false)?,
        full_context_diff_rows: row_count(full, true),
        full_context_diff_bytes: byte_count(full, true)?,
        compact_semantic_sha256,
        full_semantic_sha256,
    })
}

fn measure_fixture(
    launch: usize,
    fixture: &MaterializedFixture,
    compact_seed: &View,
    source: HybridGitClient,
) -> Result<(FixtureIdentity, ViewSourceSample)> {
    let mut compact = None;
    let mut eager = None;
    let mut transition = None;
    for operation in case_order(launch) {
        match operation {
            ViewSourceOperation::CompactConstruction => {
                compact = Some(measure(|| compute_compact(fixture, source))?);
            }
            ViewSourceOperation::EagerFullContextConstruction => {
                eager = Some(measure(|| compute_eager(fixture, source))?);
            }
            ViewSourceOperation::FirstFullContextTransition => {
                transition = Some(measure(|| transition_to_full(compact_seed, source))?);
            }
        }
    }
    let (compact, compact_measurement) = compact.context("compact measurement is missing")?;
    let (eager, eager_measurement) = eager.context("eager measurement is missing")?;
    let (transition, transition_measurement) =
        transition.context("transition measurement is missing")?;
    ensure!(
        compact == *compact_seed,
        "{} compact measurement changed row semantics",
        fixture.workload
    );
    ensure!(
        eager == transition,
        "{} eager and transition measurements changed full row semantics",
        fixture.workload
    );
    ensure!(
        matches!(eager.full_context, FullContextDiffState::Loaded),
        "{} full-context measurement was not marked loaded",
        fixture.workload
    );
    let identity = fixture_identity(fixture, &compact, &eager)?;
    let compact_cache_weight_bytes = cache_weight(compact)?;
    let full_context_cache_weight_bytes = cache_weight(eager)?;
    ensure!(
        full_context_cache_weight_bytes > compact_cache_weight_bytes,
        "{} full-context cache weight did not exceed compact weight",
        fixture.workload
    );
    let sample = ViewSourceSample {
        workload: fixture.workload,
        compact: compact_measurement,
        eager_full_context: eager_measurement,
        first_full_context_transition: transition_measurement,
        compact_cache_weight_bytes,
        full_context_cache_weight_bytes,
        compact_semantic_sha256: identity.compact_semantic_sha256.clone(),
        full_semantic_sha256: identity.full_semantic_sha256.clone(),
    };
    Ok((identity, sample))
}

fn compute_eager(fixture: &MaterializedFixture, source: HybridGitClient) -> Result<View> {
    let view = compute_compact(fixture, source)?;
    fetch_deferred_full_context(view, source)
}

fn transition_to_full(compact: &View, source: HybridGitClient) -> Result<View> {
    fetch_deferred_full_context(compact.clone(), source)
}

fn fetch_deferred_full_context(view: View, git: HybridGitClient) -> Result<View> {
    let source = match &view.full_context {
        FullContextDiffState::Deferred(source) => source,
        FullContextDiffState::Unavailable => {
            return Err(anyhow!("full-context source is unavailable"));
        }
        FullContextDiffState::Loaded => {
            return Err(anyhow!("full-context source is already loaded"));
        }
    };
    let request = FetchFullContextDiff::new(&view.repo_root, source);
    let full_context = fetch_full_context_diff::execute(&request, &git)?;
    view.with_full_context(full_context).map_err(Into::into)
}

fn compute_compact(fixture: &MaterializedFixture, source: HybridGitClient) -> Result<View> {
    let repo_root = RepositoryRoot::try_new(fixture.repository.clone())
        .context("fixture repository path is not absolute")?;
    let range = GitRange::try_new(fixture.range.clone()).context("fixture range is invalid")?;
    compute_diff::execute(
        ComputeDiff {
            repo_root,
            target: DiffTarget::Range {
                range,
                pinned: None,
            },
        },
        &CompactSettingsStore,
        &source,
        &std::collections::BTreeMap::new(),
    )
    .map(|response| response.view)
    .with_context(|| format!("compute {} compact view", fixture.workload))
}

fn measure<T>(operation: impl FnOnce() -> Result<T>) -> Result<(T, OperationMeasurement)> {
    let cpu_before = process_tree_cpu_time_nanoseconds()?;
    let wall_started = Instant::now();
    let value = operation()?;
    let wall_time_microseconds = wall_started
        .elapsed()
        .as_micros()
        .try_into()
        .context("wall-time measurement exceeds u64 microseconds")?;
    let process_tree_cpu_time_nanoseconds = process_tree_cpu_time_nanoseconds()?
        .checked_sub(cpu_before)
        .context("process-tree CPU accounting moved backwards")?;
    Ok((
        value,
        OperationMeasurement {
            wall_time_microseconds,
            process_tree_cpu_time_nanoseconds,
        },
    ))
}

#[cfg(unix)]
fn process_tree_cpu_time_nanoseconds() -> Result<u64> {
    rusage_cpu_time_nanoseconds(libc::RUSAGE_SELF)?
        .checked_add(rusage_cpu_time_nanoseconds(libc::RUSAGE_CHILDREN)?)
        .context("process-tree CPU time exceeds u64 nanoseconds")
}

#[cfg(unix)]
fn rusage_cpu_time_nanoseconds(who: libc::c_int) -> Result<u64> {
    let mut value = MaybeUninit::<libc::rusage>::uninit();
    // getrusage initializes the provided structure and retains no pointer to it.
    let status = unsafe { libc::getrusage(who, value.as_mut_ptr()) };
    ensure!(
        status == 0,
        "read process-tree CPU accounting: {}",
        std::io::Error::last_os_error()
    );
    // A successful getrusage call initialized the complete structure.
    let value = unsafe { value.assume_init() };
    timeval_nanoseconds(value.ru_utime)?
        .checked_add(timeval_nanoseconds(value.ru_stime)?)
        .context("resource-usage CPU time exceeds u64 nanoseconds")
}

#[cfg(unix)]
fn timeval_nanoseconds(value: libc::timeval) -> Result<u64> {
    let seconds = u64::try_from(value.tv_sec).context("CPU seconds are negative")?;
    let microseconds = u64::try_from(value.tv_usec).context("CPU microseconds are negative")?;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|seconds| {
            microseconds
                .checked_mul(1_000)
                .and_then(|micros| seconds.checked_add(micros))
        })
        .context("CPU time exceeds u64 nanoseconds")
}

#[cfg(not(unix))]
fn process_tree_cpu_time_nanoseconds() -> Result<u64> {
    Err(anyhow!(
        "view-source process-tree CPU measurement requires Unix"
    ))
}

fn cache_weight(view: View) -> Result<u64> {
    CachedView::new(Arc::new(view))
        .weight()
        .bytes()
        .try_into()
        .context("view cache weight exceeds u64 bytes")
}

fn row_count(view: &View, full: bool) -> usize {
    view.files
        .iter()
        .map(|file| selected_lines(file, full).len())
        .sum()
}

fn byte_count(view: &View, full: bool) -> Result<u64> {
    view.files
        .iter()
        .flat_map(|file| selected_lines(file, full))
        .try_fold(0_u64, |total, line| {
            let bytes = u64::try_from(line.len()).context("diff line length exceeds u64")?;
            total
                .checked_add(bytes)
                .context("diff byte count overflows")
        })
}

fn selected_lines(
    file: &gtl_application::diffs::FileDiff,
    full: bool,
) -> &gtl_application::diffs::source_lines::DiffSourceLines {
    if full {
        file.full_lines.as_ref().unwrap_or(&file.lines)
    } else {
        &file.lines
    }
}

fn semantic_sha256(view: &View, full: bool) -> String {
    let mut digest = Sha256::new();
    hash_usize(&mut digest, view.files.len());
    for file in &view.files {
        hash_bytes(
            &mut digest,
            file.path.as_path().as_os_str().as_encoded_bytes(),
        );
        digest.update(file.added.value().to_le_bytes());
        digest.update(file.removed.value().to_le_bytes());
        let lines = selected_lines(file, full);
        hash_usize(&mut digest, lines.len());
        for line in lines {
            hash_bytes(&mut digest, line.as_bytes());
        }
    }
    hex_bytes(&digest.finalize())
}

fn hash_usize(digest: &mut Sha256, value: usize) {
    digest.update(u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
}

fn hash_bytes(digest: &mut Sha256, value: &[u8]) {
    hash_usize(digest, value.len());
    digest.update(value);
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn protocol() -> ViewSourceProtocol {
    ViewSourceProtocol {
        independent_launches: 3,
        workloads: ViewSourceWorkload::ALL.to_vec(),
        case_order_by_launch: (1..=3)
            .map(|launch| CaseOrder {
                launch,
                operations: case_order(launch).to_vec(),
            })
            .collect(),
        process_tree_cpu_accounting: "getrusage(RUSAGE_SELF + RUSAGE_CHILDREN)".to_owned(),
        measurement_scope: "application diff construction and full-context source attachment; excludes server transport and row rendering".to_owned(),
        sparse_source_line_count: SPARSE_SOURCE_LINE_COUNT,
        sparse_change_stride: SPARSE_CHANGE_STRIDE,
    }
}

fn case_order(launch: usize) -> &'static [ViewSourceOperation] {
    match launch {
        1 => &ViewSourceOperation::ALL,
        2 => &[
            ViewSourceOperation::EagerFullContextConstruction,
            ViewSourceOperation::FirstFullContextTransition,
            ViewSourceOperation::CompactConstruction,
        ],
        3 => &[
            ViewSourceOperation::FirstFullContextTransition,
            ViewSourceOperation::CompactConstruction,
            ViewSourceOperation::EagerFullContextConstruction,
        ],
        _ => &[],
    }
}

fn runner_environment() -> Result<RunnerEnvironment> {
    Ok(RunnerEnvironment {
        operating_system: operating_system()?,
        kernel_release: command_output("uname", &["-r"])?,
        architecture: env::consts::ARCH.to_owned(),
        cpu_model: cpu_model()?,
        logical_cpu_count: std::thread::available_parallelism()
            .context("read logical CPU count")?
            .get(),
        rustc_version: command_output("rustc", &["--version"])?,
        cargo_version: command_output("cargo", &["--version"])?,
        git_version: command_output("git", &["--version"])?,
    })
}

fn command_output(program: &str, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .with_context(|| format!("run {program} {}", arguments.join(" ")))?;
    ensure!(
        output.stdout.len() <= COMMAND_OUTPUT_BYTES_MAX
            && output.stderr.len() <= COMMAND_OUTPUT_BYTES_MAX,
        "{program} output exceeded {COMMAND_OUTPUT_BYTES_MAX} bytes"
    );
    ensure!(
        output.status.success(),
        "{program} failed with exit {}: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout)
        .with_context(|| format!("{program} output is not UTF-8"))
        .map(|value| value.trim().to_owned())
}

fn operating_system() -> Result<String> {
    let release = fs::read_to_string("/etc/os-release").context("read /etc/os-release")?;
    release
        .lines()
        .find_map(|line| {
            line.strip_prefix("PRETTY_NAME=")
                .map(|value| value.trim_matches('"').to_owned())
        })
        .filter(|value| !value.is_empty())
        .context("/etc/os-release has no PRETTY_NAME")
}

fn cpu_model() -> Result<String> {
    let cpu_info = fs::read_to_string("/proc/cpuinfo").context("read /proc/cpuinfo")?;
    cpu_info
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.trim() == "model name")
                .map(|(_, value)| value.trim().to_owned())
        })
        .filter(|value| !value.is_empty())
        .context("/proc/cpuinfo has no model name")
}
