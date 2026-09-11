use std::path::PathBuf;

use gtl_models::paths::RepositoryRoot;

use super::comparison_popover_id;
use crate::test_support::TestResult;

#[test]
fn popover_ids_distinguish_path_punctuation_from_its_numeric_encoding() -> TestResult {
    let paths = [
        "/home/dev/a-b",
        "/home/dev/a_b",
        "/home/dev/a-45-b",
        "/home/dev/a/45/b",
    ];
    let ids = paths
        .into_iter()
        .map(|path| {
            RepositoryRoot::try_new(PathBuf::from(path)).map(|path| comparison_popover_id(&path))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, id) in ids.iter().enumerate() {
        assert!(!ids[index + 1..].contains(id));
    }
    Ok(())
}
