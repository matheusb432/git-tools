#![cfg(test)]

use gtl_application::ports::GitClient as _;
use gtl_infra::{git_client::HybridGitClient, testing::TestRepository};
use gtl_models::git::GitRevision;

#[test]
fn real_git_resolves_an_annotated_tag_to_its_commit_id() {
    let repository = TestRepository::new();
    repository.write("tracked.txt", "initial\n");
    let head = repository.commit_all("initial");
    repository.git(&["tag", "-am", "release", "v1.0.0"]);

    let id = HybridGitClient
        .resolve_commit_id(&repository.root(), &GitRevision::try_new("v1.0.0").unwrap())
        .unwrap();

    assert_eq!(id.as_ref(), head);
    assert_ne!(id.as_ref(), repository.git(&["rev-parse", "v1.0.0"]));
}
