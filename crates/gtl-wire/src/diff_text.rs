//! Transport bounds for uploading diff text supplied outside a repository.

use std::time::Duration;

/// The largest piece of diff text one upload message carries, below the request size limit.
pub const CHUNK_BYTES_MAX: usize = 32 * 1024;

pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// Uploads the server buffers at once; each holds up to the diff text size limit in memory.
pub const UPLOADS_CONCURRENT_MAX: usize = 2;
