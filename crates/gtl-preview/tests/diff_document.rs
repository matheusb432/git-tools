use anyhow::{Context, Result, anyhow};
use gtl_application::{
    diffs::{Cmd, FileDiff, Foot, View},
    viewer::RenderOptions,
};
use gtl_models::diffs::Commit;
use scraper::{Html, Selector};

fn sample_view() -> View {
    View {
        exclusions: None,
        repo_name: "api".to_string(),
        repo_root: "/home/user/api".to_string(),
        branch: "main".to_string(),
        upstream: "origin/main".to_string(),
        commits: vec![Commit {
            sha: "abc123def".to_string(),
            subject: "feat: thing".to_string(),
            body: "extended notes".to_string(),
            date: String::new(),
            iso: String::new(),
            parents: Vec::new(),
        }],
        files: vec![FileDiff {
            path: "src/a b.rs".to_string(),
            added: 2,
            removed: 1,
            lines: vec![
                "@@ -1 +1,2 @@".to_string(),
                "-old".to_string(),
                "+new".to_string(),
                "+extra".to_string(),
            ],
            full_lines: None,
        }],
        title: "diff".to_string(),
        cmd: Cmd {
            lead: "git diff ".to_string(),
            range: "origin/main..HEAD".to_string(),
            trail: String::new(),
        },
        commits_label: "# commits".to_string(),
        foot: Foot {
            cmd: "git diff origin/main..HEAD".to_string(),
            note: "# read-only preview".to_string(),
        },
    }
}

fn selector(value: &str) -> Result<Selector> {
    Selector::parse(value).map_err(|error| anyhow!("invalid test selector `{value}`: {error:?}"))
}

#[test]
fn diff_document_shell_contains_only_file_blocks_and_empty_row_targets() -> Result<()> {
    let shell =
        gtl_preview::diff_document_shell(&sample_view(), RenderOptions::DEFAULT)?.into_string();
    let fragment = Html::parse_fragment(&shell);

    assert_eq!(
        fragment
            .select(&selector(
                "[data-gtl-diff-document][data-gtl-diff-scroller]",
            )?)
            .count(),
        1
    );
    assert_eq!(fragment.select(&selector("details.file")?).count(), 1);
    assert_eq!(fragment.select(&selector("summary")?).count(), 1);
    assert_eq!(fragment.select(&selector(".copy-button")?).count(), 3);
    assert_eq!(fragment.select(&selector(".status-badge")?).count(), 1);
    assert_eq!(
        fragment
            .select(&selector(r#"button[data-open-diff-file="src/a b.rs"]"#)?)
            .count(),
        1
    );

    let target = fragment
        .select(&selector("#viewer-diff-0")?)
        .next()
        .context("the first file row target")?;
    assert!(target.inner_html().is_empty());

    for excluded in [
        ".layout",
        ".titlebar",
        ".tree",
        ".shelf",
        ".keybar",
        "main",
        "[popover]",
        "#viewer-chunk-loader",
    ] {
        assert_eq!(
            fragment.select(&selector(excluded)?).count(),
            0,
            "the diff document must not render `{excluded}`"
        );
    }
    assert!(!shell.contains("hx-"));
    assert!(!shell.contains(">old<"));
    assert!(!shell.contains(">new<"));
    Ok(())
}

#[test]
fn chunks_recompose_the_complete_raw_artifact_rows() -> Result<()> {
    let view = sample_view();
    let chunks = gtl_preview::view_chunks(&view, RenderOptions::DEFAULT)?;
    let recomposed = chunks
        .iter()
        .map(|chunk| chunk.html.as_str())
        .collect::<String>();
    let artifact = gtl_preview::build_html(&view, RenderOptions::DEFAULT, None)?;
    let document = Html::parse_document(&artifact);
    let complete_rows = document
        .select(&selector(".main .diff")?)
        .next()
        .context("the complete raw diff rows")?
        .inner_html();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].target_id, "viewer-diff-0");
    assert_eq!(recomposed, complete_rows);
    Ok(())
}

#[test]
fn raw_artifact_retains_its_complete_layout_and_rows() -> Result<()> {
    let artifact = gtl_preview::build_html(&sample_view(), RenderOptions::DEFAULT, None)?;
    let document = Html::parse_document(&artifact);
    let text = document.root_element().text().collect::<String>();

    for retained in [
        ".layout",
        ".titlebar",
        ".tree",
        ".main",
        ".shelf",
        ".keybar",
        "[popover]",
        ".diff .dl",
    ] {
        assert!(
            document.select(&selector(retained)?).next().is_some(),
            "the raw artifact must retain `{retained}`"
        );
    }
    assert!(
        document
            .select(&selector("[data-gtl-diff-document]")?)
            .next()
            .is_none()
    );
    assert!(
        document
            .select(&selector("[data-open-diff-file]")?)
            .next()
            .is_none()
    );
    assert!(text.contains("old"));
    assert!(text.contains("new"));
    Ok(())
}
