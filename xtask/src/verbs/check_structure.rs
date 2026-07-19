//! `check-structure` verb — mechanical architecture lint.
//!
//! Walks every `crates/*/src` and `shared/*/src` directory and exits 3 on any violation of the
//! architecture rules (see `docs/planning/specs/2026-07-02-pragmatic-backend-architecture.md`,
//! Mechanical Gates section). Called standalone as `cargo run -p xtask -- check-structure
//! [<root>]` and wired
//! into `lint` / `check` so the architecture gate cannot be silently skipped.
//!
//! ## Rules
//!
//! 1. **Max folder depth 2** under each crate's `src/`. `src/a/b/` is the deepest allowed;
//!    `src/a/b/c/` is a violation. Depth is counted as directory components only (files do not
//!    count toward depth).
//! 2. **Feature folders are flat.** No directory named `errors` or `events` may be nested inside a
//!    feature folder (depth ≥ 2 under `src/`). The convention is flat files `errors.rs` /
//!    `events.rs` alongside the feature's handler — never a subdirectory.
//! 3. **No `services/` directory.** The convention is per-feature `service.rs` files plus a single
//!    `application/src/shared/` module; any directory named `services` anywhere in a crate's `src`
//!    tree is a violation. The layout walk rejects symlinks instead of following or silently
//!    skipping source trees that could hide violations of rules 1-3.
//! 4. **CLI operations are thin.** Production CLI Rust may compose application operations and pass
//!    the concrete Git adapter, but may not reference or invoke the application Git port, use the
//!    retired CLI Git shim, launch Git directly, or hide source behind symlinks.
//! 5. **App state is a resource port.** `AppStateStore` exposes exactly `connection_lock`, keeping
//!    operation-specific persistence in application slices.
//! 6. **App-state infrastructure does not own runtime SQL.** Production Rust under
//!    `crates/infra/src/app_state/` may initialize `SQLite`, but query and mutation operations
//!    belong in application slices.

use std::path::Path;

use anyhow::{Context as _, Result};

mod rust_source;

use rust_source::{
    git_boundary_violations, sqlite_operation_identifiers, trait_method_names_top_level,
};

/// Walk every `crates/*/src` and `shared/*/src` directory under `root` and exit 3 on any
/// architecture violation.
///
/// `root` defaults to `.` (the current working directory, which should be the repo root). Pass an
/// explicit path when testing against a fixture tree.
///
/// # Errors
///
/// Returns an error when the filesystem cannot be traversed. Structural violations do **not**
/// produce an `Err` — they print to stderr and call [`std::process::exit`]`(3)`.
pub(crate) fn run(root: Option<&Path>) -> Result<()> {
    let root = root.unwrap_or(Path::new("."));
    let violations = collect_violations(root)
        .with_context(|| format!("check-structure: scanning {}", root.display()))?;

    for v in &violations {
        eprintln!("check-structure violation: {v}");
    }

    if !violations.is_empty() {
        eprintln!(
            "\n{} architecture violation(s): fix the reported violations and re-run `just check`",
            violations.len()
        );
        std::process::exit(3);
    }

    Ok(())
}

/// The workspace member directories scanned for layout violations — the app crates and the
/// app-agnostic `shared/*` crates. Both buckets obey the same three rules.
const MEMBER_DIRS: [&str; 2] = ["crates", "shared"];

/// Crates exempt from rule 1 (max folder depth) while they are still legacy-shaped —
/// rules 2 and 3 still apply. Shrink this list as migration phases land — new
/// architecture crates (`domain`, `contracts`, `application`, `infra`, `daemon`) are
/// never added.
const DEPTH_EXEMPT_CRATES: [&str; 1] = ["cli"];

/// Every production CLI Rust source belongs behind the application Git-operation boundary.
const CLI_SOURCE_DIR: &str = "crates/cli/src";
const CLI_SOURCE_SYMLINK_DESCRIPTION: &str = "production CLI source tree may not contain symlinks";
const SOURCE_LAYOUT_SYMLINK_DESCRIPTION: &str = "source layout tree may not contain symlinks";
const APP_STATE_PORTS_PATH: &str = "crates/application/src/ports.rs";
const APP_STATE_SOURCE_DIR: &str = "crates/infra/src/app_state";
const APP_STATE_STORE_METHOD_NAMES: [&str; 1] = ["connection_lock"];

/// Collect every violation under `root/{crates,shared}/*/src`.
fn collect_violations(root: &Path) -> Result<Vec<String>> {
    let mut violations = Vec::new();
    for member_dir in MEMBER_DIRS {
        collect_member_violations(&root.join(member_dir), &mut violations)?;
    }
    collect_cli_source_violations(root, &mut violations)?;
    collect_app_state_store_violations(root, &mut violations)?;
    collect_app_state_sql_violations(root, &mut violations)?;
    Ok(violations)
}

/// Collects rule-5 violations from the application app-state port.
fn collect_app_state_store_violations(root: &Path, violations: &mut Vec<String>) -> Result<()> {
    let path = root.join(APP_STATE_PORTS_PATH);
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading app-state port {}", path.display()));
        }
    };
    let method_names = trait_method_names_top_level(&source, "AppStateStore").unwrap_or_default();

    if method_names.as_slice() != APP_STATE_STORE_METHOD_NAMES {
        violations.push(format!(
            "[rule 5: AppStateStore exposes the connection resource only] {}: expected AppStateStore methods {:?}, found {method_names:?}",
            path.display(),
            APP_STATE_STORE_METHOD_NAMES,
        ));
    }

    Ok(())
}

/// Collects rule-6 violations from production app-state infrastructure.
fn collect_app_state_sql_violations(root: &Path, violations: &mut Vec<String>) -> Result<()> {
    let source_dir = root.join(APP_STATE_SOURCE_DIR);
    match std::fs::symlink_metadata(&source_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            walk_app_state_sources(&source_dir, violations)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", source_dir.display()));
        }
    }
    Ok(())
}

/// Recursively scans app-state Rust files in path order without following symlinks.
fn walk_app_state_sources(current: &Path, violations: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .with_context(|| format!("reading {}", current.display()))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", entry.path().display()))?;
        let path = entry.path();
        if file_type.is_dir() {
            walk_app_state_sources(&path, violations)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            collect_app_state_file_violations(&path, violations)?;
        }
    }

    Ok(())
}

/// Collects forbidden `SQLite` operations from one production app-state source file.
fn collect_app_state_file_violations(path: &Path, violations: &mut Vec<String>) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("reading app-state source {}", path.display()))?;

    for identifier in sqlite_operation_identifiers(&source) {
        violations.push(format!(
            "[rule 6: app-state SQL belongs to application slices] {}: production app-state infrastructure may not call SQLite operation .{identifier}",
            path.display(),
        ));
    }

    Ok(())
}

/// Collects rule-4 violations from every production Rust source in the CLI crate.
fn collect_cli_source_violations(root: &Path, violations: &mut Vec<String>) -> Result<()> {
    let source_dir = root.join(CLI_SOURCE_DIR);
    match std::fs::symlink_metadata(&source_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            walk_cli_sources(&source_dir, violations)?;
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            collect_cli_symlink_violation(&source_dir, violations);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", source_dir.display()));
        }
    }
    Ok(())
}

/// Recursively scans CLI Rust files in path order without following symlinks.
fn walk_cli_sources(current: &Path, violations: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .with_context(|| format!("reading {}", current.display()))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", entry.path().display()))?;
        let path = entry.path();
        if file_type.is_symlink() {
            collect_cli_symlink_violation(&path, violations);
        } else if file_type.is_dir() {
            walk_cli_sources(&path, violations)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            collect_cli_file_violations(&path, violations)?;
        }
    }

    Ok(())
}

fn collect_cli_symlink_violation(path: &Path, violations: &mut Vec<String>) {
    violations.push(format!(
        "[rule 4: CLI operations use application slices] {} — {CLI_SOURCE_SYMLINK_DESCRIPTION}",
        path.display()
    ));
}

fn collect_source_layout_symlink_violation(path: &Path, violations: &mut Vec<String>) {
    violations.push(format!(
        "[rules 1-3: source layout is bounded] {} — {SOURCE_LAYOUT_SYMLINK_DESCRIPTION}",
        path.display()
    ));
}

/// Collects forbidden Git boundary markers from one production CLI source file.
fn collect_cli_file_violations(path: &Path, violations: &mut Vec<String>) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("reading CLI source {}", path.display()))?;

    for violation in git_boundary_violations(&source) {
        violations.push(format!(
            "[rule 4: CLI operations use application slices] {} — production CLI source may not {}",
            path.display(),
            violation.description()
        ));
    }

    Ok(())
}

/// Collect violations under one member directory (e.g. `crates/` or `shared/`). A missing directory
/// is not an error — it simply contributes nothing.
fn collect_member_violations(member_dir: &Path, violations: &mut Vec<String>) -> Result<()> {
    match std::fs::symlink_metadata(member_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            collect_source_layout_symlink_violation(member_dir, violations);
            return Ok(());
        }
        Ok(metadata) if !metadata.file_type().is_dir() => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", member_dir.display()));
        }
    }

    let mut entries: Vec<_> = std::fs::read_dir(member_dir)
        .with_context(|| format!("reading {}", member_dir.display()))?
        .collect::<Result<_, _>>()?;

    // Sort for deterministic output order in tests.
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", entry.path().display()))?;
        if file_type.is_symlink() {
            collect_source_layout_symlink_violation(&entry.path(), violations);
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let crate_name = entry.file_name();
        let skip_depth_rule = DEPTH_EXEMPT_CRATES.iter().any(|e| crate_name == *e);
        let cli_source =
            member_dir.file_name().is_some_and(|name| name == "crates") && crate_name == "cli";
        let src = entry.path().join("src");
        let src_metadata = match std::fs::symlink_metadata(&src) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}", src.display()));
            }
        };
        if src_metadata.file_type().is_symlink() {
            if !cli_source {
                collect_source_layout_symlink_violation(&src, violations);
            }
            continue;
        }
        if !src_metadata.file_type().is_dir() {
            continue;
        }
        walk_dirs(&src, 0, skip_depth_rule, !cli_source, violations)?;
    }

    Ok(())
}

/// Recursively walk directories under `src/`, applying all three rules to each discovered
/// sub-directory.
///
/// `current` is the directory currently being iterated; `depth` is how many directory components
/// separate `current` from `src/` (0 = items directly inside `src/`). `skip_depth_rule` exempts a
/// legacy crate (see [`DEPTH_EXEMPT_CRATES`]) from rule 1 only — rules 2 and 3 still apply.
fn walk_dirs(
    current: &Path,
    depth: usize,
    skip_depth_rule: bool,
    collect_source_symlinks: bool,
    violations: &mut Vec<String>,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .with_context(|| format!("reading {}", current.display()))?
        .collect::<Result<_, _>>()?;

    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", entry.path().display()))?;
        if file_type.is_symlink() {
            if collect_source_symlinks {
                collect_source_layout_symlink_violation(&entry.path(), violations);
            }
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();

        let child_depth = depth + 1;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Rule 1: max folder depth 2 under src/.
        if !skip_depth_rule && child_depth > 2 {
            violations.push(format!(
                "[rule 1: max depth 2] {} — depth {} under src/ (max is 2)",
                path.display(),
                child_depth,
            ));
            // Continue walking so deeper violations are also reported.
        }

        // Rule 2: no `errors/` or `events/` directory inside a feature folder (depth ≥ 2).
        if child_depth >= 2 && (name == "errors" || name == "events") {
            violations.push(format!(
                "[rule 2: feature folders flat — no nested errors/events dir] {}",
                path.display()
            ));
        }

        // Rule 3: no directory named `services` anywhere.
        if name == "services" {
            violations.push(format!(
                "[rule 3: no services/ dir] {} — use per-feature service.rs files instead",
                path.display()
            ));
        }

        walk_dirs(
            &path,
            child_depth,
            skip_depth_rule,
            collect_source_symlinks,
            violations,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn seed(base: &Path, rel: &str) {
        seed_with(base, rel, "// fixture\n");
    }

    fn seed_with(base: &Path, rel: &str, contents: &str) {
        let full = base.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, contents).unwrap();
    }

    #[test]
    fn compliant_repo_has_no_violations() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/domain/src/todos/item.rs");
        seed(dir.path(), "crates/api/src/endpoints/todos/create.rs");
        seed(dir.path(), "crates/domain/src/todos/errors.rs"); // file, not dir — fine
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "unexpected violations: {v:?}");
    }

    #[test]
    fn depth_3_dir_is_a_rule_1_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/x/src/a/b/c/f.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 1")), "violations: {v:?}");
    }

    #[test]
    fn nested_errors_dir_is_a_rule_2_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/domain/src/todos/errors/detail.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 2")), "violations: {v:?}");
    }

    #[test]
    fn nested_events_dir_is_also_a_rule_2_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/domain/src/todos/events/published.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 2")), "violations: {v:?}");
    }

    #[test]
    fn services_dir_is_a_rule_3_violation() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/app/src/services/foo.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 3")), "violations: {v:?}");
    }

    #[test]
    fn linked_non_cli_services_tree_cannot_bypass_layout_rules() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed(&external, "services/forbidden.rs");
        let source = dir.path().join("crates/app/src");
        fs::create_dir_all(&source).unwrap();
        let linked = source.join("linked");
        gtl_platform::symlink_dir(&external, &linked).unwrap();

        let violations = collect_violations(dir.path()).unwrap();

        assert_eq!(
            violations,
            vec![format!(
                "[rules 1-3: source layout is bounded] {} — source layout tree may not contain symlinks",
                linked.display()
            )]
        );
    }

    #[test]
    fn linked_non_cli_source_root_is_a_layout_violation() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed(&external, "services/forbidden.rs");
        let member = dir.path().join("crates/app");
        fs::create_dir_all(&member).unwrap();
        let source = member.join("src");
        gtl_platform::symlink_dir(&external, &source).unwrap();

        let violations = collect_violations(dir.path()).unwrap();

        assert_eq!(
            violations,
            vec![format!(
                "[rules 1-3: source layout is bounded] {} — source layout tree may not contain symlinks",
                source.display()
            )]
        );
    }

    #[test]
    fn linked_workspace_member_is_a_layout_violation() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed(&external, "src/services/forbidden.rs");
        let members = dir.path().join("crates");
        fs::create_dir_all(&members).unwrap();
        let member = members.join("app");
        gtl_platform::symlink_dir(&external, &member).unwrap();

        let violations = collect_violations(dir.path()).unwrap();

        assert_eq!(
            violations,
            vec![format!(
                "[rules 1-3: source layout is bounded] {} — source layout tree may not contain symlinks",
                member.display()
            )]
        );
    }

    #[test]
    fn linked_member_bucket_is_a_layout_violation() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed(&external, "app/src/services/forbidden.rs");
        let members = dir.path().join("crates");
        gtl_platform::symlink_dir(&external, &members).unwrap();

        let violations = collect_violations(dir.path()).unwrap();

        assert_eq!(
            violations,
            vec![format!(
                "[rules 1-3: source layout is bounded] {} — source layout tree may not contain symlinks",
                members.display()
            )]
        );
    }

    #[test]
    fn shared_member_dir_is_also_scanned() {
        // The bootstrap crate lives under shared/, not crates/ — the lint must reach it too.
        // A depth-3 dir under a shared/ crate is a rule-1 violation.
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "shared/bootstrap/src/a/b/c/f.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 1")), "violations: {v:?}");
    }

    #[test]
    fn no_member_dirs_returns_empty() {
        let dir = TempDir::new().unwrap();
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty());
    }

    fn rule_5_messages(root: &Path) -> Vec<String> {
        collect_violations(root)
            .unwrap()
            .into_iter()
            .filter(|message| message.contains("rule 5"))
            .collect()
    }

    #[test]
    fn app_state_store_cannot_expose_operation_methods() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/application/src/ports.rs",
            r"
pub trait AppStateStore: Clone + Send + Sync + 'static {
    fn connection_lock(&self) -> anyhow::Result<ConnectionGuard<'_>>;
    fn list_recent_renders(&self) -> anyhow::Result<Vec<RecentRender>>;
}
",
        );

        let violations = rule_5_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(violations[0].contains("AppStateStore"));
        assert!(violations[0].contains("list_recent_renders"));
    }

    #[test]
    fn app_state_store_exposes_only_the_connection_resource() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/application/src/ports.rs",
            r"
pub trait AppStateStore: Clone + Send + Sync + 'static {
    fn connection_lock(
        &self,
    ) -> anyhow::Result<impl std::ops::DerefMut<Target = rusqlite::Connection> + '_>;
}
",
        );

        let violations = rule_5_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn app_state_store_const_generic_block_is_not_the_trait_body() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/application/src/ports.rs",
            r"
pub trait AppStateStore<const CONNECTION_COUNT: usize = { 1 }> {
    fn connection_lock(&self);
}
",
        );

        let violations = rule_5_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn app_state_store_parenthesized_comparison_is_not_a_generic_delimiter() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/application/src/ports.rs",
            r"
pub trait AppStateStore<T = [(); (1 < 2) as usize]> {
    fn connection_lock(&self);
}
",
        );

        let violations = rule_5_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    fn rule_6_messages(root: &Path) -> Vec<String> {
        collect_violations(root)
            .unwrap()
            .into_iter()
            .filter(|message| message.contains("rule 6"))
            .collect()
    }

    #[test]
    fn app_state_infrastructure_cannot_execute_runtime_sql() {
        for operation in [
            "prepare",
            "prepare_cached",
            "query_row",
            "execute",
            "execute_batch",
            "transaction",
            "transaction_with_behavior",
            "unchecked_transaction",
            "unchecked_transaction_with_behavior",
        ] {
            let dir = TempDir::new().unwrap();
            seed_with(
                dir.path(),
                "crates/infra/src/app_state/state.rs",
                &format!("fn persist(connection: &Connection) {{ connection.{operation}(); }}\n"),
            );

            let violations = rule_6_messages(dir.path());

            assert_eq!(
                violations.len(),
                1,
                "operation {operation}, violations: {violations:?}"
            );
            assert!(
                violations[0].contains(operation),
                "operation {operation}, violations: {violations:?}"
            );
        }
    }

    #[test]
    fn app_state_sql_inside_a_trailing_test_module_is_ignored() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/infra/src/app_state/state.rs",
            r#"
pub struct SqliteAppState;

#[cfg(test)]
mod tests {
    fn exercise(connection: &mut Connection) {
        connection.prepare_cached("SELECT 1");
        connection.query_row("SELECT 1", [], |_| Ok(()));
        connection.execute("DELETE FROM recent_renders", []);
        connection.transaction();
    }
}
"#,
        );

        let violations = rule_6_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn app_state_database_initialization_operations_are_allowed() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/infra/src/app_state/db.rs",
            r#"
fn open(path: &Path) {
    let mut connection = Connection::open(path);
    connection.busy_timeout(Duration::from_secs(5));
    connection.pragma_update(None, "journal_mode", "WAL");
    Migrations::to_latest(&mut connection);
}
"#,
        );

        let violations = rule_6_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn real_repo_is_clean() {
        // CARGO_MANIFEST_DIR = xtask/; its parent is the workspace root where crates/ and
        // shared/ live. Anchoring here (not ".") makes the test actually scan crates/*
        // regardless of test CWD.
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask/ must have a parent (the workspace root)");
        let v = collect_violations(workspace_root).unwrap();
        assert!(
            v.is_empty(),
            "the real repo has check-structure violations:\n{}",
            v.join("\n")
        );
    }

    #[test]
    fn exempt_cli_crate_is_skipped() {
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/cli/src/commands/managed/status/mod.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.is_empty(), "cli must be exempt: {v:?}");
    }

    #[test]
    fn non_exempt_crate_with_same_shape_still_violates() {
        let dir = TempDir::new().unwrap();
        seed(
            dir.path(),
            "crates/infra/src/commands/managed/status/mod.rs",
        );
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 1")), "violations: {v:?}");
    }

    #[test]
    fn exempt_cli_crate_still_violates_rule_3() {
        // cli is exempt from rule 1 (max depth) only — a services/ dir must still be caught.
        let dir = TempDir::new().unwrap();
        seed(dir.path(), "crates/cli/src/services/foo.rs");
        let v = collect_violations(dir.path()).unwrap();
        assert!(v.iter().any(|s| s.contains("rule 3")), "violations: {v:?}");
    }

    fn rule_4_messages(root: &Path) -> Vec<String> {
        collect_violations(root)
            .unwrap()
            .into_iter()
            .filter(|message| message.contains("rule 4"))
            .collect()
    }

    #[test]
    fn cli_lib_cannot_execute_through_a_receiver_run_method() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "fn execute(runner: &Runner) { let _ = runner .\n run \n (request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not call a receiver method named run"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_receiver_run_with_turbofish_is_also_forbidden() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "fn execute(runner: &Runner) { let _ = runner.run::<Output>(request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not call a receiver method named run"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_nested_source_cannot_alias_the_git_runner_import() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/transport/nested.rs",
            "use application::ports::GitRunner as RepositoryRunner;\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not reference the application GitRunner port"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_source_cannot_brace_import_the_git_runner() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/client.rs",
            "use application::ports::{GitOutput, GitRunner as Runner};\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not reference the application GitRunner port"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_source_cannot_use_a_qualified_git_runner_call() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/client.rs",
            "fn execute(runner: &Runner) { <Runner as application::ports::GitRunner>::run(runner, request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not reference the application GitRunner port"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_source_cannot_use_the_retired_git_shim_with_whitespace() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/client.rs",
            "use crate /* retired */ :: git;\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not use the retired crate::git shim"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_source_cannot_launch_git_directly() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/client.rs",
            "fn execute() { let _ = std::process::Command::new(\"git\").status(); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not launch git directly"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn cli_source_cannot_launch_git_through_a_command_alias() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/client.rs",
            "use std::process::Command as Process;\nfn execute() { let _ = Process::new(\"git\").status(); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not launch git directly"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn std_git_runner_composition_and_application_execution_are_allowed() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "use infra::git_runner::StdGitRunner;\nfn execute() { operation::execute(Request, &StdGitRunner); }\n",
        );
        seed_with(
            dir.path(),
            "crates/application/src/example.rs",
            "use crate::ports::GitRunner;\nfn execute(git: &impl GitRunner) { let _ = git.run(std::path::Path::new(\".\"), &[\"status\"]); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn comments_strings_and_top_level_test_module_suffix_are_ignored() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            r##"fn render() {
    // runner.run(request); GitRunner crate::git Command::new("git")
    let _ = "GitRunner runner.run(request) crate::git Command::new(\"git\")";
    let _ = r#"GitRunner runner.run(request) crate::git Command::new("git")"#;
}

#[cfg(test)]
mod tests {
    use application::ports::GitRunner;
    fn run(git: &impl GitRunner) { let _ = git.run(std::path::Path::new("."), &["status"]); }
    fn launch() { let _ = std::process::Command::new("git").status(); }
}
"##,
        );

        let violations = rule_4_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn nested_cfg_test_text_does_not_hide_later_production_code() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "fn outer() { const TEXT: &str = \"#[cfg(test)] mod tests { runner.run(request); }\"; }\nfn execute(runner: &Runner) { runner.run(request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
    }

    #[test]
    fn whole_file_cfg_test_source_is_ignored() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/test_support.rs",
            "#![cfg(test)]\nuse application::ports::GitRunner;\nfn run(git: &impl GitRunner) { git.run(request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    #[test]
    fn production_after_top_level_test_module_is_still_scanned() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "#[cfg(test)]\nmod tests { fn test_only() {} }\nfn execute(runner: &Runner) { runner.run(request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not call a receiver method named run"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn macro_inner_attribute_tokens_do_not_mark_the_file_as_test_only() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "swallow!(#![cfg(test)]);\nfn execute(runner: &Runner) { runner.run(request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
        assert!(
            violations[0].contains("may not call a receiver method named run"),
            "violations: {violations:?}"
        );
    }

    #[test]
    fn nested_file_cfg_test_attribute_does_not_hide_later_production_code() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/lib.rs",
            "mod support { #![cfg(test)] fn test_only() {} }\nfn execute(runner: &Runner) { runner.run(request); }\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(violations.len(), 1, "violations: {violations:?}");
    }

    #[test]
    fn rule_4_messages_are_deterministic_by_path_then_kind() {
        let dir = TempDir::new().unwrap();
        seed_with(
            dir.path(),
            "crates/cli/src/z.rs",
            "fn execute(runner: &Runner) { runner.run(request); }\n",
        );
        seed_with(
            dir.path(),
            "crates/cli/src/a.rs",
            "use crate::git;\nuse application::ports::GitRunner;\n",
        );

        let violations = rule_4_messages(dir.path());

        assert_eq!(
            violations,
            vec![
                format!(
                    "[rule 4: CLI operations use application slices] {} — production CLI source may not reference the application GitRunner port",
                    dir.path().join("crates/cli/src/a.rs").display()
                ),
                format!(
                    "[rule 4: CLI operations use application slices] {} — production CLI source may not use the retired crate::git shim",
                    dir.path().join("crates/cli/src/a.rs").display()
                ),
                format!(
                    "[rule 4: CLI operations use application slices] {} — production CLI source may not call a receiver method named run",
                    dir.path().join("crates/cli/src/z.rs").display()
                ),
            ]
        );
    }

    #[test]
    fn cli_source_directory_symlink_is_a_deterministic_rule_4_violation() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed_with(
            &external,
            "forbidden.rs",
            "fn execute(runner: &Runner) { runner.run(request); }\n",
        );
        let cli_source = dir.path().join("crates/cli/src");
        fs::create_dir_all(&cli_source).unwrap();
        gtl_platform::symlink_dir(&external, &cli_source.join("linked")).unwrap();

        let violations = rule_4_messages(dir.path());

        assert_eq!(
            violations,
            vec![format!(
                "[rule 4: CLI operations use application slices] {} — production CLI source tree may not contain symlinks",
                cli_source.join("linked").display()
            )]
        );
    }

    #[test]
    fn layout_walk_does_not_enter_cli_source_symlinks_before_rule_4() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed(&external, "services/forbidden.rs");
        let cli_source = dir.path().join("crates/cli/src");
        fs::create_dir_all(&cli_source).unwrap();
        let linked = cli_source.join("linked");
        gtl_platform::symlink_dir(&external, &linked).unwrap();

        let violations = collect_violations(dir.path()).unwrap();

        assert_eq!(
            violations,
            vec![format!(
                "[rule 4: CLI operations use application slices] {} — production CLI source tree may not contain symlinks",
                linked.display()
            )]
        );
    }

    #[test]
    fn cyclic_cli_source_symlink_is_reported_without_recursing() {
        let dir = TempDir::new().unwrap();
        let cli_source = dir.path().join("crates/cli/src");
        fs::create_dir_all(&cli_source).unwrap();
        let linked = cli_source.join("loop");
        gtl_platform::symlink_dir(Path::new("."), &linked).unwrap();

        let violations = collect_violations(dir.path()).unwrap();

        assert_eq!(
            violations,
            vec![format!(
                "[rule 4: CLI operations use application slices] {} — production CLI source tree may not contain symlinks",
                linked.display()
            )]
        );
    }

    #[test]
    fn cli_source_root_symlink_is_a_deterministic_rule_4_violation() {
        let dir = TempDir::new().unwrap();
        let external = dir.path().join("external");
        seed_with(&external, "lib.rs", "fn render() {}\n");
        let cli_crate = dir.path().join("crates/cli");
        fs::create_dir_all(&cli_crate).unwrap();
        let cli_source = cli_crate.join("src");
        gtl_platform::symlink_dir(&external, &cli_source).unwrap();

        let violations = rule_4_messages(dir.path());

        assert_eq!(
            violations,
            vec![format!(
                "[rule 4: CLI operations use application slices] {} — production CLI source tree may not contain symlinks",
                cli_source.display()
            )]
        );
    }
}
