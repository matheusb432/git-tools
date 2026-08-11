use std::fs;

use anyhow::{Context as _, Result, ensure};
use gtl_application::{
    diffs::{Cmd, FileDiff, Foot, View},
    viewer::RenderOptions,
};

use crate::support::{self, wait};

#[tokio::test(flavor = "multi_thread")]
async fn raw_artifact_runs_from_file_url_in_webkit() -> Result<()> {
    support::run_test("raw-artifact-webkit", |session| {
        Box::pin(async move {
            let directory = tempfile::Builder::new()
                .prefix("gtl-webkit-artifact-")
                .tempdir()
                .context("create WebKit raw artifact directory")?;
            let artifact = directory.path().join("artifact.html");
            let html = gtl_artifacts::build_html(&artifact_view(), RenderOptions::DEFAULT, None)
                .context("build WebKit raw artifact")?;
            let module = "<script type=\"module\">";
            let error_probe = r#"<script>
globalThis.__gtlWebKitErrors = [];
globalThis.addEventListener("error", (event) => globalThis.__gtlWebKitErrors.push(event.message));
globalThis.addEventListener("unhandledrejection", (event) => globalThis.__gtlWebKitErrors.push(String(event.reason)));
const gtlOriginalConsoleError = console.error;
console.error = (...values) => {
  globalThis.__gtlWebKitErrors.push(values.map(String).join(" "));
  gtlOriginalConsoleError(...values);
};
</script>"#;
            ensure!(html.contains(module), "WebKit artifact module is missing");
            let html = html.replacen(module, &format!("{error_probe}{module}"), 1);
            fs::write(&artifact, html).context("write WebKit raw artifact")?;
            let artifact = artifact
                .canonicalize()
                .context("canonicalize raw artifact")?;
            let artifact_url = format!("file://{}", artifact.display());
            let driver = session.driver();
            driver
                .set_window_rect(0, 0, 390, 844)
                .await
                .context("resize WebKit raw artifact viewport")?;
            driver
                .goto(&artifact_url)
                .await
                .context("open WebKit raw artifact from file URL")?;

            let observation = wait::until(
                "compressed raw artifact to finish in WebKit",
                wait::ASSERTION_TIMEOUT,
                || async {
                    let result = driver
                        .execute(
                            r#"
const documentState = document.querySelector('[data-gtl-diff-document]');
const ready = document.querySelector('main[data-gtl-artifact-ready="true"]') !== null
  && documentState?.dataset.viewState === 'complete';
if (!ready) return null;
return {
  marker: documentState.textContent.includes('webkit-marker'),
  syntaxSpans: documentState.querySelectorAll('[class*="--sy-"]').length,
  compressedNodes: document.querySelectorAll('script[type="application/octet-stream"]').length,
  hasLoader: typeof globalThis.__gtlLoadCompressedAsset === 'function',
  innerWidth: window.innerWidth,
  errors: globalThis.__gtlWebKitErrors,
};
"#,
                            Vec::new(),
                        )
                        .await
                        .context("inspect completed WebKit raw artifact")?;
                    Ok((!result.json().is_null()).then(|| result.json().clone()))
                },
            )
            .await?;
            ensure!(
                observation["marker"].as_bool() == Some(true),
                "WebKit raw artifact omitted its diff marker"
            );
            ensure!(
                observation["syntaxSpans"].as_u64().unwrap_or_default() > 0,
                "WebKit raw artifact rendered no syntax highlighting"
            );
            ensure!(
                observation["compressedNodes"].as_u64() == Some(0),
                "WebKit retained consumed compressed nodes"
            );
            ensure!(
                observation["hasLoader"].as_bool() == Some(true),
                "WebKit artifact did not install the compressed asset loader"
            );
            ensure!(
                observation["innerWidth"]
                    .as_u64()
                    .is_some_and(|width| width <= 390),
                "WebKit artifact did not retain the phone-sized viewport"
            );
            ensure!(
                observation["errors"].as_array().is_some_and(Vec::is_empty),
                "WebKit raw artifact raised browser errors: {}",
                observation["errors"]
            );

            support::selectors::by_accessible_name(driver, "Changed files")
                .await?
                .click()
                .await
                .context("open WebKit raw artifact file navigation")?;
            support::selectors::by_css(
                driver,
                "dialog[open] [data-file-target='f-src-beta-rs']",
                "second file in WebKit raw artifact navigation",
            )
            .await?
            .click()
            .await
            .context("navigate to the second WebKit raw artifact file")?;
            support::selectors::by_css(
                driver,
                "details#f-src-beta-rs[open]",
                "opened second WebKit raw artifact file",
            )
            .await?;
            ensure!(
                driver.current_url().await?.as_str() == artifact_url,
                "WebKit raw artifact navigation left its file URL"
            );
            Ok(())
        })
    })
    .await
}

fn artifact_view() -> View {
    let alpha_lines = vec![
        "@@ -1 +1 @@".to_owned(),
        "+pub fn alpha() { println!(\"webkit-marker\"); }".to_owned(),
    ];
    let beta_lines = vec![
        "@@ -1 +1 @@".to_owned(),
        "+pub fn beta() -> usize { 42 }".to_owned(),
    ];
    View {
        exclusions: None,
        repo_name: "webkit".to_owned(),
        repo_root: "/tmp/webkit-artifact".to_owned(),
        branch: "feature".to_owned(),
        upstream: "main".to_owned(),
        commits: Vec::new(),
        files: vec![
            FileDiff {
                path: "src/alpha.rs".to_owned(),
                added: 1,
                removed: 0,
                full_lines: Some(alpha_lines.clone()),
                lines: alpha_lines,
            },
            FileDiff {
                path: "src/beta.rs".to_owned(),
                added: 1,
                removed: 0,
                full_lines: Some(beta_lines.clone()),
                lines: beta_lines,
            },
        ],
        title: "compressed artifact".to_owned(),
        cmd: Cmd {
            lead: "git diff ".to_owned(),
            range: "main..feature".to_owned(),
            trail: String::new(),
        },
        commits_label: "0 commits".to_owned(),
        foot: Foot {
            cmd: "git diff main..feature".to_owned(),
            note: "# offline WebKit evidence".to_owned(),
        },
    }
}
