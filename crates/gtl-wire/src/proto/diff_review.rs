use std::path::PathBuf;

use gtl_models::{
    diffs::DiffReviewContentId,
    paths::{RepositoryRelativePath, RepositoryRoot},
};

use crate::{
    diff_review::{DiffFileReview, DiffFileReviewReference, SetDiffFileReviewed},
    proto::viewer::ViewerCodecError,
    v1,
};

#[must_use]
pub fn encode_reference(file: &DiffFileReviewReference) -> v1::DiffFileReviewReference {
    v1::DiffFileReviewReference {
        repository_root: file.repository.to_string_lossy().into_owned(),
        file_path: file.path.to_string_lossy().into_owned(),
        content_id: file.content_id.into_digest().to_vec(),
    }
}

pub fn decode_reference(
    file: v1::DiffFileReviewReference,
) -> Result<DiffFileReviewReference, ViewerCodecError> {
    Ok(DiffFileReviewReference {
        repository: RepositoryRoot::try_new(PathBuf::from(file.repository_root))
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        path: RepositoryRelativePath::try_new(PathBuf::from(file.file_path))
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        content_id: DiffReviewContentId::from_digest(
            file.content_id
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
        ),
    })
}

#[must_use]
pub fn encode_review(review: &DiffFileReview) -> v1::DiffFileReview {
    v1::DiffFileReview {
        reference: Some(encode_reference(&review.reference)),
        reviewed: review.reviewed,
    }
}

pub fn decode_review(review: v1::DiffFileReview) -> Result<DiffFileReview, ViewerCodecError> {
    Ok(DiffFileReview {
        reference: decode_reference(review.reference.ok_or(ViewerCodecError::InvalidMessage)?)?,
        reviewed: review.reviewed,
    })
}

#[must_use]
pub fn encode_set(request: &SetDiffFileReviewed) -> v1::SetDiffFileReviewedRequest {
    v1::SetDiffFileReviewedRequest {
        file: Some(encode_reference(&request.file)),
        reviewed: Some(request.reviewed),
    }
}

pub fn decode_set(
    request: v1::SetDiffFileReviewedRequest,
) -> Result<SetDiffFileReviewed, ViewerCodecError> {
    Ok(SetDiffFileReviewed {
        file: decode_reference(request.file.ok_or(ViewerCodecError::InvalidMessage)?)?,
        reviewed: request.reviewed.ok_or(ViewerCodecError::InvalidMessage)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_commands_preserve_false_and_reject_missing_or_invalid_identity() {
        let file = v1::DiffFileReviewReference {
            repository_root: if cfg!(windows) { "C:/repo" } else { "/repo" }.into(),
            file_path: "src/a.rs".into(),
            content_id: vec![1; 32],
        };
        let command = v1::SetDiffFileReviewedRequest {
            file: Some(file.clone()),
            reviewed: Some(false),
        };
        assert_eq!(encode_set(&decode_set(command.clone()).unwrap()), command);
        assert!(
            decode_set(v1::SetDiffFileReviewedRequest {
                reviewed: None,
                ..command.clone()
            })
            .is_err()
        );
        assert!(
            decode_set(v1::SetDiffFileReviewedRequest {
                file: None,
                ..command
            })
            .is_err()
        );
        assert!(
            decode_reference(v1::DiffFileReviewReference {
                content_id: vec![1; 31],
                ..file.clone()
            })
            .is_err()
        );
        assert!(
            decode_reference(v1::DiffFileReviewReference {
                file_path: "../escape".into(),
                ..file
            })
            .is_err()
        );
    }
}
