use std::{collections::BTreeMap, path::Path};

use crate::commands::squash_local::GitRunner;

const LOCAL_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=2)\t%(contents:lines=1)";
const REMOTE_TAG_FORMAT_ARG: &str = "--format=%(objectname)\t%(*objectname)\t%(*objectname:short)\t%(refname:strip=4)\t%(contents:lines=1)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Created,
    Listed,
    Noop,
    Pushed,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagResult {
    pub status: Status,
    pub detail: String,
}

impl TagResult {
    fn new(status: Status, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }
}

pub fn list(runner: &impl GitRunner, repo: &Path, commits: bool) -> TagResult {
    let refs = match tag_refs(runner, repo) {
        Ok(refs) => refs,
        Err(detail) => return TagResult::new(Status::Fail, detail),
    };

    let mut lines = Vec::new();
    for group in group_by_commit(refs.local.values()) {
        match group.as_slice() {
            [single] => lines.push(render_tag(&refs, single, commits)),
            many => match classify_group(many) {
                TagGroup::Canonical(canonical) => {
                    lines.push(render_tag(&refs, canonical, commits));
                    for label in many.iter().filter(|tag| tag.name != canonical.name) {
                        lines.push(render_label(&refs, label));
                    }
                }
                TagGroup::MoreThanOneTagHasMessage | TagGroup::AllLabels => {
                    lines.push(render_commit_header(many[0]));
                    for tag in many {
                        lines.push(render_label(&refs, tag));
                    }
                }
            },
        }
    }

    TagResult::new(Status::Listed, lines.join("\n"))
}

/// How the tags sharing one commit relate: which (if any) is the canonical,
/// message-bearing tag the others nest under as lightweight labels.
#[derive(Debug, PartialEq, Eq)]
enum TagGroup<'a> {
    /// Exactly one tag carries a message; the rest nest under it as labels.
    Canonical(&'a TagRef),
    /// More than one tag carries a message, so no single canonical tag exists —
    /// nest them all under the commit (pseudocode's `MoreThanOneTagHasMessage`).
    MoreThanOneTagHasMessage,
    /// No tag carries a message; every tag is a lightweight label under the commit.
    AllLabels,
}

/// Classifies a group of two-or-more tags pointing at one commit. Only one tag
/// may carry a message for the others to read as its labels.
fn classify_group<'a>(group: &[&'a TagRef]) -> TagGroup<'a> {
    let mut with_message = group.iter().copied().filter(|tag| tag.has_message());
    match (with_message.next(), with_message.next()) {
        (Some(canonical), None) => TagGroup::Canonical(canonical),
        (Some(_), Some(_)) => TagGroup::MoreThanOneTagHasMessage,
        (None, _) => TagGroup::AllLabels,
    }
}

/// Buckets tags by the commit they resolve to, preserving first-seen order.
fn group_by_commit<'a>(tags: impl Iterator<Item = &'a TagRef>) -> Vec<Vec<&'a TagRef>> {
    let mut groups: Vec<Vec<&'a TagRef>> = Vec::new();
    for tag in tags {
        match groups
            .iter_mut()
            .find(|group| group[0].commit_id() == tag.commit_id())
        {
            Some(group) => group.push(tag),
            None => groups.push(vec![tag]),
        }
    }
    groups
}

fn ref_state(refs: &TagRefs, tag: &TagRef) -> &'static str {
    if refs.is_remote(tag) {
        "remote"
    } else {
        "local"
    }
}

fn render_tag(refs: &TagRefs, tag: &TagRef, commits: bool) -> String {
    let state = ref_state(refs, tag);
    let message = message_suffix(tag);
    if commits {
        format!("{} {} [{state}]{message}", tag.commit_short(), tag.name)
    } else {
        format!("{} [{state}]{message}", tag.name)
    }
}

fn render_label(refs: &TagRefs, tag: &TagRef) -> String {
    format!(
        "  - {} [{}]{}",
        tag.name,
        ref_state(refs, tag),
        message_suffix(tag)
    )
}

/// Trailing `  <first message line>` for a message-bearing (annotated) tag, or
/// empty for a lightweight tag — its underlying commit subject is not its own.
fn message_suffix(tag: &TagRef) -> String {
    match tag.message_subject() {
        Some(subject) => format!("  {subject}"),
        None => String::new(),
    }
}

fn render_commit_header(tag: &TagRef) -> String {
    tag.commit_short()
}

pub fn add(runner: &impl GitRunner, repo: &Path, tag: &str, message: &str) -> TagResult {
    if tag.trim().is_empty() {
        return TagResult::new(Status::Fail, "tag name is required");
    }
    if message.trim().is_empty() {
        return TagResult::new(Status::Fail, "tag message is required");
    }

    match runner.run(repo, &["tag", "-a", tag, "-m", message]) {
        Ok(output) if output.exit_code == 0 => {
            TagResult::new(Status::Created, format!("created tag {tag}"))
        }
        Ok(output) => TagResult::new(
            Status::Fail,
            output.fail_detail(&format!("git tag add failed for {tag}")),
        ),
        Err(error) => TagResult::new(Status::Fail, error.to_string()),
    }
}

pub fn add_and_push(
    runner: &impl GitRunner,
    repo: &Path,
    tag: &str,
    message: &str,
    label: Option<&str>,
) -> TagResult {
    let created = add(runner, repo, tag, message);
    if created.status == Status::Fail {
        return created;
    }

    let mut names = vec![tag];
    let mut detail = created.detail;
    if let Some(label) = label {
        match create_label_tag(runner, repo, tag, label) {
            Ok(()) => {
                names.push(label);
                detail = format!("{detail}\ncreated tag {label}");
            }
            Err(failure) => return TagResult::new(Status::Fail, failure),
        }
    }

    push_created(runner, repo, &names, detail)
}

/// Attach a lightweight label tag to an existing tag's commit, then push it.
///
/// Backs `tag update <tag> -l <label>`: the canonical (message-bearing) tag
/// already exists, so this only adds the second ref and pushes it.
pub fn label_tag(runner: &impl GitRunner, repo: &Path, tag: &str, label: &str) -> TagResult {
    if tag.trim().is_empty() {
        return TagResult::new(Status::Fail, "tag name is required");
    }
    if label.trim().is_empty() {
        return TagResult::new(Status::Fail, "label is required");
    }

    if let Err(failure) = create_label_tag(runner, repo, tag, label) {
        return TagResult::new(Status::Fail, failure);
    }

    push_created(runner, repo, &[label], format!("created tag {label}"))
}

/// Creates a lightweight label tag pointing at `target`'s commit
/// (`git tag <label> <target>^{}` — two refs to one commit, no duplicated message).
fn create_label_tag(
    runner: &impl GitRunner,
    repo: &Path,
    target: &str,
    label: &str,
) -> Result<(), String> {
    let peeled = format!("{target}^{{}}");
    match runner.run(repo, &["tag", label, &peeled]) {
        Ok(output) if output.exit_code == 0 => Ok(()),
        Ok(output) => Err(output.fail_detail(&format!("git tag label failed for {label}"))),
        Err(error) => Err(error.to_string()),
    }
}

/// Pushes the just-created tags named in `names` that are not yet on origin,
/// prefixing the push line with `created_detail`.
fn push_created(
    runner: &impl GitRunner,
    repo: &Path,
    names: &[&str],
    created_detail: String,
) -> TagResult {
    let refs = match tag_refs(runner, repo) {
        Ok(refs) => refs,
        Err(detail) => return TagResult::new(Status::Fail, detail),
    };

    let mut pending = Vec::new();
    for name in names {
        let Some(tag_ref) = refs.local.get(*name) else {
            return TagResult::new(Status::Fail, format!("created tag {name} was not found"));
        };
        if !refs.is_remote(tag_ref) {
            pending.push(tag_ref);
        }
    }

    if pending.is_empty() {
        return TagResult::new(Status::Noop, created_detail);
    }

    let pushed = push_tags(runner, repo, &pending);
    match pushed.status {
        Status::Pushed => TagResult::new(
            Status::Pushed,
            format!("{created_detail}\n{}", pushed.detail),
        ),
        Status::Noop => TagResult::new(Status::Noop, created_detail),
        Status::Fail => pushed,
        Status::Created | Status::Listed => pushed,
    }
}

pub fn push(runner: &impl GitRunner, repo: &Path) -> TagResult {
    let refs = match tag_refs(runner, repo) {
        Ok(refs) => refs,
        Err(detail) => return TagResult::new(Status::Fail, detail),
    };

    let pending = refs.pending();
    if pending.is_empty() {
        return TagResult::new(Status::Noop, "tags already up to date");
    }

    push_tags(runner, repo, &pending)
}

fn push_tags(runner: &impl GitRunner, repo: &Path, pending: &[&TagRef]) -> TagResult {
    let mut args = vec!["push".to_string(), "origin".to_string()];
    args.extend(
        pending
            .iter()
            .map(|tag| format!("refs/tags/{0}:refs/tags/{0}", tag.name)),
    );
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

    match runner.run(repo, &arg_refs) {
        Ok(output) if output.exit_code == 0 => {
            for tag in pending {
                if let Err(detail) = track_pushed_tag(runner, repo, tag) {
                    return TagResult::new(Status::Fail, detail);
                }
            }
            let names = pending
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let noun = if pending.len() == 1 { "tag" } else { "tags" };
            TagResult::new(
                Status::Pushed,
                format!("pushed {} {noun}: {names}", pending.len()),
            )
        }
        Ok(output) => TagResult::new(Status::Fail, output.fail_detail("git push tags failed")),
        Err(error) => TagResult::new(Status::Fail, error.to_string()),
    }
}

fn track_pushed_tag(runner: &impl GitRunner, repo: &Path, tag: &TagRef) -> Result<(), String> {
    let remote_ref = format!("refs/remotes/origin/tags/{}", tag.name);
    match runner.run(repo, &["update-ref", &remote_ref, &tag.object]) {
        Ok(output) if output.exit_code == 0 => Ok(()),
        Ok(output) => Err(output.fail_detail(&format!("git update-ref failed for {}", tag.name))),
        Err(error) => Err(error.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TagRef {
    object: String,
    commit: String,
    peeled_short: String,
    name: String,
    /// First line of the annotated tag's message (`%(contents:lines=1)`). For a
    /// lightweight tag this is the underlying commit's subject, so it is only
    /// surfaced for message-bearing tags (see [`TagRef::message_subject`]).
    message: String,
}

impl TagRef {
    fn commit_short(&self) -> String {
        if self.peeled_short.is_empty() {
            self.object.chars().take(7).collect()
        } else {
            self.peeled_short.clone()
        }
    }

    /// Full commit this tag resolves to: the peeled commit for an annotated tag,
    /// or the object itself for a lightweight tag.
    fn commit_id(&self) -> &str {
        if self.commit.is_empty() {
            &self.object
        } else {
            &self.commit
        }
    }

    /// An annotated tag carries a message (and peels to a separate commit object);
    /// a lightweight tag does not.
    fn has_message(&self) -> bool {
        !self.commit.is_empty()
    }

    /// First line of this tag's own message, or `None` for a lightweight tag (which
    /// has no message of its own) or an annotated tag with an empty subject.
    fn message_subject(&self) -> Option<&str> {
        if !self.has_message() {
            return None;
        }
        let subject = self.message.trim();
        (!subject.is_empty()).then_some(subject)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TagRefs {
    local: BTreeMap<String, TagRef>,
    remote: BTreeMap<String, TagRef>,
}

impl TagRefs {
    fn is_remote(&self, tag: &TagRef) -> bool {
        self.remote
            .get(&tag.name)
            .is_some_and(|remote| remote.object == tag.object)
    }

    fn pending(&self) -> Vec<&TagRef> {
        self.local
            .values()
            .filter(|tag| !self.is_remote(tag))
            .collect()
    }
}

fn tag_refs(runner: &impl GitRunner, repo: &Path) -> Result<TagRefs, String> {
    Ok(TagRefs {
        local: load_refs(
            runner,
            repo,
            &["for-each-ref", LOCAL_TAG_FORMAT_ARG, "refs/tags"],
        )?,
        remote: load_refs(
            runner,
            repo,
            &[
                "for-each-ref",
                REMOTE_TAG_FORMAT_ARG,
                "refs/remotes/origin/tags",
            ],
        )?,
    })
}

fn load_refs(
    runner: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> Result<BTreeMap<String, TagRef>, String> {
    match runner.run(repo, args) {
        Ok(output) if output.exit_code == 0 => Ok(parse_refs(&output.stdout)),
        Ok(output) => Err(output.fail_detail("git for-each-ref failed")),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_refs(stdout: &str) -> BTreeMap<String, TagRef> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\t');
            let object = fields.next()?.to_string();
            let commit = fields.next()?.to_string();
            let peeled_short = fields.next()?.to_string();
            let name = fields.next()?.to_string();
            let message = fields.next().unwrap_or_default().to_string();
            if object.is_empty() || name.is_empty() {
                return None;
            }
            Some((
                name.clone(),
                TagRef {
                    object,
                    commit,
                    peeled_short,
                    name,
                    message,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
    };

    use super::*;
    use crate::commands::squash_local::GitOutput;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        repo: PathBuf,
        args: Vec<String>,
    }

    struct FakeRunner {
        calls: RefCell<Vec<Call>>,
        results: RefCell<Vec<GitOutput>>,
    }

    impl FakeRunner {
        fn new(results: Vec<GitOutput>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                results: RefCell::new(results),
            }
        }

        fn ok(stdout: &str) -> GitOutput {
            GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                exit_code: 0,
            }
        }

        fn exit(exit_code: i32) -> GitOutput {
            GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code,
            }
        }

        fn arg_lists(&self) -> Vec<Vec<String>> {
            self.calls
                .borrow()
                .iter()
                .map(|call| call.args.clone())
                .collect()
        }
    }

    impl GitRunner for FakeRunner {
        fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
            self.calls.borrow_mut().push(Call {
                repo: repo.to_path_buf(),
                args: args.iter().map(|arg| arg.to_string()).collect(),
            });
            Ok(self.results.borrow_mut().remove(0))
        }
    }

    #[test]
    fn list_tags_uses_plain_tag_list() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("aaa\t\t\tv1.0.0\nbbb\t\t\tv1.1.0\n"),
            FakeRunner::ok("aaa\t\t\tv1.0.0\n"),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(result.status, Status::Listed);
        assert_eq!(result.detail, "v1.0.0 [remote]\nv1.1.0 [local]");
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["for-each-ref", LOCAL_TAG_FORMAT_ARG, "refs/tags",],
                vec![
                    "for-each-ref",
                    REMOTE_TAG_FORMAT_ARG,
                    "refs/remotes/origin/tags",
                ],
            ]
        );
    }

    #[test]
    fn list_tags_with_commits_uses_format() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("aaa\tabc1234def\tabc1234\tv1.0.0\tship it\n"),
            FakeRunner::ok("aaa\tabc1234def\tabc1234\tv1.0.0\tship it\n"),
        ]);

        let result = list(&runner, Path::new("."), true);

        assert_eq!(result.status, Status::Listed);
        assert_eq!(result.detail, "abc1234 v1.0.0 [remote]  ship it");
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["for-each-ref", LOCAL_TAG_FORMAT_ARG, "refs/tags",],
                vec![
                    "for-each-ref",
                    REMOTE_TAG_FORMAT_ARG,
                    "refs/remotes/origin/tags",
                ],
            ]
        );
    }

    #[test]
    fn push_tags_skips_when_all_tags_are_already_tracked() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("aaa\t\t\tv1.0.0\n"),
            FakeRunner::ok("aaa\t\t\tv1.0.0\n"),
        ]);

        let result = push(&runner, Path::new("."));

        assert_eq!(
            result,
            TagResult::new(Status::Noop, "tags already up to date")
        );
        assert!(
            runner
                .arg_lists()
                .iter()
                .all(|args| args.first().map(String::as_str) != Some("push")),
            "no redundant push when fetched remote tracking tags match"
        );
    }

    #[test]
    fn push_tags_pushes_only_untracked_tags() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("aaa\t\t\tv1.0.0\nbbb\t\t\tv1.1.0\n"),
            FakeRunner::ok("aaa\t\t\tv1.0.0\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
        ]);

        let result = push(&runner, Path::new("."));

        assert_eq!(
            result,
            TagResult::new(Status::Pushed, "pushed 1 tag: v1.1.0")
        );
        let calls = runner.arg_lists();
        assert_eq!(
            calls.get(2),
            Some(&vec![
                "push".to_string(),
                "origin".to_string(),
                "refs/tags/v1.1.0:refs/tags/v1.1.0".to_string(),
            ])
        );
        assert_eq!(
            calls.get(3),
            Some(&vec![
                "update-ref".to_string(),
                "refs/remotes/origin/tags/v1.1.0".to_string(),
                "bbb".to_string(),
            ])
        );
    }

    #[test]
    fn add_tag_creates_annotated_tag() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("")]);

        let result = add(&runner, Path::new("."), "v1.2.0", "release notes");

        assert_eq!(
            result,
            TagResult::new(Status::Created, "created tag v1.2.0")
        );
        assert_eq!(
            runner.arg_lists(),
            vec![vec!["tag", "-a", "v1.2.0", "-m", "release notes"]]
        );
    }

    #[test]
    fn add_and_push_tag_creates_then_pushes() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),
            FakeRunner::ok("aaa\t\t\tv1.0.0\nbbb\t\t\tv1.1.0\nccc\t\t\tv1.2.0\n"),
            FakeRunner::ok("aaa\t\t\tv1.0.0\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
        ]);

        let result = add_and_push(&runner, Path::new("."), "v1.2.0", "release notes", None);

        assert_eq!(
            result,
            TagResult::new(Status::Pushed, "created tag v1.2.0\npushed 1 tag: v1.2.0")
        );
        let calls = runner.arg_lists();
        assert_eq!(
            calls.first(),
            Some(&vec![
                "tag".to_string(),
                "-a".to_string(),
                "v1.2.0".to_string(),
                "-m".to_string(),
                "release notes".to_string(),
            ])
        );
        assert_eq!(
            calls.get(3),
            Some(&vec![
                "push".to_string(),
                "origin".to_string(),
                "refs/tags/v1.2.0:refs/tags/v1.2.0".to_string(),
            ])
        );
    }

    #[test]
    fn add_and_push_with_label_creates_both_refs_and_pushes_them() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""), // tag -a v0.1.0
            FakeRunner::ok(""), // tag base-template v0.1.0^{}
            // local refs: annotated v0.1.0 + lightweight label, same commit ccc
            FakeRunner::ok("t01\tccc\tccc\tv0.1.0\nccc\t\t\tbase-template\n"),
            FakeRunner::ok(""), // remote refs (none)
            FakeRunner::ok(""), // push
            FakeRunner::ok(""), // update-ref v0.1.0
            FakeRunner::ok(""), // update-ref base-template
        ]);

        let result = add_and_push(
            &runner,
            Path::new("."),
            "v0.1.0",
            "base template",
            Some("base-template"),
        );

        assert_eq!(result.status, Status::Pushed);
        assert_eq!(
            result.detail,
            "created tag v0.1.0\ncreated tag base-template\npushed 2 tags: v0.1.0, base-template"
        );
        let calls = runner.arg_lists();
        assert_eq!(
            calls.get(1),
            Some(&vec![
                "tag".to_string(),
                "base-template".to_string(),
                "v0.1.0^{}".to_string(),
            ])
        );
    }

    #[test]
    fn label_tag_labels_existing_tag_and_pushes_only_the_label() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""), // tag base-template v0.1.0^{}
            // v0.1.0 already on remote; only the new label is pending
            FakeRunner::ok("t01\tccc\tccc\tv0.1.0\nccc\t\t\tbase-template\n"),
            FakeRunner::ok("t01\tccc\tccc\tv0.1.0\n"),
            FakeRunner::ok(""), // push
            FakeRunner::ok(""), // update-ref base-template
        ]);

        let result = label_tag(&runner, Path::new("."), "v0.1.0", "base-template");

        assert_eq!(
            result,
            TagResult::new(
                Status::Pushed,
                "created tag base-template\npushed 1 tag: base-template"
            )
        );
        let calls = runner.arg_lists();
        assert_eq!(
            calls.first(),
            Some(&vec![
                "tag".to_string(),
                "base-template".to_string(),
                "v0.1.0^{}".to_string(),
            ])
        );
        assert_eq!(
            calls.get(3),
            Some(&vec![
                "push".to_string(),
                "origin".to_string(),
                "refs/tags/base-template:refs/tags/base-template".to_string(),
            ])
        );
    }

    #[test]
    fn list_nests_label_under_its_canonical_tag() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("t01\tccc\tccc\tv0.1.0\nccc\t\t\tbase-template\n"),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(result.status, Status::Listed);
        assert_eq!(result.detail, "v0.1.0 [local]\n  - base-template [local]");
    }

    #[test]
    fn list_nests_all_labels_under_commit_when_none_has_a_message() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abcdef1\t\t\tlabel-a\nabcdef1\t\t\tlabel-b\n"),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(
            result.detail,
            "abcdef1\n  - label-a [local]\n  - label-b [local]"
        );
    }

    #[test]
    fn list_nests_all_under_commit_when_more_than_one_tag_has_a_message() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("t01\tccccccc\tccccccc\tv0.1.0\nt02\tccccccc\tccccccc\tv0.2.0\n"),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(
            result.detail,
            "ccccccc\n  - v0.1.0 [local]\n  - v0.2.0 [local]"
        );
    }

    #[test]
    fn list_shows_annotated_tag_message_first_line() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("d69\t48bf864\t48bf864\tv1.0.0\trelease one\n"),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(result.detail, "v1.0.0 [local]  release one");
    }

    #[test]
    fn list_omits_message_for_lightweight_tag() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abcdef1\t\t\tlabel-a\tunderlying commit subject\n"),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(result.detail, "label-a [local]");
    }

    #[test]
    fn list_canonical_shows_message_label_stays_bare() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(
                "t01\tccc\tccc\tv0.1.0\tbase template\nccc\t\t\tbase-template\tcommit subj\n",
            ),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(
            result.detail,
            "v0.1.0 [local]  base template\n  - base-template [local]"
        );
    }

    #[test]
    fn list_more_than_one_message_shows_each_subject_under_commit() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(
                "t01\tccccccc\tccccccc\tv0.1.0\tfirst subject\nt02\tccccccc\tccccccc\tv0.2.0\tsecond subject\n",
            ),
            FakeRunner::ok(""),
        ]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(
            result.detail,
            "ccccccc\n  - v0.1.0 [local]  first subject\n  - v0.2.0 [local]  second subject"
        );
    }

    #[test]
    fn classify_group_distinguishes_canonical_ambiguous_and_label_only() {
        let annotated = |name: &str| TagRef {
            object: format!("obj-{name}"),
            commit: "ccc".to_string(),
            peeled_short: "ccc".to_string(),
            name: name.to_string(),
            message: format!("message for {name}"),
        };
        let lightweight = |name: &str| TagRef {
            object: "ccc".to_string(),
            commit: String::new(),
            peeled_short: String::new(),
            name: name.to_string(),
            message: String::new(),
        };

        let canonical = annotated("v0.1.0");
        let label = lightweight("base-template");
        assert_eq!(
            classify_group(&[&canonical, &label]),
            TagGroup::Canonical(&canonical)
        );

        let other = annotated("v0.2.0");
        assert_eq!(
            classify_group(&[&canonical, &other]),
            TagGroup::MoreThanOneTagHasMessage
        );

        let label_b = lightweight("dev");
        assert_eq!(classify_group(&[&label, &label_b]), TagGroup::AllLabels);
    }

    #[test]
    fn failed_git_command_maps_to_failure() {
        let runner = FakeRunner::new(vec![FakeRunner::exit(128)]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(result.status, Status::Fail);
        assert!(result.detail.contains("exit 128"));
    }
}
