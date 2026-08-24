use std::{future::Future, time::Duration};

use anyhow::Context as _;
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
use tonic::{
    Request, Status,
    codegen::http::{
        HeaderValue, Method,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName},
    },
    server::NamedService,
    service::{Interceptor, InterceptorLayer, LayerExt as _},
    transport::{Server, server::TcpIncoming},
};
use tower_http::{
    LatencyUnit,
    cors::{AllowOrigin, CorsLayer},
    trace::{
        DefaultMakeSpan, DefaultOnBodyChunk, DefaultOnEos, DefaultOnFailure, DefaultOnResponse,
        GrpcMakeClassifier, TraceLayer,
    },
};

use crate::{
    services::{
        DiffApi, LiveViewApi, ProjectApi, RepositoryApi, SettingsApi, TagApi, ViewerApi,
        WorktreeApi,
    },
    state::AppState,
};

const MAX_CONCURRENT_REQUESTS_PER_CONNECTION: usize = 16;
const MAX_REQUEST_MESSAGE_SIZE: usize = 64 * 1024;
const MAX_RESPONSE_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
const VIEWER_MAX_RESPONSE_MESSAGE_SIZE: usize = VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1024;

const AUTHORIZATION_METADATA_KEY: &str = "authorization";
const AUTHORIZATION_SCHEME: &str = "Bearer ";

type GrpcTraceLayer = TraceLayer<
    GrpcMakeClassifier,
    DefaultMakeSpan,
    (),
    DefaultOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    DefaultOnFailure,
>;

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
    listener: tokio::net::TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
    shutdown_grace_period: Duration,
    capability: CapabilityToken,
    viewer_capability: CapabilityToken,
    state: AppState,
) -> anyhow::Result<()> {
    let (health_reporter, health_server) = tonic_health::server::health_reporter();
    let application_service_names = [
        DiffServiceServer::<DiffApi>::NAME,
        LiveViewServiceServer::<LiveViewApi>::NAME,
        ProjectServiceServer::<ProjectApi>::NAME,
        RepositoryServiceServer::<RepositoryApi>::NAME,
        SettingsServiceServer::<SettingsApi>::NAME,
        TagServiceServer::<TagApi>::NAME,
        ViewerServiceServer::<ViewerApi>::NAME,
        WorktreeServiceServer::<WorktreeApi>::NAME,
    ];
    for service_name in application_service_names {
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
    let diff_server = DiffServiceServer::new(DiffApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let live_view_server = LiveViewServiceServer::new(LiveViewApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let project_server = ProjectServiceServer::new(ProjectApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let repository_server = RepositoryServiceServer::new(RepositoryApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let settings_server = SettingsServiceServer::new(SettingsApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let tag_server = TagServiceServer::new(TagApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let worktree_server = WorktreeServiceServer::new(WorktreeApi::new(state.clone()))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_RESPONSE_MESSAGE_SIZE);
    let viewer_server = ViewerServiceServer::new(ViewerApi::new(state))
        .max_decoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
        .max_encoding_message_size(VIEWER_MAX_RESPONSE_MESSAGE_SIZE);
    let reflection_server = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
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
    let viewer_server = InterceptorLayer::new(Authentication {
        capability: viewer_capability,
    })
    .named_layer(viewer_server);
    let viewer_server = tonic_web::GrpcWebLayer::new().named_layer(viewer_server);
    let viewer_server = viewer_cors_layer().named_layer(viewer_server);
    let (shutdown_started_sender, shutdown_started_receiver) = tokio::sync::oneshot::channel();
    let shutdown = async move {
        shutdown.await;
        for service_name in application_service_names {
            health_reporter
                .set_service_status(service_name, tonic_health::ServingStatus::NotServing)
                .await;
        }
        health_reporter
            .set_service_status("", tonic_health::ServingStatus::NotServing)
            .await;
        let _ = shutdown_started_sender.send(());
    };
    let grpc_server = Server::builder()
        .accept_http1(true)
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
        .add_service(worktree_server)
        .serve_with_incoming_shutdown(TcpIncoming::from(listener), shutdown);
    tokio::pin!(grpc_server);

    let result = tokio::select! {
        result = &mut grpc_server => result,
        _ = shutdown_started_receiver => {
            let Ok(result) = tokio::time::timeout(shutdown_grace_period, &mut grpc_server).await else {
                tracing::warn!(
                    shutdown_grace_period = ?shutdown_grace_period,
                    "gRPC connections exceeded the shutdown grace period"
                );
                return Ok(());
            };
            result
        }
    };

    result.context("gRPC server failure")
}

fn viewer_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("http://tauri.localhost"),
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://127.0.0.1:8080"),
        ]))
        .allow_methods([Method::POST])
        .allow_headers([
            ACCEPT,
            AUTHORIZATION,
            CONTENT_TYPE,
            HeaderName::from_static("grpc-timeout"),
            HeaderName::from_static("x-grpc-web"),
            HeaderName::from_static("x-user-agent"),
        ])
        .expose_headers([
            HeaderName::from_static("grpc-message"),
            HeaderName::from_static("grpc-status"),
            HeaderName::from_static("grpc-status-details-bin"),
        ])
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
