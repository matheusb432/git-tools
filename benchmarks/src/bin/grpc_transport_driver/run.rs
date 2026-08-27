use std::{fs, io::Write as _, path::Path, time::Instant};

use anyhow::{Context, Result, ensure};
use gtl_benchmarks::{
    grpc_transport::{
        BENCHMARK_NAME, BenchmarkSource, GrpcTransportReport, REPORT_FORMAT_VERSION, TransportKind,
        TransportLaunch, validate_report,
    },
    server_highlighting::read_process_sample,
};

use super::{
    environment::DriverConfig,
    measurement::run_ghz,
    server::{BenchmarkServer, ServerAccess},
};

pub async fn run() -> Result<()> {
    let config = DriverConfig::from_environment()?;
    let report = measure_report(&config).await?;
    validate_report(&report).context("validate completed gRPC transport report")?;
    write_report(&config.report_path, &report)?;
    Ok(())
}

async fn measure_report(config: &DriverConfig) -> Result<GrpcTransportReport> {
    let mut transport = None;
    let mut launches = Vec::with_capacity(config.launches);
    for launch in 1..=config.launches {
        eprintln!(
            "gRPC transport launch {launch}/{}: {} requests ({} warmup)",
            config.launches,
            config.protocol.total_request_count,
            config.protocol.warmup_request_count
        );
        let started = Instant::now();
        let (launch_transport, measurement) = measure_launch(config, launch).await?;
        ensure!(
            transport.is_none_or(|expected| expected == launch_transport),
            "server transport changed between independent launches"
        );
        transport = Some(launch_transport);
        launches.push(measurement);
        eprintln!(
            "gRPC transport launch {launch}/{} completed in {:.1}s",
            config.launches,
            started.elapsed().as_secs_f64()
        );
    }
    Ok(GrpcTransportReport {
        format_version: REPORT_FORMAT_VERSION,
        benchmark: BENCHMARK_NAME.to_owned(),
        source: BenchmarkSource {
            commit: config.source_commit.clone(),
            invocation: config.invocation.clone(),
            profile: "release".to_owned(),
        },
        transport: transport.context("gRPC transport benchmark produced no launches")?,
        protocol: config.protocol.clone(),
        resource_bounds: config.resource_bounds,
        runner: config.runner.clone(),
        launches,
    })
}

async fn measure_launch(
    config: &DriverConfig,
    launch: usize,
) -> Result<(TransportKind, TransportLaunch)> {
    let temporary = tempfile::Builder::new()
        .prefix(&format!("grpc-transport-{launch}-"))
        .tempdir()
        .context("create independent gRPC transport benchmark sandbox")?;
    let mut server = BenchmarkServer::start(config, temporary.path())?;
    let measurement = measure_running_server(config, temporary.path(), launch, &mut server).await;
    let stop = server.stop();
    match (measurement, stop) {
        (Ok(measurement), Ok(())) => Ok(measurement),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error).context("stop gRPC transport benchmark server"),
        (Err(error), Err(stop_error)) => {
            Err(error).context(format!("server shutdown also failed: {stop_error:#}"))
        }
    }
}

async fn measure_running_server(
    config: &DriverConfig,
    sandbox: &Path,
    launch: usize,
    server: &mut BenchmarkServer,
) -> Result<(TransportKind, TransportLaunch)> {
    let access = server.wait_until_ready().await?;
    let before = read_process_sample(server.process_id()).context("sample server before ghz")?;
    let ghz = run_ghz(
        config,
        sandbox,
        &access.target,
        &access.capability,
        server.process_id(),
    )
    .with_context(|| format!("run ghz launch {launch}"))?;
    server.ensure_running()?;
    let after = read_process_sample(server.process_id()).context("sample server after ghz")?;
    let server_cpu_time_nanoseconds = after
        .cpu_time_nanoseconds
        .checked_sub(before.cpu_time_nanoseconds)
        .context("server scheduled CPU time moved backwards")?;
    let ServerAccess {
        transport,
        push_confirmation_required,
        ..
    } = access;
    Ok((
        transport,
        TransportLaunch {
            launch,
            server_cpu_time_nanoseconds,
            peak_server_and_client_rss_bytes: ghz.peak_server_and_client_rss_bytes,
            push_confirmation_required,
            ghz: ghz.measurement,
        },
    ))
}

fn write_report(path: &Path, report: &GrpcTransportReport) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("gRPC transport report path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create report directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary report in {}", parent.display()))?;
    serde_json::to_writer_pretty(&mut temporary, report).context("encode gRPC transport report")?;
    temporary
        .write_all(b"\n")
        .context("finish gRPC transport report")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync gRPC transport report")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish gRPC transport report {}", path.display()))?;
    Ok(())
}
