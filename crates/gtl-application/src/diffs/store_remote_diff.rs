use gtl_models::{
    diffs::{DIFF_TEXT_BYTES_MAX, DiffText},
    failure::{DiffTextFailure, ErrorMeta, ExternalDiagnostic, RemoteDiffFailure},
};
use rusqlite::{Connection, params};

use super::remote_diff::{GitHubApiResponse, RemoteDiffKey, StoredRemoteDiff};

/// Cached remote diffs beyond this count are evicted, least recently used first.
const CACHED_REMOTE_DIFFS_MAX: i64 = 64;

/// GitHub's answer to the request `resolve_remote_diff` named for `origin` and `range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreRemoteDiff {
    pub origin: String,
    pub range: String,
    pub response: GitHubApiResponse,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum StoreRemoteDiffError {
    #[error(transparent)]
    #[meta(failure)]
    Remote(#[from] RemoteDiffFailure),
    #[error(transparent)]
    #[meta(failure)]
    Invalid(#[from] DiffTextFailure),
    #[error(transparent)]
    #[meta(private(Internal))]
    Persistence(#[from] rusqlite::Error),
}

/// Validates the compared diff GitHub returned and caches it as the most recently used diff for
/// its repository and range.
#[cqrsy::command]
pub fn execute(
    request: StoreRemoteDiff,
    connection: &mut Connection,
) -> Result<StoredRemoteDiff, StoreRemoteDiffError> {
    let key = RemoteDiffKey::parse(&request.origin, &request.range)?;
    let text = compare_diff_text(request.response)?;
    super::text_diff::parse_files(&text)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO github_compare_diffs (repository, base, head, text_id, used_order)
         VALUES (?1, ?2, ?3, ?4,
                 (SELECT coalesce(max(used_order), 0) + 1 FROM github_compare_diffs))
         ON CONFLICT (repository, base, head)
         DO UPDATE SET text_id = excluded.text_id, used_order = excluded.used_order",
        params![
            key.repository.to_string(),
            key.range.base.as_str(),
            key.range.head.as_str(),
            text.id().as_ref(),
        ],
    )?;
    transaction.execute(
        "DELETE FROM github_compare_diffs WHERE id IN (
             SELECT id FROM github_compare_diffs
             ORDER BY used_order DESC
             LIMIT -1 OFFSET ?1)",
        params![CACHED_REMOTE_DIFFS_MAX],
    )?;
    super::store_diff_text::insert_and_prune(&text, &transaction)?;
    transaction.commit()?;
    Ok(StoredRemoteDiff {
        id: text.id().clone(),
        label: key.label(),
    })
}

fn compare_diff_text(response: GitHubApiResponse) -> Result<DiffText, StoreRemoteDiffError> {
    let GitHubApiResponse {
        status,
        rate_limit_remaining,
        body,
    } = response;
    let failure = match status {
        200..=299 => {
            let text = String::from_utf8(body)
                .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
            return DiffText::try_new(text).map_err(|_| {
                DiffTextFailure::TooLarge {
                    bytes_max: DIFF_TEXT_BYTES_MAX as u64,
                }
                .into()
            });
        }
        401 => RemoteDiffFailure::Unauthenticated,
        404 => RemoteDiffFailure::NotFound,
        429 => RemoteDiffFailure::RateLimited,
        403 if rate_limit_remaining == Some(0) => RemoteDiffFailure::RateLimited,
        status => RemoteDiffFailure::Rejected {
            status,
            diagnostic: github_message(&body),
        },
    };
    Err(failure.into())
}

#[derive(serde::Deserialize)]
struct GitHubErrorBody {
    message: String,
}

fn github_message(body: &[u8]) -> ExternalDiagnostic {
    serde_json::from_slice::<GitHubErrorBody>(body).map_or_else(
        |_| ExternalDiagnostic::new(&String::from_utf8_lossy(body)),
        |error| ExternalDiagnostic::new(&error.message),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, rate_limit_remaining: Option<u64>, body: &str) -> GitHubApiResponse {
        GitHubApiResponse {
            status,
            rate_limit_remaining,
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn successful_responses_become_diff_text_with_invalid_bytes_replaced() {
        let mut body = b"diff --git a/x b/x\n+caf".to_vec();
        body.push(0xe9);

        let text = compare_diff_text(GitHubApiResponse {
            status: 200,
            rate_limit_remaining: Some(4999),
            body,
        })
        .unwrap();

        assert_eq!(text.as_str(), "diff --git a/x b/x\n+caf\u{fffd}");
    }

    #[test]
    fn refusals_map_to_typed_failures() {
        for (response, expected) in [
            (
                response(401, Some(60), "{}"),
                RemoteDiffFailure::Unauthenticated,
            ),
            (response(404, Some(4999), "{}"), RemoteDiffFailure::NotFound),
            (response(429, None, "{}"), RemoteDiffFailure::RateLimited),
            (
                response(403, Some(0), r#"{"message":"API rate limit exceeded"}"#),
                RemoteDiffFailure::RateLimited,
            ),
            (
                response(
                    403,
                    Some(4999),
                    r#"{"message":"Resource protected by SSO"}"#,
                ),
                RemoteDiffFailure::Rejected {
                    status: 403,
                    diagnostic: ExternalDiagnostic::new("Resource protected by SSO"),
                },
            ),
            (
                response(502, None, "Bad gateway"),
                RemoteDiffFailure::Rejected {
                    status: 502,
                    diagnostic: ExternalDiagnostic::new("Bad gateway"),
                },
            ),
        ] {
            let error = compare_diff_text(response).unwrap_err();

            assert!(
                matches!(&error, StoreRemoteDiffError::Remote(failure) if *failure == expected),
                "{error:?}"
            );
        }
    }
}
