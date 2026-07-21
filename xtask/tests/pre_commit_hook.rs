use std::{fs, path::Path, process::Command};

#[test]
fn tracked_hook_is_thin_posix_glue() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a repository parent");
    let hook = repository.join(".githooks/pre-commit");
    let contents = fs::read_to_string(&hook).expect("tracked pre-commit hook exists");

    assert!(contents.starts_with("#!/bin/sh\n"));
    assert!(contents.contains("git rev-parse --show-toplevel"));
    assert!(contents.contains("exec cargo run --quiet -p xtask -- pre-commit"));
    assert!(
        Command::new("sh")
            .args(["-n", hook.to_str().expect("hook path is UTF-8")])
            .status()
            .expect("sh starts")
            .success()
    );
}
