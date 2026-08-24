#[cfg(feature = "grpc")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "proto/gtl/v1/common.proto",
        "proto/gtl/v1/diff.proto",
        "proto/gtl/v1/live_view.proto",
        "proto/gtl/v1/project.proto",
        "proto/gtl/v1/repository.proto",
        "proto/gtl/v1/settings.proto",
        "proto/gtl/v1/tag.proto",
        "proto/gtl/v1/viewer.proto",
        "proto/gtl/v1/worktree.proto",
    ];
    tonic_prost_build::configure()
        .build_transport(false)
        .boxed(".gtl.v1.ViewerActiveState.state.ready")
        .file_descriptor_set_path(std::env::var("OUT_DIR")? + "/gtl_descriptor.bin")
        .compile_protos(&protos, &["proto"])?;

    for proto in protos {
        println!("cargo:rerun-if-changed={proto}");
    }
    Ok(())
}

#[cfg(not(feature = "grpc"))]
fn main() {}
