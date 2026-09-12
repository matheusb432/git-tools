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
    Ok(())
}

fn verify_noops_and_cancellation(fixture: &CliFixture) -> Result<()> {
    fixture.succeeds(&["commit", "Unused message"], "Nothing to commit\n");
    fixture.succeeds(&["push"], "Already up to date\n");

    let head = fixture.head()?;
    fixture.write_file("work.txt", "Example change\n")?;
    fixture.requires_confirmation(&["commit", "Example change"]);
    for key in [CancelKey::No, CancelKey::Quit, CancelKey::Escape] {
        fixture.cancel(&["commit", "Example change"], key)?;
        assert_eq!(fixture.head()?, head);
        assert_eq!(fixture.working_tree()?, "?? work.txt");
    }
    fixture.accept_default(&["commit", "Example change"], "Staged and committed")?;
    assert_ne!(fixture.head()?, head);
    Ok(())
}

fn verify_push_review(fixture: &CliFixture) -> Result<()> {
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
