#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::{
    collections::BTreeMap,
    env, fs,
    fs::File,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail, ensure};
use gtl_benchmarks::release_server::{ReleaseServerConfig, ReleaseServerProcess};
use gtl_client::GtlClient;
use gtl_local_auth::{CapabilityToken, LocalAuth, ServerEndpoint};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::report::{
    BENCHMARK_NAME, Compatibility, LaunchMeasurement, REPORT_BYTES_MAX, REPORT_FORMAT_VERSION,
    Report, Summary, Transport,
};

const GHZ_VERSION: &str = "v0.121.0";
const BENCHMARK_CONFIG: &str = "benchmarks/grpc/get_push_confirmation_requirement.toml";
const SMOKE_CONFIG: &str = "benchmarks/grpc/get_push_confirmation_requirement_smoke.toml";
const CURRENT_REPORT: &str = ".artifacts/benchmarks/grpc-transport/current.json";
const BASELINE_REPORT: &str = ".artifacts/benchmarks/grpc-transport/baseline.json";
const RPC: &str = "gtl.v1.SettingsService.GetPushConfirmationRequirement";
const PROTO: &str = "crates/gtl-wire/proto/gtl/v1/settings.proto";
const SETTINGS: &str = "[push]\nconfirm = true\n";
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(60);
const SERVER_READY_RETRY_DELAY: Duration = Duration::from_millis(10);
const RSS_SAMPLE_INTERVAL: Duration = Duration::from_millis(1);
const TOOL_OUTPUT_BYTES_MAX: usize = 64 * 1_024;
const TRACKED_INPUT_BYTES_MAX: usize = 1_024 * 1_024;
const GHZ_OUTPUT_BYTES_MAX: u64 = 16 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Benchmark { update: bool },
    Smoke,
}

impl Mode {
    const fn workload(self) -> Workload {
        match self {
            Self::Benchmark { .. } => Workload {
                config: BENCHMARK_CONFIG,
                launches: 3,
                total_request_count: 10_200,
                warmup_request_count: 200,
                ghz_wall_timeout: Duration::from_secs(600),
                worker_wall_time_minutes: 30,
            },
            Self::Smoke => Workload {
                config: SMOKE_CONFIG,
                launches: 1,
                total_request_count: 120,
                warmup_request_count: 20,
                ghz_wall_timeout: Duration::from_secs(30),
                worker_wall_time_minutes: 2,
            },
        }
    }

    const fn update(self) -> bool {
        matches!(self, Self::Benchmark { update: true })
    }

    const fn requires_clean_repository(self) -> bool {
        matches!(self, Self::Benchmark { .. })
    }

    fn report_paths(self, repository_root: &Path) -> Option<ReportPaths> {
        matches!(self, Self::Benchmark { .. }).then(|| ReportPaths {
            current: repository_root.join(CURRENT_REPORT),
            baseline: repository_root.join(BASELINE_REPORT),
        })
    }
}

#[derive(Clone, Copy)]
struct Workload {
    config: &'static str,
    launches: usize,
    total_request_count: usize,
    warmup_request_count: usize,
    ghz_wall_timeout: Duration,
    worker_wall_time_minutes: u64,
}

struct Paths {
    repository_root: PathBuf,
    server_binary: PathBuf,
    ghz_binary: PathBuf,
    ghz_config: PathBuf,
}

struct ReportPaths {
    current: PathBuf,
    baseline: PathBuf,
}

pub(crate) async fn run(mode: Mode) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "the bounded gRPC transport benchmark requires Linux systemd"
    );
    let workload = mode.workload();
    let paths = Paths::discover(workload)?;
    if mode.requires_clean_repository() {
        ensure_clean_repository(&paths.repository_root)?;
    }
    let compatibility = compatibility(&paths, workload)?;
    let report_paths = mode.report_paths(&paths.repository_root);
    let baseline = report_paths
        .as_ref()
        .map(|paths| preflight_baseline(&paths.baseline, &compatibility, mode.update()))
        .transpose()?
        .flatten();
    let source_commit = source_commit(&paths.repository_root)?;
    let started = Instant::now();
    let (transport, launches) = measure_launches(&paths, &compatibility, workload).await?;
    let report = Report {
        format_version: REPORT_FORMAT_VERSION,
        benchmark: BENCHMARK_NAME.to_owned(),
        source_commit,
        transport,
        compatibility,
        launches,
    };
    report.validate()?;

    let Some(report_paths) = report_paths else {
        println!(
            "gRPC transport smoke passed ({}; {} measured requests after {} warmup requests)",
            report.transport,
            report.compatibility.measured_request_count,
            report.compatibility.warmup_request_count
        );
        return Ok(());
    };
    write_report(&report_paths.current, &report)?;
    println!("gRPC transport report: {}", report_paths.current.display());
    let summary = Summary::from_report(&report)?;
    print_summary("current", summary);
    if let Some(baseline) = baseline {
        print_summary("baseline", Summary::from_report(&baseline)?);
    }
    if mode.update() {
        write_report(&report_paths.baseline, &report)?;
        println!(
            "gRPC transport baseline: {}",
            report_paths.baseline.display()
        );
    }
    eprintln!(
        "gRPC transport measurement completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

impl Paths {
    fn discover(workload: Workload) -> Result<Self> {
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("benchmark manifest has no repository parent")?
            .canonicalize()
            .context("resolve repository root")?;
        let binary_directory = env::current_exe()
            .context("resolve benchmark executable")?
            .parent()
            .context("benchmark executable has no parent")?
            .to_path_buf();
        let ghz_binary = PathBuf::from(command_output(
            Path::new("mise"),
            &["which", "ghz"],
            &repository_root,
            "resolve the Mise-managed ghz binary",
        )?);
        let paths = Self {
            server_binary: binary_directory.join("gtl-server"),
            ghz_config: repository_root.join(workload.config),
            ghz_binary,
            repository_root,
        };
        for (description, path) in [
            ("release server", &paths.server_binary),
            ("ghz", &paths.ghz_binary),
            ("tracked ghz config", &paths.ghz_config),
        ] {
            ensure!(
                path.is_file(),
                "{description} is not a file: {}",
                path.display()
            );
        }
        Ok(paths)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GhzConfig {
    proto: PathBuf,
    call: String,
    total: usize,
    #[serde(rename = "skipFirst")]
    skip_first: usize,
    concurrency: u32,
    connections: u32,
    cpus: u32,
    insecure: bool,
    timeout: String,
    #[serde(rename = "disable-template-data")]
    disable_template_data: bool,
    data: toml::Table,
}

fn compatibility(paths: &Paths, workload: Workload) -> Result<Compatibility> {
    let config_bytes = read_tracked_input(&paths.ghz_config)?;
    let config: GhzConfig = toml::from_str(
        std::str::from_utf8(&config_bytes).context("tracked ghz config is not UTF-8")?,
    )
    .context("decode tracked ghz config")?;
    ensure!(
        config.proto == Path::new(PROTO)
            && config.call == RPC
            && config.total == workload.total_request_count
            && config.skip_first == workload.warmup_request_count
            && config.concurrency == 1
            && config.connections == 1
            && config.cpus == 1
            && config.insecure
            && config.timeout == "2s"
            && config.disable_template_data
            && config.data.is_empty(),
        "tracked ghz config drifted from its fixed sequential transport workload"
    );
    let proto_bytes = read_tracked_input(&paths.repository_root.join(&config.proto))?;
    let ghz_version = command_output(
        &paths.ghz_binary,
        &["--version"],
        &paths.repository_root,
        "read the ghz version",
    )?;
    ensure!(
        ghz_version == GHZ_VERSION,
        "ghz version drifted: expected {GHZ_VERSION}, found {ghz_version}"
    );
    Ok(Compatibility {
        build_profile: "release".to_owned(),
        rpc: config.call,
        launch_count: workload.launches,
        total_request_count: config.total,
        warmup_request_count: config.skip_first,
        measured_request_count: config
            .total
            .checked_sub(config.skip_first)
            .context("ghz warmup requests exceed total requests")?,
        protobuf_sha256: sha256(&proto_bytes),
        ghz_config_sha256: sha256(&config_bytes),
        resource_bounds: format!(
            "CPUQuota=200%; MemoryMax=2147483648; MemorySwapMax=0; TasksMax=128; nice=10; cargo jobs=1; wall={}m; termination grace={}s; ghz timeout={}s; RSS interval={}us",
            workload.worker_wall_time_minutes,
            15,
            workload.ghz_wall_timeout.as_secs(),
            RSS_SAMPLE_INTERVAL.as_micros()
        ),
        runner: runner_environment(&ghz_version)?,
    })
}

fn runner_environment(ghz_version: &str) -> Result<String> {
    let mut system = System::new();
    system.refresh_cpu_all();
    let cpu_model = system
        .cpus()
        .first()
        .map(sysinfo::Cpu::brand)
        .filter(|value| !value.is_empty())
        .context("CPU model is unavailable")?
        .to_owned();
    Ok(format!(
        "{}; {}; {}; {}; {} logical CPUs; {}; ghz {ghz_version}; sysinfo 0.39.6 process ancestry",
        System::long_os_version()
            .or_else(System::name)
            .context("operating system version is unavailable")?,
        System::kernel_long_version(),
        System::cpu_arch(),
        cpu_model,
        thread::available_parallelism()
            .context("logical CPU count is unavailable")?
            .get(),
        command_output(
            Path::new("rustc"),
            &["--version"],
            Path::new("."),
            "read the rustc version",
        )?
    ))
}

fn ensure_clean_repository(repository_root: &Path) -> Result<()> {
    let status = command_output(
        Path::new("git"),
        &["status", "--porcelain=v1", "--untracked-files=all"],
        repository_root,
        "inspect repository status",
    )?;
    ensure!(
        status.is_empty(),
        "gRPC transport measurement requires an exact committed source tree; dirty paths:\n{status}"
    );
    Ok(())
}

fn source_commit(repository_root: &Path) -> Result<String> {
    command_output(
        Path::new("git"),
        &["rev-parse", "HEAD"],
        repository_root,
        "read the source commit",
    )
}

fn preflight_baseline(
    path: &Path,
    compatibility: &Compatibility,
    update: bool,
) -> Result<Option<Report>> {
    if !path
        .try_exists()
        .context("inspect gRPC transport baseline")?
    {
        ensure!(
            update,
            "gRPC transport baseline is missing at {}; run `just bench-grpc --update` first",
            path.display()
        );
        return Ok(None);
    }
    match read_report(path) {
        Ok(report) if report.is_compatible_with(compatibility) => Ok(Some(report)),
        Ok(_) if update => {
            eprintln!("existing gRPC transport baseline is incompatible and will be replaced");
            Ok(None)
        }
        Err(error) if update => {
            eprintln!(
                "existing gRPC transport baseline is unreadable and will be replaced: {error:#}"
            );
            Ok(None)
        }
        Ok(_) => bail!("gRPC transport baseline is incompatible; use --update to replace it"),
        Err(error) => Err(error).context(
            "gRPC transport baseline is unreadable or incompatible; use --update to replace it",
        ),
    }
}

fn read_report(path: &Path) -> Result<Report> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("inspect gRPC transport report {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() <= REPORT_BYTES_MAX,
        "gRPC transport report is not a file of at most {REPORT_BYTES_MAX} bytes: {}",
        path.display()
    );
    let report: Report = serde_json::from_reader(std::io::BufReader::new(
        File::open(path).with_context(|| format!("open report {}", path.display()))?,
    ))
    .with_context(|| format!("decode report {}", path.display()))?;
    report.validate()?;
    Ok(report)
}

fn write_report(path: &Path, report: &Report) -> Result<u64> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .context("gRPC transport report path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create report directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary report in {}", parent.display()))?;
    serde_json::to_writer(&mut temporary, report)
        .context("encode compact gRPC transport report")?;
    temporary.write_all(b"\n").context("finish report")?;
    temporary.flush().context("flush report")?;
    let bytes = temporary
        .as_file()
        .metadata()
        .context("inspect report size")?
        .len();
    ensure!(
        bytes <= REPORT_BYTES_MAX,
        "gRPC transport report exceeds {REPORT_BYTES_MAX} bytes"
    );
    temporary.as_file().sync_all().context("sync report")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("atomically publish report {}", path.display()))?;
    Ok(bytes)
}

async fn measure_launches(
    paths: &Paths,
    compatibility: &Compatibility,
    workload: Workload,
) -> Result<(Transport, Vec<LaunchMeasurement>)> {
    let mut transport = None;
    let mut launches = Vec::with_capacity(compatibility.launch_count);
    for launch in 1..=compatibility.launch_count {
        eprintln!(
            "gRPC transport launch {launch}/{}: {} requests ({} warmup)",
            compatibility.launch_count,
            compatibility.total_request_count,
            compatibility.warmup_request_count
        );
        let started = Instant::now();
        let (launch_transport, measurement) =
            measure_launch(paths, compatibility, workload).await?;
        ensure!(
            transport.is_none_or(|expected| expected == launch_transport),
            "server transport changed between independent launches"
        );
        transport = Some(launch_transport);
        launches.push(measurement);
        eprintln!(
            "gRPC transport launch {launch}/{} completed in {:.1}s",
            compatibility.launch_count,
            started.elapsed().as_secs_f64()
        );
    }
    Ok((
        transport.context("gRPC transport benchmark produced no launches")?,
        launches,
    ))
}

async fn measure_launch(
    paths: &Paths,
    compatibility: &Compatibility,
    workload: Workload,
) -> Result<(Transport, LaunchMeasurement)> {
    let sandbox = tempfile::Builder::new()
        .prefix("grpc-transport-")
        .tempdir()
        .context("create benchmark sandbox")?;
    let mut server = ReleaseServerProcess::start(
        ReleaseServerConfig {
            server_binary: &paths.server_binary,
            settings: SETTINGS,
            profiler: None,
        },
        sandbox.path(),
    )?;
    let measurement =
        measure_running_server(paths, compatibility, workload, sandbox.path(), &mut server).await;
    let stop = server.stop();
    match (measurement, stop) {
        (Ok(measurement), Ok(())) => Ok(measurement),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error).context("stop benchmark server"),
        (Err(error), Err(stop_error)) => {
            Err(error).context(format!("server shutdown also failed: {stop_error:#}"))
        }
    }
}

async fn measure_running_server(
    paths: &Paths,
    compatibility: &Compatibility,
    workload: Workload,
    sandbox: &Path,
    server: &mut ReleaseServerProcess,
) -> Result<(Transport, LaunchMeasurement)> {
    let access = wait_for_server_access(server).await?;
    let process_id = server.process_id();
    let cpu_before = process_tree_cpu_milliseconds(process_id)?;
    let ghz = run_ghz(paths, compatibility, workload, sandbox, &access, process_id)?;
    server.ensure_running()?;
    let cpu_after = process_tree_cpu_milliseconds(process_id)?;
    let server_process_tree_cpu_milliseconds = cpu_after
        .checked_sub(cpu_before)
        .context("server process-tree CPU time moved backwards")?;
    ensure!(
        server_process_tree_cpu_milliseconds > 0,
        "server process-tree CPU measurement is empty"
    );
    Ok((
        access.transport,
        LaunchMeasurement {
            request_latency_nanoseconds: ghz.request_latency_nanoseconds,
            requests_per_second: ghz.requests_per_second,
            server_process_tree_cpu_milliseconds,
            peak_server_and_client_process_tree_rss_bytes: ghz.peak_rss_bytes,
        },
    ))
}

struct ServerAccess {
    target: String,
    transport: Transport,
    capability: CapabilityToken,
}

async fn wait_for_server_access(server: &mut ReleaseServerProcess) -> Result<ServerAccess> {
    let auth =
        LocalAuth::from_data_root(server.data_root()).context("open isolated server auth")?;
    let deadline = deadline(SERVER_READY_TIMEOUT)?;
    loop {
        server.ensure_running()?;
        let last_error = match GtlClient::connect(&auth).await {
            Ok(client) => match client.get_push_confirmation_requirement().await {
                Ok(response) => {
                    ensure!(
                        response.push_confirmation_required,
                        "isolated server ignored the fixed push confirmation setting"
                    );
                    let endpoint = auth.load_endpoint().context("load benchmark endpoint")?;
                    let (target, transport) = benchmark_target(&endpoint);
                    return Ok(ServerAccess {
                        target,
                        transport,
                        capability: auth
                            .load_client_token()
                            .context("load benchmark capability")?,
                    });
                }
                Err(error) => format!("settings request failed: {error}"),
            },
            Err(error) => format!("connection failed: {error}"),
        };
        ensure!(
            Instant::now() < deadline,
            "release server was not ready within {} seconds: {last_error}",
            SERVER_READY_TIMEOUT.as_secs()
        );
        tokio::time::sleep(SERVER_READY_RETRY_DELAY).await;
    }
}

fn benchmark_target(endpoint: &ServerEndpoint) -> (String, Transport) {
    #[cfg(unix)]
    {
        (
            format!("unix://{}", endpoint.uds_path().to_string_lossy()),
            Transport::Uds,
        )
    }
    #[cfg(windows)]
    {
        (endpoint.tcp_address().to_string(), Transport::Tcp)
    }
}

#[derive(Serialize)]
struct GhzMetadata {
    authorization: String,
}

#[derive(Deserialize)]
struct GhzOutput {
    count: usize,
    rps: f64,
    #[serde(rename = "errorDistribution")]
    error_distribution: BTreeMap<String, usize>,
    #[serde(rename = "statusCodeDistribution")]
    status_code_distribution: BTreeMap<String, usize>,
    details: Vec<GhzSample>,
}

#[derive(Deserialize)]
struct GhzSample {
    #[serde(rename = "latency")]
    latency_nanoseconds: u64,
    error: String,
    status: String,
}

struct GhzMeasurement {
    request_latency_nanoseconds: Vec<u64>,
    requests_per_second: f64,
    peak_rss_bytes: u64,
}

impl GhzOutput {
    fn into_measurement(
        self,
        measured_request_count: usize,
        peak_rss_bytes: u64,
    ) -> Result<GhzMeasurement> {
        ensure!(
            self.count == measured_request_count
                && self.details.len() == measured_request_count
                && self.error_distribution.is_empty()
                && self.status_code_distribution.len() == 1
                && self.status_code_distribution.get("OK") == Some(&measured_request_count)
                && self.rps.is_finite()
                && self.rps > 0.0
                && self.details.iter().all(|sample| {
                    sample.latency_nanoseconds > 0
                        && sample.error.is_empty()
                        && sample.status == "OK"
                }),
            "ghz did not retain exactly {measured_request_count} successful requests"
        );
        ensure!(peak_rss_bytes > 0, "process-tree RSS measurement is empty");
        Ok(GhzMeasurement {
            request_latency_nanoseconds: self
                .details
                .into_iter()
                .map(|sample| sample.latency_nanoseconds)
                .collect(),
            requests_per_second: self.rps,
            peak_rss_bytes,
        })
    }
}

fn run_ghz(
    paths: &Paths,
    compatibility: &Compatibility,
    workload: Workload,
    sandbox: &Path,
    access: &ServerAccess,
    server_process_id: u32,
) -> Result<GhzMeasurement> {
    let mut metadata = tempfile::NamedTempFile::new_in(sandbox).context("create ghz metadata")?;
    serde_json::to_writer(
        &mut metadata,
        &GhzMetadata {
            authorization: format!("Bearer {}", access.capability.expose_secret()),
        },
    )
    .context("encode ghz metadata")?;
    metadata.flush().context("flush ghz metadata")?;
    let output = tempfile::NamedTempFile::new_in(sandbox).context("create ghz output")?;
    let mut command = Command::new(&paths.ghz_binary);
    command
        .current_dir(&paths.repository_root)
        .arg(format!("--config={}", paths.ghz_config.display()))
        .arg(format!("--metadata-file={}", metadata.path().display()))
        .arg("--format=json")
        .arg(format!("--output={}", output.path().display()))
        .arg(&access.target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    configure_process_group(&mut command);
    let mut child = command.spawn().context("start ghz transport workload")?;
    let (status, peak_rss_bytes) = wait_for_ghz(
        &mut child,
        server_process_id,
        deadline(workload.ghz_wall_timeout)?,
    )?;
    ensure!(status.success(), "ghz failed with {status}");
    ensure!(
        output.as_file().metadata()?.len() <= GHZ_OUTPUT_BYTES_MAX,
        "ghz output exceeds {GHZ_OUTPUT_BYTES_MAX} bytes"
    );
    let output: GhzOutput = serde_json::from_reader(std::io::BufReader::new(output.reopen()?))
        .context("decode ghz output")?;
    output.into_measurement(compatibility.measured_request_count, peak_rss_bytes)
}

fn wait_for_ghz(
    child: &mut Child,
    server_process_id: u32,
    deadline: Instant,
) -> Result<(ExitStatus, u64)> {
    let roots = [Pid::from_u32(server_process_id), Pid::from_u32(child.id())];
    let mut system = System::new();
    let mut peak = 0;
    loop {
        refresh_processes(&mut system);
        ensure!(
            system.process(roots[0]).is_some(),
            "server disappeared during RSS sampling"
        );
        let rss = system
            .processes()
            .iter()
            .filter(|(process_id, _)| belongs_to_process_trees(**process_id, &roots, &system))
            .try_fold(0_u64, |total, (_, process)| {
                total
                    .checked_add(process.memory())
                    .context("process-tree RSS overflowed")
            })?;
        peak = peak.max(rss);
        if let Some(status) = child.try_wait().context("poll ghz workload")? {
            return Ok((status, peak));
        }
        if Instant::now() >= deadline {
            terminate_and_reap(child).context("terminate overdue ghz workload")?;
            bail!("ghz workload exceeded its deadline");
        }
        thread::sleep(RSS_SAMPLE_INTERVAL);
    }
}

fn process_tree_cpu_milliseconds(root_process_id: u32) -> Result<u64> {
    let root = Pid::from_u32(root_process_id);
    let mut system = System::new();
    refresh_processes(&mut system);
    ensure!(
        system.process(root).is_some(),
        "server process is not visible to sysinfo"
    );
    system
        .processes()
        .iter()
        .filter(|(process_id, _)| belongs_to_process_trees(**process_id, &[root], &system))
        .try_fold(0_u64, |total, (_, process)| {
            total
                .checked_add(process.accumulated_cpu_time())
                .context("process-tree CPU time overflowed")
        })
}

fn refresh_processes(system: &mut System) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .without_tasks(),
    );
}

fn belongs_to_process_trees(process_id: Pid, roots: &[Pid], system: &System) -> bool {
    let mut current = Some(process_id);
    for _ in 0..=system.processes().len() {
        let Some(process_id) = current else {
            return false;
        };
        if roots.contains(&process_id) {
            return true;
        }
        current = system
            .process(process_id)
            .and_then(sysinfo::Process::parent);
    }
    false
}

fn deadline(timeout: Duration) -> Result<Instant> {
    Instant::now()
        .checked_add(timeout)
        .context("operation deadline overflowed")
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    command.process_group(0);
}

fn terminate_and_reap(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    {
        let process_id = i32::try_from(child.id()).context("process ID exceeds i32")?;
        // kill receives scalar process-group and signal identifiers.
        let result = unsafe { libc::kill(-process_id, libc::SIGKILL) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(std::io::Error::last_os_error()).context("kill process group");
        }
    }
    #[cfg(windows)]
    child.kill().context("kill process")?;
    child.wait().context("reap process")?;
    Ok(())
}

fn command_output(
    program: &Path,
    arguments: &[&str],
    current_directory: &Path,
    description: &str,
) -> Result<String> {
    let output = Command::new("/usr/bin/timeout")
        .arg("--signal=KILL")
        .arg("30s")
        .arg(program)
        .args(arguments)
        .current_dir(current_directory)
        .output()
        .with_context(|| format!("{description}: run {}", program.display()))?;
    ensure!(
        output.stdout.len() <= TOOL_OUTPUT_BYTES_MAX
            && output.stderr.len() <= TOOL_OUTPUT_BYTES_MAX,
        "command output exceeded {TOOL_OUTPUT_BYTES_MAX} bytes while trying to {description}"
    );
    ensure!(
        output.status.success(),
        "command failed while trying to {description}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let bytes = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    String::from_utf8(bytes)
        .with_context(|| format!("command output is not UTF-8 while trying to {description}"))
        .map(|output| output.trim().to_owned())
}

fn read_tracked_input(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("read tracked input {}", path.display()))?;
    ensure!(
        bytes.len() <= TRACKED_INPUT_BYTES_MAX,
        "tracked input exceeds {TRACKED_INPUT_BYTES_MAX} bytes: {}",
        path.display()
    );
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn print_summary(label: &str, summary: Summary) {
    println!(
        "{label}: latency p50/p95/p99 {:.3}/{:.3}/{:.3} ms; throughput {:.3} req/s; server CPU {:.3} us/request; peak RSS {:.3} MiB",
        summary.latency_p50_nanoseconds / 1_000_000.0,
        summary.latency_p95_nanoseconds / 1_000_000.0,
        summary.latency_p99_nanoseconds / 1_000_000.0,
        summary.requests_per_second_median,
        summary.server_cpu_nanoseconds_per_request_median / 1_000.0,
        summary.peak_process_tree_rss_bytes / 1_048_576.0,
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn ghz_output_constructs_only_validated_latency_evidence() {
        let raw = r#"{"count":2,"rps":10.0,"errorDistribution":{},"statusCodeDistribution":{"OK":2},"details":[{"timestamp":"ignored","latency":100,"error":"","status":"OK"},{"timestamp":"ignored","latency":200,"error":"","status":"OK"}]}"#;
        let output: GhzOutput = serde_json::from_str(raw).unwrap();

        let measurement = output.into_measurement(2, 1_024).unwrap();

        assert_eq!(measurement.request_latency_nanoseconds, [100, 200]);
        let mut output: GhzOutput = serde_json::from_str(raw).unwrap();
        output.details[0].error = "failure".to_owned();
        assert!(output.into_measurement(2, 1_024).is_err());
    }

    #[test]
    fn smoke_has_no_persistent_report_paths_or_clean_tree_requirement() {
        let root = Path::new("/synthetic/repository");

        assert!(Mode::Smoke.report_paths(root).is_none());
        assert!(!Mode::Smoke.requires_clean_repository());
        assert!(
            Mode::Benchmark { update: false }
                .report_paths(root)
                .is_some()
        );
    }

    #[test]
    fn report_publication_atomically_replaces_the_destination() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("current.json");
        fs::write(&path, "incomplete").unwrap();

        write_report(&path, &crate::report::tests::report()).unwrap();

        assert_eq!(read_report(&path).unwrap().source_commit, "a".repeat(40));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_and_reaps_the_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let child_pid_path = directory.path().join("child.pid");
        let mut command = Command::new("sh");
        command
            .env("SYNTHETIC_CHILD_PID_PATH", &child_pid_path)
            .args([
                "-c",
                "sleep 2147483647 & printf '%s' \"$!\" > \"$SYNTHETIC_CHILD_PID_PATH\"; wait",
            ]);
        configure_process_group(&mut command);
        let mut child = command.spawn().unwrap();
        while !child_pid_path.exists() {
            thread::sleep(Duration::from_millis(1));
        }
        let child_process_id = fs::read_to_string(&child_pid_path)
            .unwrap()
            .parse::<u32>()
            .unwrap();

        assert!(wait_for_ghz(&mut child, std::process::id(), Instant::now()).is_err());
        assert!(wait_for_process_exit(
            child_process_id,
            Duration::from_secs(1)
        ));
    }

    fn wait_for_process_exit(process_id: u32, timeout: Duration) -> bool {
        let path = PathBuf::from(format!("/proc/{process_id}"));
        let deadline = Instant::now() + timeout;
        while path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        !path.exists()
    }
}
