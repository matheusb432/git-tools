use std::fmt::Write as _;

use gtl_infra::testing::TestRepository;
use gtl_wire::{
    proto::terminal_diff::SnapshotDecoder,
    terminal_diff::{ROW_BYTES_MAX, RowKind},
    v1::{
        self, terminal_diff_service_client::TerminalDiffServiceClient,
        viewer_service_client::ViewerServiceClient,
    },
    viewer::ViewerSyntaxClass,
};
use serial_test::serial;

use super::{ServerHarness, TestResult};

#[tokio::test]
#[serial(server_tracing)]
async fn terminal_snapshot_captures_context_without_desktop_state_and_validates_requests()
-> TestResult {
    let data = tempfile::tempdir()?;
    let repository = TestRepository::new();
    let mut original = String::new();
    for number in 1..=50 {
        writeln!(original, "context {number}")?;
    }
    repository.write("code.rs", &original);
    repository.commit_all("Initial code");
    repository.write("code.rs", original.replace("context 30", "changed thirty"));
    repository.write("untracked.txt", "untracked content\n");
    repository.write("syntax.rs", "pub fn café() { let name = \"α\"; } // note\n");
    let server = ServerHarness::start(data.path(), None).await?;
    let mut client = TerminalDiffServiceClient::new(server.native_channel());
    let mut viewer = ViewerServiceClient::new(server.native_channel());
    let before = viewer
        .get_viewer_shell(v1::GetViewerShellRequest {})
        .await?
        .into_inner();
    let request = || v1::ReadTerminalDiffRequest {
        working_directory: repository.path().to_string_lossy().into_owned(),
        target: Some(v1::DiffTarget {
            selection: Some(v1::diff_target::Selection::BaseRevision("HEAD".into())),
        }),
    };
    let mut stream = client.read_terminal_diff(request()).await?.into_inner();
    let mut decoder = SnapshotDecoder::default();
    while let Some(event) = stream.message().await? {
        decoder.accept(event)?;
    }
    let snapshot = decoder.finish()?;
    let syntax = snapshot
        .files()
        .iter()
        .find(|file| file.path == "syntax.rs")
        .ok_or("syntax diff")?;
    let added = syntax
        .compact
        .iter()
        .find(|row| row.kind == RowKind::Added)
        .ok_or("syntax source")?;
    for (text, class) in [
        ("pub", ViewerSyntaxClass::Keyword),
        ("café", ViewerSyntaxClass::Function),
        ("\"α\"", ViewerSyntaxClass::String),
        ("// note", ViewerSyntaxClass::Comment),
    ] {
        assert!(added.syntax.iter().any(|span| {
            span.syntax_class == Some(class)
                && span
                    .text(&added.text)
                    .is_some_and(|value| value.contains(text))
        }));
    }
    let code = snapshot
        .files()
        .iter()
        .find(|file| file.path == "code.rs")
        .ok_or("code diff")?;
    assert!(code.compact.iter().any(|row| row.kind == RowKind::Added
        && row.text == "changed thirty"
        && row.new_line_number == Some(30)));
    assert!(!code.compact.iter().any(|row| row.text == "context 1"));
    assert!(code.full.iter().any(|row| row.text == "context 1"));
    assert!(
        snapshot
            .files()
            .iter()
            .any(|file| file.path == "untracked.txt")
    );
    assert_eq!(
        before,
        viewer
            .get_viewer_shell(v1::GetViewerShellRequest {})
            .await?
            .into_inner()
    );
    assert_eq!(
        client
            .read_terminal_diff(v1::ReadTerminalDiffRequest {
                working_directory: "relative".into(),
                ..request()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    repository.write("untracked.txt", "x".repeat(ROW_BYTES_MAX + 1));
    let mut stream = client.read_terminal_diff(request()).await?.into_inner();
    assert_eq!(
        stream.message().await.unwrap_err().code(),
        tonic::Code::ResourceExhausted
    );
    server.stop().await?;
    Ok(())
}
