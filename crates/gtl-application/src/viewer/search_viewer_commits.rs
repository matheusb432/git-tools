use std::{cmp::Reverse, time::Instant};

use gtl_models::{
    diffs::{Commit, CommitTimeRange},
    failure::{ErrorMeta, Failure, ViewerFailure},
    git::GitHead,
    paths::RepositoryRoot,
};
use gtl_wire::viewer::{
    VIEWER_SEARCH_QUERY_MAX_BYTES, ViewerCommitSummary,
    commit_search::{
        SearchViewerCommits, VIEWER_COMMIT_SEARCH_RESULTS_MAX, ViewerCommitSearchResult,
        ViewerCommitSearchScope,
    },
};

use super::{ViewerState, source::ViewerSourceError};
use crate::ports::UserSettingsReader;

pub trait ActiveBranchCommitReader {
    /// Visits local HEAD ancestry without loading other refs or contacting remotes.
    /// Detached and unborn HEADs produce no commits. Work must stop at `deadline`.
    fn visit_commits(
        &self,
        path: &RepositoryRoot,
        deadline: Instant,
        visitor: &mut dyn FnMut(Commit),
    ) -> anyhow::Result<GitHead>;
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SearchViewerCommitsError {
    #[error(transparent)]
    #[meta(transparent)]
    Source(#[from] ViewerSourceError),
    #[error("the snapshot has no repository")]
    #[meta(failure = Failure::InvalidRequest { field: "scope".to_owned() })]
    NoRepository,
    #[error("commit search query is too long")]
    #[meta(failure = Failure::InvalidRequest { field: "query".to_owned() })]
    QueryTooLong,
    #[error("commit search exceeded its deadline")]
    #[meta(private(DeadlineExceeded))]
    DeadlineExceeded,
    #[error("commit search metadata exceeds its limit")]
    #[meta(failure = ViewerFailure::ResponseTooLarge)]
    ResponseTooLarge,
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    request: &SearchViewerCommits,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl ActiveBranchCommitReader,
) -> Result<ViewerCommitSearchResult, SearchViewerCommitsError> {
    if request.query.len() > VIEWER_SEARCH_QUERY_MAX_BYTES {
        return Err(SearchViewerCommitsError::QueryTooLong);
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    let mut matches = CommitMatches::new(&request.query, request.time_range.clone());
    let branch = match &request.scope {
        ViewerCommitSearchScope::Snapshot(identity) => {
            let snapshot = commit_source(*identity, state, settings)?;
            for commit in &snapshot.commits {
                if Instant::now() >= deadline {
                    return Err(SearchViewerCommitsError::DeadlineExceeded);
                }
                matches.visit(commit);
            }
            None
        }
        ViewerCommitSearchScope::ActiveBranchSnapshot(identity) => {
            let snapshot = commit_source(*identity, state, settings)?;
            let repository = snapshot
                .origin
                .repository()
                .ok_or(SearchViewerCommitsError::NoRepository)?;
            Some(
                git.visit_commits(&repository.root, deadline, &mut |commit| {
                    matches.visit(&commit);
                })?,
            )
        }
        ViewerCommitSearchScope::ActiveBranch(path) => {
            Some(git.visit_commits(path, deadline, &mut |commit| matches.visit(&commit))?)
        }
    };
    let result = matches.finish(branch);
    let bytes: usize = result
        .commits
        .iter()
        .map(|commit| commit.subject.len() + 128)
        .sum();
    if bytes > gtl_wire::viewer::VIEWER_COMMIT_PAGE_MAX_ENCODED_BYTES {
        return Err(SearchViewerCommitsError::ResponseTooLarge);
    }
    Ok(result)
}

struct CommitMatches {
    terms: Vec<String>,
    time_range: CommitTimeRange,
    matches: Vec<(u64, ViewerCommitSummary)>,
    total: u64,
}

impl CommitMatches {
    fn new(query: &str, time_range: CommitTimeRange) -> Self {
        Self {
            terms: query
                .to_lowercase()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            time_range,
            matches: Vec::new(),
            total: 0,
        }
    }

    fn visit(&mut self, commit: &Commit) {
        if !self.time_range.contains(&commit.committed_at) {
            return;
        }
        let subject = commit.subject.to_lowercase();
        let body = commit.body.to_lowercase();
        let id = commit.id.as_ref();
        let Some(score) = self.terms.iter().try_fold(0, |score, term| {
            let hash = (term.len() >= 4
                && term.bytes().all(|character| character.is_ascii_hexdigit())
                && id.starts_with(term))
            .then_some(4000);
            let subject = fuzzy_score(&subject, term).map(|score| score + 1000);
            let body = fuzzy_score(&body, term);
            hash.into_iter()
                .chain(subject)
                .chain(body)
                .max()
                .map(|value| score + value)
        }) else {
            return;
        };
        self.total += 1;
        let key = (
            Reverse(score),
            Reverse(&commit.committed_at),
            commit.id.as_ref(),
        );
        let position = self.matches.partition_point(|(score, existing)| {
            (
                Reverse(*score),
                Reverse(&existing.committed_at),
                existing.id.as_ref(),
            ) <= key
        });
        if position >= VIEWER_COMMIT_SEARCH_RESULTS_MAX {
            return;
        }
        self.matches.insert(
            position,
            (
                score,
                ViewerCommitSummary {
                    id: commit.id.clone(),
                    subject: commit.subject.clone(),
                    body: String::new(),
                    committed_at: commit.committed_at.clone(),
                    is_merge: commit.is_merge(),
                },
            ),
        );
        self.matches.truncate(VIEWER_COMMIT_SEARCH_RESULTS_MAX);
    }

    fn finish(self, branch: Option<GitHead>) -> ViewerCommitSearchResult {
        ViewerCommitSearchResult {
            commits: self.matches.into_iter().map(|(_, commit)| commit).collect(),
            total_matches: self.total,
            branch,
        }
    }
}

fn fuzzy_score(text: &str, term: &str) -> Option<u64> {
    if let Some(position) = text.find(term) {
        return Some(2000_u64.saturating_sub(position as u64));
    }
    let mut characters = text.chars().enumerate();
    let mut first = None;
    let mut last = 0;
    for wanted in term.chars() {
        let (position, _) = characters.find(|(_, character)| *character == wanted)?;
        first.get_or_insert(position);
        last = position;
    }
    Some(500_u64.saturating_sub((last + first.unwrap_or_default()) as u64))
}

pub(super) fn commit_source(
    identity: gtl_wire::viewer::ViewerViewIdentity,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
) -> Result<super::ViewerDiffSnapshot, ViewerSourceError> {
    let options = settings.load()?.viewer_render_options();
    super::shell::commit_source_for_identity(state, identity, options)?
        .ok_or(ViewerSourceError::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(seed: char, subject: &str, body: &str) -> Commit {
        Commit {
            id: seed.to_string().repeat(40).try_into().unwrap(),
            subject: subject.into(),
            body: body.into(),
            parents: Vec::new(),
            committed_at: "2026-09-29T12:00:00Z".try_into().unwrap(),
        }
    }

    #[test]
    fn fuzzy_search_ranks_contiguous_matches_and_combines_message_and_hash_terms() {
        let mut matches = CommitMatches::new("FIX auth", CommitTimeRange::default());
        matches.visit(&commit('a', "Fix authorization", ""));
        matches.visit(&commit('b', "Finally index x", "authentication"));
        matches.visit(&commit('c', "unrelated", ""));
        let result = matches.finish(None);
        assert_eq!(result.total_matches, 2);
        assert_eq!(result.commits[0].subject, "Fix authorization");
        let mut matches = CommitMatches::new("bbbb auth", CommitTimeRange::default());
        matches.visit(&commit('a', "Fix authorization", ""));
        matches.visit(&commit('b', "Fix authorization", ""));
        assert_eq!(matches.finish(None).commits[0].id.as_ref(), "b".repeat(40));
    }

    #[test]
    fn result_limit_keeps_best_matches_after_the_first_page() {
        let mut matches = CommitMatches::new("fx", CommitTimeRange::default());
        for _ in 0..150 {
            matches.visit(&commit('a', "Finally index", ""));
        }
        matches.visit(&commit('b', "fx", ""));
        let result = matches.finish(None);
        assert_eq!(result.total_matches, 151);
        assert_eq!(result.commits.len(), VIEWER_COMMIT_SEARCH_RESULTS_MAX);
        assert_eq!(result.commits[0].subject, "fx");
    }

    #[test]
    fn time_filter_combines_with_text_before_ranking_counting_and_limiting() {
        let from = "2026-09-29T12:00:00Z".try_into().unwrap();
        let range = CommitTimeRange::new(Some(from), None).unwrap();
        let mut matches = CommitMatches::new("fix", range.clone());
        for _ in 0..150 {
            let mut excluded = commit('a', "fix", "");
            excluded.committed_at = "2026-09-29T11:59:59Z".try_into().unwrap();
            matches.visit(&excluded);
        }
        matches.visit(&commit('b', "Fix authorization", ""));
        matches.visit(&commit('c', "unrelated", ""));
        let result = matches.finish(None);
        assert_eq!(result.total_matches, 1);
        assert_eq!(result.commits[0].subject, "Fix authorization");

        let mut matches = CommitMatches::new("", range);
        matches.visit(&commit('b', "Fix authorization", ""));
        matches.visit(&commit('c', "unrelated", ""));
        assert_eq!(matches.finish(None).total_matches, 2);
    }

    #[test]
    fn unicode_subsequences_match_without_matching_reversed_letters() {
        assert!(fuzzy_score("ação rápida", "ãrá").is_some());
        assert!(fuzzy_score("fix", "xf").is_none());
    }
}
