//! Process-boundary values that are not application operation requests.
//! Application requests are serialized directly by their presentation layer.

pub mod daemon;
pub mod diffs;
pub mod envelope;
pub mod live_views;
pub mod projects;
pub mod recipes;
pub mod settings;
pub mod tags;
pub mod viewer;

#[allow(
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::match_single_binding
)]
pub mod v1 {
    tonic::include_proto!("gtl.v1");
}

pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("gtl_descriptor");

#[cfg(test)]
mod testing;
