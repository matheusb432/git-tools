#[cfg(feature = "protobuf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "proto/gtl/v1/common.proto",
        "proto/gtl/v1/diff.proto",
        "proto/gtl/v1/diff_presentation.proto",
        "proto/gtl/v1/failure.proto",
        "proto/gtl/v1/project.proto",
        "proto/gtl/v1/repository.proto",
        "proto/gtl/v1/repository_status.proto",
        "proto/gtl/v1/repository_sync.proto",
        "proto/gtl/v1/settings.proto",
        "proto/gtl/v1/tag.proto",
        "proto/gtl/v1/viewer.proto",
        "proto/gtl/v1/viewer_theme.proto",
    ];
    tonic_prost_build::configure()
        .build_transport(false)
        .build_client(cfg!(feature = "grpc"))
        .build_server(cfg!(feature = "grpc"))
        .boxed(".gtl.v1.ViewerActiveState.state.ready")
        .boxed(".gtl.v1.ViewerProjectStatusUpdate.result.status")
        .file_descriptor_set_path(std::env::var("OUT_DIR")? + "/gtl_descriptor.bin")
        .compile_protos(&protos, &["proto"])?;

    for proto in protos {
        println!("cargo:rerun-if-changed={proto}");
    }
    Ok(())
}

#[cfg(not(feature = "protobuf"))]
fn main() {}
