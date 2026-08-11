use std::io::Write as _;

use anyhow::{Context as _, Result};
use flate2::{Compression, GzBuilder};

pub(crate) fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), Compression::best());
    encoder
        .write_all(bytes)
        .context("write deterministic gzip stream")?;
    encoder.finish().context("finish deterministic gzip stream")
}

#[cfg(test)]
pub(crate) fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read as _;

    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut decoded)
        .context("decode gzip stream")?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_is_deterministic_and_round_trips() {
        let input = b"same artifact bytes across invocations";
        let first = gzip(input).expect("compress fixture");
        let second = gzip(input).expect("compress fixture again");

        assert_eq!(first, second);
        assert_eq!(&first[..3], &[0x1f, 0x8b, 0x08]);
        assert_eq!(gunzip(&first).expect("decompress fixture"), input);
    }
}
