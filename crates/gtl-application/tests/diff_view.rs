use gtl_application::diffs::{FileDiff, FileStatus, sort_files_tree_order};

fn file(path: &str) -> FileDiff {
    FileDiff {
        path: path.to_string(),
        added: 0,
        removed: 0,
        lines: Vec::new(),
        full_lines: None,
    }
}

fn file_with_lines(lines: &[&str]) -> FileDiff {
    let mut file = file("example.rs");
    file.lines = lines.iter().map(|line| (*line).to_string()).collect();
    file
}

#[test]
fn file_tree_order_places_directories_before_files_at_each_level() {
    let mut files = vec![
        file("src/render.rs"),
        file("docs/adr/0001-render-stack.md"),
        file("src/assets/preview.css"),
        file("src/model.rs"),
        file("src/assets/components.js"),
    ];

    sort_files_tree_order(&mut files);

    assert_eq!(
        files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        [
            "docs/adr/0001-render-stack.md",
            "src/assets/components.js",
            "src/assets/preview.css",
            "src/model.rs",
            "src/render.rs",
        ]
    );
}

#[test]
fn file_status_classifies_raw_git_metadata() {
    for (lines, expected) in [
        (vec!["new file mode 100644"], FileStatus::Added),
        (vec!["+++ /dev/null"], FileStatus::Deleted),
        (vec!["rename from old.rs"], FileStatus::Renamed),
        (vec!["@@ -1 +1 @@"], FileStatus::Modified),
    ] {
        assert_eq!(file_with_lines(&lines).status(), expected);
    }
}
