#![cfg(unix)]

use anyhow::Result;

#[path = "support/cli_fixture.rs"]
mod cli_fixture;
mod common;

use cli_fixture::{CancelKey, CliFixture, assert_contains, assert_omits};

#[test]
fn confirmations_and_results_follow_the_cli_contract() -> Result<()> {
    let fixture = CliFixture::new()?;
    let _server = common::ServerHarness::start(Some(&fixture.config_path()), None)?;
    verify_noops_and_cancellation(&fixture)?;
    verify_push_review(&fixture)?;
    verify_tag_preview_and_publication(&fixture)?;
    verify_live_view_results(&fixture);
    verify_partial_failures(&fixture)?;
    verify_project_selection(&fixture)?;
    verify_recursive_push_preserves_changes()?;
    Ok(())
}

fn verify_recursive_push_preserves_changes() -> Result<()> {
    let outer = CliFixture::new()?;
    let inner = outer.nested()?;
    let mut before = Vec::new();
    for repository in [&outer, &inner] {
        repository.commit_file("ready.txt", "Ready work\n", "Prepare recursive push")?;
        repository.write_file("unfinished.txt", "Unfinished work\n")?;
        before.push((repository, repository.head()?, repository.working_tree()?));
    }

    let output = outer.output(&["p", "--recursive", "--yes"])?;
    assert_contains(&output, &["2 projects: 2 pushed"]);
    for (repository, head, changes) in before {
        assert_eq!(repository.origin_head()?, head);
        assert_eq!(repository.head()?, head);
        assert_eq!(repository.working_tree()?, changes);
    }
    Ok(())
}

fn verify_noops_and_cancellation(fixture: &CliFixture) -> Result<()> {
    fixture.succeeds(&["push", "Unused message"], "Nothing to commit or push\n");
    fixture.succeeds(&["push"], "Already up to date\n");

    let head = fixture.head()?;
    fixture.write_file("work.txt", "Example change\n")?;
    fixture.requires_confirmation(&["push", "Example change"]);
    for key in [CancelKey::No, CancelKey::Quit, CancelKey::Escape] {
        fixture.cancel(&["push", "Example change"], key)?;
        assert_eq!(fixture.head()?, head);
        assert_eq!(fixture.working_tree()?, "?? work.txt");
    }
    fixture.accept_default(&["push", "Example change"], "Staged, committed, and pushed")?;
    assert_ne!(fixture.head()?, head);
    Ok(())
}

fn verify_push_review(fixture: &CliFixture) -> Result<()> {
    fixture.commit_file("work.txt", "Pending example change\n", "Prepare push")?;
    fixture.requires_confirmation(&["push"]);
    for arguments in [&["push"][..], &["push", "Example message"], &["push", "-r"]] {
        fixture.cancel(arguments, CancelKey::No)?;
        assert_eq!(fixture.unpushed_commits()?, 1);
    }
    fixture.succeeds(&["push", "--yes"], "Pushed 1 commit\n");
    assert_contains(&fixture.output(&["push", "-r"])?, &["Up to date"]);
    Ok(())
}

fn verify_tag_preview_and_publication(fixture: &CliFixture) -> Result<()> {
    let destination = fixture.use_push_remote("publish.git")?;
    let mirror = fixture.add_push_remote("mirror.git")?;
    let preview = fixture.output(&["tag", "bump", "Hidden annotation", "--push", "--dry"])?;
    assert_contains(
        &preview,
        &["Tag bump preview", "publish.git", "mirror.git", "v1.0.1"],
    );
    assert_omits(&preview, &["Hidden annotation", "pattern", "branch"]);

    fixture.cancel(
        &["tag", "bump", "Example release", "--push"],
        CancelKey::Quit,
    )?;
    assert_eq!(fixture.tags()?, ["v1.0.0"]);
    fixture.succeeds(
        &["tag", "bump", "Example release", "--push", "--yes"],
        "Created tag v1.0.1 and pushed to origin\n",
    );
    assert_eq!(destination.tags()?, ["v1.0.1"]);
    assert_eq!(mirror.tags()?, ["v1.0.1"]);
    fixture.succeeds(
        &["tag", "add", "example-label", "Example annotation"],
        "Created tag example-label\n",
    );
    Ok(())
}

fn verify_live_view_results(fixture: &CliFixture) {
    fixture.succeeds(
        &["diff", "live", "--path", "."],
        "Saved live view for example-project\n",
    );
    fixture.succeeds(
        &["diff", "live", "--path", "."],
        "Refreshed live view for example-project\n",
    );
}

fn verify_partial_failures(fixture: &CliFixture) -> Result<()> {
    fixture.make_push_remote_unavailable()?;
    fixture.fails(
        &["tag", "bump", "Example release", "--push", "--yes"],
        1,
        &["Created locally: v1.0.2", "Push outcome unconfirmed"],
    );
    assert!(fixture.tags()?.contains(&"v1.0.2".to_string()));

    fixture.write_file("work.txt", "Another example change\n")?;
    fixture.fails(
        &["push", "Example change", "--yes"],
        1,
        &["Created local commit:", "Push outcome unconfirmed"],
    );
    assert_eq!(fixture.last_commit_message()?, "Example change");
    Ok(())
}

fn verify_project_selection(settings: &CliFixture) -> Result<()> {
    let caller = CliFixture::new()?;
    let selected = CliFixture::managed("DEMO", "Selected project")?;
    caller.write_file("untouched.txt", "Caller change\n")?;
    let caller_head = caller.head()?;
    selected.commit_file("selected.txt", "Selected change\n", "Prepare selected diff")?;

    let artifact = caller.artifact(&["diff", "--id", "demo", "--raw"])?;
    assert_contains(&artifact, &["selected.txt"]);
    assert_omits(&artifact, &["untouched.txt"]);
    caller.succeeds(&["push", "--id", "demo", "--yes"], "Pushed 1 commit\n");
    assert_eq!(selected.head()?, selected.origin_head()?);

    selected.write_file("new-work/nested.txt", "Selected new work\n")?;
    let artifact = caller.artifact(&["diff", "HEAD", "--id", "demo", "--raw"])?;
    assert_contains(&artifact, &["Selected new work"]);
    assert_omits(&artifact, &["untouched.txt"]);
    caller.succeeds(
        &["push", "--id", "DEMO", "Selected message", "--yes"],
        "Staged, committed, and pushed\n",
    );
    assert_eq!(selected.last_commit_message()?, "Selected message");
    assert_eq!(selected.working_tree()?, "");

    let before_pull = selected.head()?;
    let remote_head = selected.advance_origin("remote.txt", "Remote work\n")?;
    caller.succeeds(
        &["pull", "--id", "demo", "--dry"],
        "Behind by 1 - fast-forward\n",
    );
    assert_eq!(selected.head()?, before_pull);
    caller.succeeds(&["pull", "--id", "demo"], "Fast-forwarded 1 commit\n");
    assert_eq!(selected.head()?, remote_head);
    caller.succeeds(&["pull", "--id", "demo"], "Up to date\n");

    for arguments in [
        &["push", "--id", "NONE", "--yes"][..],
        &["pull", "--id", "NONE"],
        &["diff", "--id", "NONE", "--raw"],
    ] {
        caller.fails(arguments, 3, &["This project is no longer available."]);
    }
    assert_eq!(caller.head()?, caller_head);
    assert_eq!(caller.working_tree()?, "?? untouched.txt");
    verify_project_batches(&caller, &selected, settings)?;
    Ok(())
}

fn verify_project_batches(
    caller: &CliFixture,
    selected: &CliFixture,
    settings: &CliFixture,
) -> Result<()> {
    let excluded = CliFixture::managed("EXCL", "Excluded project")?;
    excluded.write_file("excluded.txt", "Excluded work\n")?;
    selected.commit_file("batch.txt", "Batch work\n", "Prepare batch push")?;
    selected.write_file("batch/nested.txt", "Batch work\n")?;
    settings.write_settings(
        "[[projects]]\nname = \"Excluded project\"\nexcluded_from_push_all = true\n",
    )?;
    let selected_head = selected.head()?;
    let excluded_head = excluded.head()?;

    let preview = caller.output(&["project", "push", "--all", "--dry", "--json"])?;
    let preview: serde_json::Value = serde_json::from_str(&preview)?;
    assert_eq!(preview["Selected"][0]["Status"], "would-push");
    assert_eq!(preview["Excluded"], serde_json::json!(["Excluded project"]));
    assert_eq!(selected.head()?, selected_head);
    assert_eq!(selected.working_tree()?, "?? batch/");

    let result = caller.output(&["project", "push", "--all"])?;
    assert_contains(&result, &["2 projects: 1 pushed, 1 excluded"]);
    assert_eq!(selected.head()?, selected.origin_head()?);
    assert_eq!(selected.head()?, selected_head);
    assert_eq!(selected.working_tree()?, "?? batch/");
    assert_eq!(excluded.head()?, excluded_head);
    assert_eq!(excluded.working_tree()?, "?? excluded.txt");
    assert_eq!(
        caller.output(&["ls", "--json"])?,
        caller.output(&["project", "ls", "--json"])?
    );

    let remote_head = selected.advance_origin("batch-remote.txt", "Batch remote work\n")?;
    let pull = caller.output(&["project", "pull", "--all"])?;
    assert_contains(
        &pull,
        &[
            "Selected project",
            "Pulled",
            "Excluded project",
            "Up to date",
        ],
    );
    assert_eq!(selected.head()?, remote_head);
    caller.succeeds(&["pull"], "Up to date\n");

    selected.commit_file("batch-diff.txt", "Batch diff work\n", "Prepare batch diff")?;
    let artifact = caller.artifact(&["project", "diff", "--all", "--raw"])?;
    assert_contains(&artifact, &["batch-diff.txt"]);
    assert_omits(&artifact, &["untouched.txt"]);

    selected.make_push_remote_unavailable()?;
    let peer = CliFixture::managed("PEER", "Peer project")?;
    peer.commit_file("peer.txt", "Peer work\n", "Prepare peer push")?;
    peer.write_file("unfinished.txt", "Unfinished peer work\n")?;
    let selected_head = selected.head()?;
    let peer_head = peer.head()?;
    let failure = caller.output_with_exit(&["project", "push", "--all"], 1)?;
    assert_contains(
        &failure,
        &[
            "Failed",
            "unavailable.git",
            "Peer project",
            "3 projects: 1 pushed, 1 failed, 1 excluded",
        ],
    );
    assert_eq!(peer.origin_head()?, peer_head);
    assert_eq!(peer.head()?, peer_head);
    assert_eq!(peer.working_tree()?, "?? unfinished.txt");
    assert_eq!(selected.head()?, selected_head);
    assert_eq!(selected.working_tree()?, "?? batch/");
    assert_ne!(selected.head()?, selected.origin_head()?);
    assert_eq!(excluded.head()?, excluded_head);

    caller.succeeds(
        &["push", "--id", "EXCL", "Explicit selection", "--yes"],
        "Staged, committed, and pushed\n",
    );
    assert_eq!(excluded.head()?, excluded.origin_head()?);
    Ok(())
}
