use std::collections::BTreeMap;
use std::path::Path;

use crate::commands::squash_local::GitRunner;

const LOCAL_TAG_FORMAT_ARG: &str =
    "--format=%(objectname)\t%(*objectname:short)\t%(refname:strip=2)";
const REMOTE_TAG_FORMAT_ARG: &str =
    "--format=%(objectname)\t%(*objectname:short)\t%(refname:strip=4)";

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

    let lines = refs
        .local
        .values()
        .map(|tag| {
            let state = if refs.is_remote(tag) {
                "remote"
            } else {
                "local"
            };
            if commits {
                format!("{} {} [{state}]", tag.commit_short(), tag.name)
            } else {
                format!("{} [{state}]", tag.name)
            }
        })
        .collect::<Vec<_>>();

    TagResult::new(Status::Listed, lines.join("\n"))
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

pub fn add_and_push(runner: &impl GitRunner, repo: &Path, tag: &str, message: &str) -> TagResult {
    let created = add(runner, repo, tag, message);
    if created.status == Status::Fail {
        return created;
    }

    let refs = match tag_refs(runner, repo) {
        Ok(refs) => refs,
        Err(detail) => return TagResult::new(Status::Fail, detail),
    };
    let Some(tag_ref) = refs.local.get(tag) else {
        return TagResult::new(Status::Fail, format!("created tag {tag} was not found"));
    };

    let pushed = if refs.is_remote(tag_ref) {
        TagResult::new(Status::Noop, "tags already up to date")
    } else {
        push_tags(runner, repo, &[tag_ref])
    };
    match pushed.status {
        Status::Pushed => TagResult::new(
            Status::Pushed,
            format!("{}\n{}", created.detail, pushed.detail),
        ),
        Status::Noop => TagResult::new(Status::Noop, created.detail),
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
    peeled_short: String,
    name: String,
}

impl TagRef {
    fn commit_short(&self) -> String {
        if self.peeled_short.is_empty() {
            self.object.chars().take(7).collect()
        } else {
            self.peeled_short.clone()
        }
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
            let mut fields = line.splitn(3, '\t');
            let object = fields.next()?.to_string();
            let peeled_short = fields.next()?.to_string();
            let name = fields.next()?.to_string();
            if object.is_empty() || name.is_empty() {
                return None;
            }
            Some((
                name.clone(),
                TagRef {
                    object,
                    peeled_short,
                    name,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

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
            FakeRunner::ok("aaa\t\tv1.0.0\nbbb\t\tv1.1.0\n"),
            FakeRunner::ok("aaa\t\tv1.0.0\n"),
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
            FakeRunner::ok("aaa\tabc1234\tv1.0.0\n"),
            FakeRunner::ok("aaa\tabc1234\tv1.0.0\n"),
        ]);

        let result = list(&runner, Path::new("."), true);

        assert_eq!(result.status, Status::Listed);
        assert_eq!(result.detail, "abc1234 v1.0.0 [remote]");
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
            FakeRunner::ok("aaa\t\tv1.0.0\n"),
            FakeRunner::ok("aaa\t\tv1.0.0\n"),
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
            FakeRunner::ok("aaa\t\tv1.0.0\nbbb\t\tv1.1.0\n"),
            FakeRunner::ok("aaa\t\tv1.0.0\n"),
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
            FakeRunner::ok("aaa\t\tv1.0.0\nbbb\t\tv1.1.0\nccc\t\tv1.2.0\n"),
            FakeRunner::ok("aaa\t\tv1.0.0\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
        ]);

        let result = add_and_push(&runner, Path::new("."), "v1.2.0", "release notes");

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
    fn failed_git_command_maps_to_failure() {
        let runner = FakeRunner::new(vec![FakeRunner::exit(128)]);

        let result = list(&runner, Path::new("."), false);

        assert_eq!(result.status, Status::Fail);
        assert!(result.detail.contains("exit 128"));
    }
}
