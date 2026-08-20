fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .file_descriptor_set_path(std::env::var("OUT_DIR")? + "/gtl_descriptor.bin")
        .compile_protos(&["proto/gtl/v1/gtl.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/gtl/v1/gtl.proto");
    Ok(())
}
