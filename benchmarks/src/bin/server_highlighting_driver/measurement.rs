use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use gtl_benchmarks::server_highlighting::{
    StreamMeasurement, StreamValidator, read_process_rss, read_process_sample,
};
use gtl_wire::v1::StreamViewerRowsRequest;

use super::server::BenchmarkViewerClient;

pub async fn measure_stream(
    mut client: BenchmarkViewerClient,
    request: StreamViewerRowsRequest,
    server_process_id: u32,
    rss_sample_interval: Duration,
) -> Result<StreamMeasurement> {
    let before = read_process_sample(server_process_id).context("sample server before stream")?;
    let sampler = RssSampler::start(server_process_id, rss_sample_interval)?;
    let started = Instant::now();
    let stream_result = consume_stream(&mut client, request).await;
    let wall_time = started.elapsed();
    let after = read_process_sample(server_process_id).context("sample server after stream")?;
    let observed_peak = sampler.finish()?;
    let evidence = stream_result?;
    let server_cpu_clock_ticks = after
        .cpu_clock_ticks
        .checked_sub(before.cpu_clock_ticks)
        .context("server CPU clock moved backwards")?;
    let server_cpu_time_nanoseconds = after
        .cpu_time_nanoseconds
        .checked_sub(before.cpu_time_nanoseconds)
        .context("server scheduled CPU time moved backwards")?;
    let peak_rss_bytes = observed_peak
        .max(before.memory.rss)
        .max(before.memory.high_water)
        .max(after.memory.rss)
        .max(after.memory.high_water);
    Ok(StreamMeasurement {
        wall_time_microseconds: wall_time
            .as_micros()
            .try_into()
            .context("stream wall time exceeds u64 microseconds")?,
        server_cpu_time_nanoseconds,
        server_cpu_clock_ticks,
        memory_before: before.memory,
        peak_rss_bytes,
        memory_after: after.memory,
        evidence: black_box(evidence),
    })
}

async fn consume_stream(
    client: &mut BenchmarkViewerClient,
    request: StreamViewerRowsRequest,
) -> Result<gtl_benchmarks::server_highlighting::StreamEvidence> {
    let mut stream = client
        .stream_viewer_rows(request)
        .await
        .context("start StreamViewerRows")?
        .into_inner();
    let mut validator = StreamValidator::new();
    loop {
        let Some(message) = stream
            .message()
            .await
            .context("receive StreamViewerRows response")?
        else {
            break;
        };
        validator
            .observe(&message)
            .context("validate StreamViewerRows response")?;
    }
    validator
        .finish()
        .context("finish StreamViewerRows validation")
}

struct RssSampler {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<Result<u64>>>,
}

impl RssSampler {
    fn start(process_id: u32, interval: Duration) -> Result<Self> {
        let initial = read_process_rss(process_id).context("sample initial server RSS")?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("server-highlighting-rss".to_owned())
            .spawn(move || sample_peak_rss(&thread_stop, process_id, interval, initial))
            .context("start server RSS sampler")?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    fn finish(mut self) -> Result<u64> {
        self.stop.store(true, Ordering::Release);
        self.thread
            .take()
            .context("server RSS sampler thread is missing")?
            .join()
            .map_err(|_| anyhow::anyhow!("server RSS sampler panicked"))?
    }
}

fn sample_peak_rss(
    stop: &AtomicBool,
    process_id: u32,
    interval: Duration,
    initial: gtl_benchmarks::server_highlighting::ProcessRss,
) -> Result<u64> {
    let mut peak = initial.current.max(initial.high_water);
    while !stop.load(Ordering::Acquire) {
        let sample = read_process_rss(process_id).context("sample server RSS")?;
        peak = peak.max(sample.current).max(sample.high_water);
        thread::sleep(interval);
    }
    let sample = read_process_rss(process_id).context("sample final server RSS")?;
    Ok(peak.max(sample.current).max(sample.high_water))
}

impl Drop for RssSampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
