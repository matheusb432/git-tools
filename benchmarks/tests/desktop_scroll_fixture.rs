use gtl_benchmarks::desktop_scroll::{
    COMMIT_COUNT, DISTINCT_FILE_COUNT, FILE_TOUCH_COUNT_PER_COMMIT, FIXTURE_INPUT_BYTES_MAX,
    fixture_root, verify_fixture,
};

#[test]
fn committed_fixture_reconstructs_the_bounded_desktop_workload() {
    let evidence = verify_fixture(&fixture_root()).unwrap();
    let manifest = evidence.manifest;

    assert_eq!(manifest.workload.commit_count, COMMIT_COUNT);
    assert_eq!(manifest.workload.distinct_file_count, DISTINCT_FILE_COUNT);
    assert_eq!(
        manifest
            .commits
            .iter()
            .map(|commit| commit.file_touch_count)
            .collect::<Vec<_>>(),
        vec![FILE_TOUCH_COUNT_PER_COMMIT; COMMIT_COUNT]
    );
    assert!(evidence.fixture_input_bytes <= FIXTURE_INPUT_BYTES_MAX);
    assert!(manifest.workload.compact_diff_rows < manifest.workload.full_context_diff_rows);
    assert!(manifest.workload.compact_diff_bytes < manifest.workload.full_context_diff_bytes);
    assert!(manifest.workload.additions > 0);
    assert!(manifest.workload.deletions > 0);
    assert!(manifest.statuses.added > 0);
    assert!(manifest.statuses.modified > 0);
    assert!(manifest.statuses.deleted > 0);
    assert!(manifest.statuses.renamed > 0);
}
