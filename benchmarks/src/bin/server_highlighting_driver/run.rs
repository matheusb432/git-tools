use std::{collections::BTreeMap, fs, io::Write as _, path::Path, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use gtl_benchmarks::server_highlighting::{
    BENCHMARK_NAME, BenchmarkSource, FixtureIdentity, HighlightLaunch, HighlightProtocol,
    HighlightSample, HighlightWorkload, REPORT_FORMAT_VERSION, ResidencyLaunch, ResidencySample,
    ServerHighlightingReport, StreamTemperature, clock_ticks_per_second, materialize_fixture,
    validate_report,
};
use gtl_wire::v1::{
    self, DiffTarget, GetViewerShellRequest, PresentDiffRequest, StreamViewerRowsRequest,
    diff_presentation, diff_target, viewer_active_state,
};

use super::{
    environment::{self, DriverConfig},
    measurement::measure_stream,
    server::{BenchmarkViewerClient, ServerClients, ServerProcess},
};

const VIEW_READY_TIMEOUT: Duration = Duration::from_secs(30);
const VIEW_READY_RETRY_DELAY: Duration = Duration::from_millis(10);

pub async fn run() -> Result<()> {
    let config = DriverConfig::from_environment()?;
    if config.massif.is_some() {
        return run_massif(&config).await;
    }
    let report = measure_report(&config).await?;
    validate_report(&report).context("validate completed server highlighting report")?;
    write_report(&config.report_path, &report)?;
    println!(
        "server highlighting report: {} independent streams, {} residency streams at {}",
        report.launches.len().saturating_mul(2),
        report
            .residency_launches
            .iter()
            .map(|launch| launch.samples.len())
            .sum::<usize>(),
        config.report_path.display()
    );
    Ok(())
}

async fn measure_report(config: &DriverConfig) -> Result<ServerHighlightingReport> {
    let mut fixture_identities = BTreeMap::new();
    let mut launches = Vec::new();
    for launch in 1..=config.launches {
        for workload in HighlightWorkload::ALL {
            let (identity, measured) = measure_independent(config, launch, workload).await?;
            record_fixture_identity(&mut fixture_identities, identity)?;
            launches.push(measured);
        }
    }
    let mut residency_launches = Vec::new();
    for launch in 1..=config.launches {
        let (identities, measured) = measure_residency(config, launch).await?;
        for identity in identities {
            record_fixture_identity(&mut fixture_identities, identity)?;
        }
        residency_launches.push(measured);
    }
    let fixtures = HighlightWorkload::ALL
        .into_iter()
        .map(|workload| {
            fixture_identities
                .remove(&workload)
                .with_context(|| format!("missing {workload} fixture identity"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ServerHighlightingReport {
        format_version: REPORT_FORMAT_VERSION,
        benchmark: BENCHMARK_NAME.to_owned(),
        source: BenchmarkSource {
            commit: config.source_commit.clone(),
            invocation: config.invocation.clone(),
            profile: "release".to_owned(),
        },
        fixtures,
        protocol: HighlightProtocol {
            independent_launches: config.launches,
            workloads: HighlightWorkload::ALL.to_vec(),
            temperatures: StreamTemperature::ALL.to_vec(),
            residency_sequence: residency_sequence().to_vec(),
            layout: "unified".to_owned(),
            density: "compact".to_owned(),
            stream_scope: "all-files".to_owned(),
            rss_attribution: "exact isolated gtl-server PID".to_owned(),
            rss_sample_interval_microseconds: config.rss_sample_interval_microseconds,
            process_cpu_clock_ticks_per_second: clock_ticks_per_second()
                .context("read process CPU clock frequency")?,
        },
        resource_bounds: config.resource_bounds.clone(),
        runner: environment::runner_environment()?,
        launches,
        residency_launches,
    })
}

async fn measure_independent(
    config: &DriverConfig,
    launch: usize,
    workload: HighlightWorkload,
) -> Result<(FixtureIdentity, HighlightLaunch)> {
    let conditions_before_launch = environment::system_conditions()?;
    let temporary = tempfile::Builder::new()
        .prefix(&format!("server-highlighting-{launch}-{workload}-"))
        .tempdir()
        .context("create independent benchmark sandbox")?;
    let fixture = materialize_fixture(workload, &temporary.path().join("repository"))
        .with_context(|| format!("materialize {workload} fixture"))?;
    let mut server = ServerProcess::start(config, temporary.path())?;
    let clients = server.connect().await?;
    let view = present_fixture(&clients, &fixture).await?;
    let request = StreamViewerRowsRequest {
        identity: Some(view),
        file_id: None,
        row_range: None,
    };
    let interval = Duration::from_micros(config.rss_sample_interval_microseconds);
    let cold = measure_stream(
        clients.viewer.clone(),
        request.clone(),
        server.process_id(),
        interval,
    )
    .await
    .with_context(|| format!("measure cold {workload} stream"))?;
    let warm = measure_stream(clients.viewer, request, server.process_id(), interval)
        .await
        .with_context(|| format!("measure warm {workload} stream"))?;
    ensure!(
        cold.evidence == warm.evidence,
        "cold and warm {workload} streams produced different semantics"
    );
    server.stop()?;
    Ok((
        fixture.identity,
        HighlightLaunch {
            launch,
            workload,
            conditions_before_launch,
            samples: vec![
                HighlightSample {
                    temperature: StreamTemperature::Cold,
                    measurement: cold,
                },
                HighlightSample {
                    temperature: StreamTemperature::Warm,
                    measurement: warm,
                },
            ],
        },
    ))
}

async fn measure_residency(
    config: &DriverConfig,
    launch: usize,
) -> Result<(Vec<FixtureIdentity>, ResidencyLaunch)> {
    let conditions_before_launch = environment::system_conditions()?;
    let temporary = tempfile::Builder::new()
        .prefix(&format!("server-highlighting-residency-{launch}-"))
        .tempdir()
        .context("create residency benchmark sandbox")?;
    let mut fixtures = BTreeMap::new();
    for workload in [HighlightWorkload::Rust, HighlightWorkload::Full] {
        let fixture = materialize_fixture(
            workload,
            &temporary.path().join(format!("repository-{workload}")),
        )
        .with_context(|| format!("materialize residency {workload} fixture"))?;
        fixtures.insert(workload, fixture);
    }
    let mut server = ServerProcess::start(config, temporary.path())?;
    let clients = server.connect().await?;
    let interval = Duration::from_micros(config.rss_sample_interval_microseconds);
    let mut samples = Vec::new();
    for workload in residency_sequence() {
        let fixture = fixtures
            .get(&workload)
            .with_context(|| format!("missing residency {workload} fixture"))?;
        let view = present_fixture(&clients, fixture).await?;
        let measurement = measure_stream(
            clients.viewer.clone(),
            StreamViewerRowsRequest {
                identity: Some(view),
                file_id: None,
                row_range: None,
            },
            server.process_id(),
            interval,
        )
        .await
        .with_context(|| format!("measure residency {workload} stream"))?;
        samples.push(ResidencySample {
            workload,
            measurement,
        });
    }
    server.stop()?;
    let identities = fixtures
        .into_values()
        .map(|fixture| fixture.identity)
        .collect();
    Ok((
        identities,
        ResidencyLaunch {
            launch,
            conditions_before_launch,
            samples,
        },
    ))
}

async fn run_massif(config: &DriverConfig) -> Result<()> {
    let massif = config
        .massif
        .as_ref()
        .context("Massif configuration is missing")?;
    let temporary = tempfile::Builder::new()
        .prefix("server-highlighting-massif-")
        .tempdir()
        .context("create Massif benchmark sandbox")?;
    let fixture = materialize_fixture(
        HighlightWorkload::Full,
        &temporary.path().join("repository"),
    )
    .context("materialize full Massif fixture")?;
    let mut server = ServerProcess::start(config, temporary.path())?;
    let clients = server.connect().await?;
    let view = present_fixture(&clients, &fixture).await?;
    let measurement = measure_stream(
        clients.viewer,
        StreamViewerRowsRequest {
            identity: Some(view),
            file_id: None,
            row_range: None,
        },
        server.process_id(),
        Duration::from_micros(config.rss_sample_interval_microseconds),
    )
    .await
    .context("run full-language stream under Massif")?;
    ensure!(
        measurement.evidence.row_count > 0,
        "Massif stream produced no rows"
    );
    server.stop()?;
    ensure!(
        massif.output_path.is_file(),
        "Massif did not publish {}",
        massif.output_path.display()
    );
    println!(
        "server highlighting Massif profile: {}",
        massif.output_path.display()
    );
    Ok(())
}

async fn present_fixture(
    clients: &ServerClients,
    fixture: &gtl_benchmarks::server_highlighting::MaterializedFixture,
) -> Result<v1::ViewerViewIdentity> {
    let response = clients
        .application
        .present_diff(PresentDiffRequest {
            working_directory: fixture.repository.to_string_lossy().into_owned(),
            target: Some(DiffTarget {
                selection: Some(diff_target::Selection::LastCommitCount(
                    fixture.identity.last_commit_count,
                )),
            }),
            name: Some(format!("server-highlighting-{}", fixture.identity.workload)),
        })
        .await
        .with_context(|| format!("present {} fixture", fixture.identity.workload))?;
    let outcome = response
        .presentation
        .and_then(|presentation| presentation.outcome)
        .context("presented fixture has no outcome")?;
    if !matches!(outcome, diff_presentation::Outcome::ViewerOpened(_)) {
        bail!("presented fixture did not open the viewer: {outcome:?}");
    }
    wait_for_ready_view(&clients.viewer, fixture.identity.file_count).await
}

async fn wait_for_ready_view(
    client: &BenchmarkViewerClient,
    expected_file_count: usize,
) -> Result<v1::ViewerViewIdentity> {
    let started = std::time::Instant::now();
    loop {
        let mut client = client.clone();
        let shell = client
            .get_viewer_shell(GetViewerShellRequest {})
            .await
            .context("get benchmark viewer shell")?
            .into_inner()
            .shell;
        if let Some(view) = shell
            .and_then(|shell| shell.active)
            .and_then(|active| active.state)
            .and_then(|state| match state {
                viewer_active_state::State::Ready(ready) => ready.view,
                viewer_active_state::State::Empty(_)
                | viewer_active_state::State::Pending(_)
                | viewer_active_state::State::Broken(_)
                | viewer_active_state::State::Error(_) => None,
            })
            .filter(|view| view.row_source() == v1::ViewerRowSourceState::Ready)
        {
            let paths = view
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            ensure!(
                view.files.len() == expected_file_count,
                "ready fixture has {} files, expected {expected_file_count}: {paths}",
                view.files.len(),
            );
            return view.identity.context("ready fixture has no identity");
        }
        ensure!(
            started.elapsed() < VIEW_READY_TIMEOUT,
            "viewer fixture did not become ready within {} seconds",
            VIEW_READY_TIMEOUT.as_secs()
        );
        tokio::time::sleep(VIEW_READY_RETRY_DELAY).await;
    }
}

fn record_fixture_identity(
    identities: &mut BTreeMap<HighlightWorkload, FixtureIdentity>,
    identity: FixtureIdentity,
) -> Result<()> {
    match identities.get(&identity.workload) {
        Some(expected) => ensure!(
            expected == &identity,
            "{} fixture identity changed between launches",
            identity.workload
        ),
        None => {
            identities.insert(identity.workload, identity);
        }
    }
    Ok(())
}

const fn residency_sequence() -> [HighlightWorkload; 3] {
    [
        HighlightWorkload::Rust,
        HighlightWorkload::Full,
        HighlightWorkload::Rust,
    ]
}

fn write_report(path: &Path, report: &ServerHighlightingReport) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("server highlighting report path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create report directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary report in {}", parent.display()))?;
    serde_json::to_writer_pretty(&mut temporary, report)
        .context("encode server highlighting report")?;
    temporary
        .write_all(b"\n")
        .context("finish benchmark report")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync benchmark report")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish benchmark report {}", path.display()))?;
    Ok(())
}
