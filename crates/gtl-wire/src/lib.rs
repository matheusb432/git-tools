//! Process-boundary values that are not application operation requests.
//! Application requests are serialized directly by their presentation layer.

pub mod recipes;
pub mod settings;
pub mod viewer;

#[allow(
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::match_single_binding,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref
)]
#[cfg(feature = "grpc")]
pub mod v1 {
    tonic::include_proto!("gtl.v1");
}

#[cfg(feature = "grpc")]
pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("gtl_descriptor");

#[cfg(test)]
mod testing;
