//! Length-framed protobuf row responses carried by raw desktop IPC.

use prost::Message as _;

use super::viewer::{ViewerCodecError, decode_stream_viewer_rows_response};
use crate::{
    v1,
    viewer::{VIEWER_PROTOCOL_VERSION, VIEWER_ROW_MAX_ENCODED_BYTES, ViewerRowStreamItem},
};

pub const ROW_IPC_BATCH_ITEMS_MAX: usize = 4;
pub const ROW_IPC_BATCH_BYTES_MAX: usize =
    8 + ROW_IPC_BATCH_ITEMS_MAX * (4 + VIEWER_ROW_MAX_ENCODED_BYTES);

/// Preserves the protobuf response without building a second native row model.
pub fn encode_frame(response: &v1::StreamViewerRowsResponse) -> Result<Vec<u8>, ViewerCodecError> {
    if response.encoded_len() > VIEWER_ROW_MAX_ENCODED_BYTES {
        return Err(ViewerCodecError::Unrepresentable);
    }
    Ok(response.encode_to_vec())
}

/// Writes protocol version, frame count, then length-prefixed protobuf frames.
pub fn encode_batch(frames: &[Vec<u8>]) -> Result<Vec<u8>, ViewerCodecError> {
    if frames.len() > ROW_IPC_BATCH_ITEMS_MAX
        || frames
            .iter()
            .any(|frame| frame.len() > VIEWER_ROW_MAX_ENCODED_BYTES)
    {
        return Err(ViewerCodecError::Unrepresentable);
    }
    let capacity = 8 + frames.iter().map(|frame| 4 + frame.len()).sum::<usize>();
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(&VIEWER_PROTOCOL_VERSION.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(frames.len())
            .map_err(|_| ViewerCodecError::Unrepresentable)?
            .to_le_bytes(),
    );
    for frame in frames {
        output.extend_from_slice(
            &u32::try_from(frame.len())
                .map_err(|_| ViewerCodecError::Unrepresentable)?
                .to_le_bytes(),
        );
        output.extend_from_slice(frame);
    }
    Ok(output)
}

/// Validates the bounded frame envelope and constructs the client row values.
pub fn decode_batch(mut bytes: &[u8]) -> Result<Vec<ViewerRowStreamItem>, ViewerCodecError> {
    if bytes.len() > ROW_IPC_BATCH_BYTES_MAX || take_u32(&mut bytes)? != VIEWER_PROTOCOL_VERSION {
        return Err(ViewerCodecError::InvalidMessage);
    }
    let count =
        usize::try_from(take_u32(&mut bytes)?).map_err(|_| ViewerCodecError::InvalidMessage)?;
    if count > ROW_IPC_BATCH_ITEMS_MAX {
        return Err(ViewerCodecError::InvalidMessage);
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let length =
            usize::try_from(take_u32(&mut bytes)?).map_err(|_| ViewerCodecError::InvalidMessage)?;
        if length > VIEWER_ROW_MAX_ENCODED_BYTES {
            return Err(ViewerCodecError::InvalidMessage);
        }
        let (frame, remaining) = bytes
            .split_at_checked(length)
            .ok_or(ViewerCodecError::InvalidMessage)?;
        bytes = remaining;
        let response = v1::StreamViewerRowsResponse::decode(frame)
            .map_err(|_| ViewerCodecError::InvalidMessage)?;
        items.push(decode_stream_viewer_rows_response(response)?);
    }
    if !bytes.is_empty() {
        return Err(ViewerCodecError::InvalidMessage);
    }
    Ok(items)
}

fn take_u32(bytes: &mut &[u8]) -> Result<u32, ViewerCodecError> {
    let (number, remaining) = bytes
        .split_at_checked(4)
        .ok_or(ViewerCodecError::InvalidMessage)?;
    *bytes = remaining;
    Ok(u32::from_le_bytes(
        number
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
    ))
}

#[cfg(test)]
mod tests;
