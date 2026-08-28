use std::{future::Future, time::Duration};

use anyhow::{Context as _, bail};
use gtl_local_auth::CapabilityToken;
use gtl_wire::{
    FILE_DESCRIPTOR_SET,
    v1::{
        diff_service_server::DiffServiceServer, live_view_service_server::LiveViewServiceServer,
        project_service_server::ProjectServiceServer,
        repository_service_server::RepositoryServiceServer,
        settings_service_server::SettingsServiceServer, tag_service_server::TagServiceServer,
        viewer_service_server::ViewerServiceServer, worktree_service_server::WorktreeServiceServer,
    },
    viewer::VIEWER_ROW_MAX_ENCODED_BYTES,
};
#[cfg(windows)]
use tonic::transport::server::TcpIncoming;
use tonic::{
    Request, Status,
    server::NamedService,
    service::{Interceptor, InterceptorLayer, LayerExt as _},
    transport::Server,
};
use tonic_health::server::HealthReporter;
use tower_http::{
    LatencyUnit,
    trace::{
        DefaultMakeSpan, DefaultOnBodyChunk, DefaultOnEos, DefaultOnFailure, DefaultOnResponse,
        GrpcMakeClassifier, TraceLayer,
    },
};

#[cfg(unix)]
use crate::uds_listener::BoundUdsListener;
use crate::{
    services::{
        DiffGrpcService, LiveViewGrpcService, ProjectGrpcService, RepositoryGrpcService,
        SettingsGrpcService, TagGrpcService, ViewerGrpcService, WorktreeGrpcService,
    },
    state::AppState,
};

const MAX_CONCURRENT_REQUESTS_PER_CONNECTION: usize = 32;
const MAX_REQUEST_MESSAGE_SIZE: usize = 64 * 1024;
const MAX_RESPONSE_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
const VIEWER_MAX_RESPONSE_MESSAGE_SIZE: usize = VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1024;

const AUTHORIZATION_METADATA_KEY: &str = "authorization";
const AUTHORIZATION_SCHEME: &str = "Bearer ";
const HEALTH_SERVICE_NAME: &str = "grpc.health.v1.Health";
const REFLECTION_SERVICE_NAME: &str = "grpc.reflection.v1.ServerReflection";
const NATIVE_APPLICATION_SERVICE_NAMES: [&str; 8] = [
    DiffServiceServer::<DiffGrpcService>::NAME,
    LiveViewServiceServer::<LiveViewGrpcService>::NAME,
    ProjectServiceServer::<ProjectGrpcService>::NAME,
    RepositoryServiceServer::<RepositoryGrpcService>::NAME,
    SettingsServiceServer::<SettingsGrpcService>::NAME,
    TagServiceServer::<TagGrpcService>::NAME,
    ViewerServiceServer::<ViewerGrpcService>::NAME,
    WorktreeServiceServer::<WorktreeGrpcService>::NAME,
];

type GrpcTraceLayer = TraceLayer<
    GrpcMakeClassifier,
    DefaultMakeSpan,
    (),
    DefaultOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    DefaultOnFailure,
>;

#[cfg(unix)]
pub(crate) struct ServerListeners {
    native: BoundUdsListener,
}

#[cfg(unix)]
impl ServerListeners {
    pub(crate) const fn new(native: BoundUdsListener) -> Self {
        Self { native }
    }
}

#[cfg(windows)]
pub(crate) struct ServerListeners {
    combined: tokio::net::TcpListener,
}

#[cfg(windows)]
impl ServerListeners {
    pub(crate) const fn new(combined: tokio::net::TcpListener) -> Self {
        Self { combined }
    }
}

#[cfg(windows)]
fn tcp_incoming(listener: tokio::net::TcpListener) -> TcpIncoming {
    TcpIncoming::from(listener).with_nodelay(Some(true))
}

#[derive(Clone)]
struct Authentication {
    capability: CapabilityToken,
}

impl std::fmt::Debug for Authentication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Authentication(REDACTED)")
    }
}

impl Interceptor for Authentication {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let authenticated = request
            .metadata()
            .get(AUTHORIZATION_METADATA_KEY)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix(AUTHORIZATION_SCHEME))
            .is_some_and(|candidate| self.capability.authenticates(candidate));
        if !authenticated {
            return Err(Status::unauthenticated("authentication required"));
        }
        Ok(request)
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn serve(
    listeners: ServerListeners,
    shutdown: impl Future<Output = ()> + Send + 'static,
    shutdown_grace_period: Duration,
    capability: CapabilityToken,
    state: AppState,
) -> anyhow::Result<()> {
    let (health_reporter, health_server) = tonic_health::server::health_reporter();
    for service_name in NATIVE_APPLICATION_SERVICE_NAMES {
        health_reporter
            .set_service_status(service_name, tonic_health::ServingStatus::Serving)
            .await;
    }
    health_reporter
        .set_service_status("", tonic_health::ServingStatus::Serving)
        .await;
    let health_server = health_server
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let diff_server = DiffServiceServer::new(DiffGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let live_view_server = LiveViewServiceServer::new(LiveViewGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let project_server = ProjectServiceServer::new(ProjectGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let repository_server = RepositoryServiceServer::new(RepositoryGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let settings_server = SettingsServiceServer::new(SettingsGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let tag_server = TagServiceServer::new(TagGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let worktree_server = WorktreeServiceServer::new(WorktreeGrpcService::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let viewer_server = ViewerServiceServer::new(ViewerGrpcService::new(state))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(VIEWER_MAX_RESPONSE_MESSAGE_SIZE);
    let mut reflection_builder = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET);
    for service_name in NATIVE_APPLICATION_SERVICE_NAMES {
        reflection_builder = reflection_builder.with_service_name(service_name);
    }
    let reflection_server = reflection_builder
        .with_service_name(HEALTH_SERVICE_NAME)
        .with_service_name(REFLECTION_SERVICE_NAME)
        .build_v1()
        .context("building the gRPC reflection service")?
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let native_authentication = || Authentication {
        capability: capability.clone(),
    };
    let health_server = InterceptorLayer::new(native_authentication()).named_layer(health_server);
    let reflection_server =
        InterceptorLayer::new(native_authentication()).named_layer(reflection_server);
    let diff_server = InterceptorLayer::new(native_authentication()).named_layer(diff_server);
    let live_view_server =
        InterceptorLayer::new(native_authentication()).named_layer(live_view_server);
    let project_server = InterceptorLayer::new(native_authentication()).named_layer(project_server);
    let repository_server =
        InterceptorLayer::new(native_authentication()).named_layer(repository_server);
    let settings_server =
        InterceptorLayer::new(native_authentication()).named_layer(settings_server);
    let tag_server = InterceptorLayer::new(native_authentication()).named_layer(tag_server);
    let worktree_server =
        InterceptorLayer::new(native_authentication()).named_layer(worktree_server);
    let viewer_server = InterceptorLayer::new(native_authentication()).named_layer(viewer_server);
    let (shutdown_sender, shutdown_receiver) = tokio::sync::watch::channel(false);
    let server_shutdown = wait_for_shutdown(shutdown_receiver);

    let server = Server::builder()
        .layer(grpc_trace_layer())
        .concurrency_limit_per_connection(MAX_CONCURRENT_REQUESTS_PER_CONNECTION)
        .load_shed(true)
        .add_service(health_server)
        .add_service(reflection_server)
        .add_service(diff_server)
        .add_service(live_view_server)
        .add_service(project_server)
        .add_service(repository_server)
        .add_service(settings_server)
        .add_service(tag_server)
        .add_service(viewer_server)
        .add_service(worktree_server);
    #[cfg(unix)]
    let grpc_server = server.serve_with_incoming_shutdown(listeners.native, server_shutdown);
    #[cfg(windows)]
    let grpc_server =
        server.serve_with_incoming_shutdown(tcp_incoming(listeners.combined), server_shutdown);
    supervise_server(
        grpc_server,
        shutdown,
        shutdown_sender,
        health_reporter,
        shutdown_grace_period,
    )
    .await
}

async fn wait_for_shutdown(mut receiver: tokio::sync::watch::Receiver<bool>) {
    while !*receiver.borrow() {
        if receiver.changed().await.is_err() {
            break;
        }
    }
}

async fn signal_shutdown(
    health_reporter: &HealthReporter,
    shutdown_sender: &tokio::sync::watch::Sender<bool>,
) {
    for service_name in NATIVE_APPLICATION_SERVICE_NAMES {
        health_reporter
            .set_service_status(service_name, tonic_health::ServingStatus::NotServing)
            .await;
    }
    health_reporter
        .set_service_status("", tonic_health::ServingStatus::NotServing)
        .await;
    let _ = shutdown_sender.send(true);
}

async fn supervise_server<Grpc, Shutdown>(
    grpc_server: Grpc,
    shutdown: Shutdown,
    shutdown_sender: tokio::sync::watch::Sender<bool>,
    health_reporter: HealthReporter,
    shutdown_grace_period: Duration,
) -> anyhow::Result<()>
where
    Grpc: Future<Output = Result<(), tonic::transport::Error>>,
    Shutdown: Future<Output = ()>,
{
    tokio::pin!(grpc_server);
    tokio::pin!(shutdown);

    tokio::select! {
        () = &mut shutdown => {
            signal_shutdown(&health_reporter, &shutdown_sender).await;
            let Ok(result) =
                tokio::time::timeout(shutdown_grace_period, &mut grpc_server).await
            else {
                tracing::warn!(
                    shutdown_grace_period = ?shutdown_grace_period,
                    "gRPC connections exceeded the shutdown grace period"
                );
                return Ok(());
            };
            result.context("gRPC server failure during shutdown")
        }
        result = &mut grpc_server => match result {
            Err(error) => Err(error).context("gRPC server failure"),
            Ok(()) => bail!("gRPC server stopped unexpectedly"),
        },
    }
}

fn grpc_trace_layer() -> GrpcTraceLayer {
    TraceLayer::new_for_grpc()
        .make_span_with(
            DefaultMakeSpan::new()
                .level(tracing::Level::INFO)
                .include_headers(false),
        )
        .on_request(())
        .on_response(
            DefaultOnResponse::new()
                .level(tracing::Level::INFO)
                .latency_unit(LatencyUnit::Micros)
                .include_headers(false),
        )
        .on_eos(
            DefaultOnEos::new()
                .level(tracing::Level::INFO)
                .latency_unit(LatencyUnit::Micros),
        )
        .on_failure(
            DefaultOnFailure::new()
                .level(tracing::Level::WARN)
                .latency_unit(LatencyUnit::Micros),
        )
}

#[cfg(test)]
mod tests;
