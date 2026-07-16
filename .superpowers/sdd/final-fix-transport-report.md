# Final Branch Review Fix: Preserve Remaining Git Transport Errors

## Result

- Base: `5eb8ac62ada982f91e1b38d04cb823d48f552a35`.
- Scope: final branch review Important #1 only; no partial-progress redesign, dependency change, worktree, push, or prune behavior change.
- Expected nonzero Git exits remain the existing closed refused, failed, false, `None`, or symbolic-fallback values.
- Unexpected `GitRunner::run` errors now remain sourced operation errors and retain the exact Git argv label in their display text.

## Shared Git probes

- Added internal checked capture, success, and branch-existence probes under `application::shared::git`.
- Kept the existing infallible `capture` and `onto_exists` wrappers for status paths whose fallback behavior is intentional.
- Added value tests proving nonzero rejection remains `None` or `false` while transport remains `Err(anyhow::Error)`.

## Operation boundaries

- `repository_sync::{plan_push,plan_commit,apply_push,apply_commit}` now each returns `Result<ClosedValue, OperationError>` with an operation-local `#[non_exhaustive]` sourced error.
- `branches::{plan_switch,apply_switch,plan_rebase,apply_rebase,plan_revert,apply_revert}` now uses the same boundary. Branch prune was already correct and remains unchanged.
- Recipe pin probes keep nonzero resolution symbolic, but transport becomes `PinRecipeError::Transport` and propagates through operation-specific managed and recursive builder errors.
- The CLI maps transport to internal exit 1 and stderr as `<command>: git <argv>: <source>` for push, commit, and switch flows. Recipe callers retain their existing top-level behavior.

## TDD evidence

- Shared checked-probe tests first failed to compile because the fallible helpers did not exist, then passed 4/4.
- Each repository-sync and branch transition operation first failed to compile after its test required a sourced `Result` boundary, then passed its focused suite.
- The recipe transport regression failed behaviorally because a scripted transport failure returned an unpinned symbolic recipe; after the fix the recipe suite passed 10/10.
- Existing nonzero cases stayed in the focused suites and continue asserting their exact closed details. No collaborator call-list assertion was added.

## Verification

- `cargo test -p application --lib`: 276 passed.
- `cargo test -p cli --test e2e push -- --nocapture`: 17 passed.
- `cargo test -p cli --test e2e commit -- --nocapture`: 17 passed.
- `cargo test -p cli --test sw_e2e`: 5 passed.
- `cargo test -p cli --test prune_e2e`: 5 passed.
- `cargo test -p infra --test recipes_git`: 7 passed.
- Manual debug-binary smoke with an empty `PATH`: commit, push, and sw each exited 1, wrote no stdout, and printed the exact command-prefixed transport diagnostic on stderr.
- `just fmt-check`: passed warning-clean.
- `just test`: passed default formatting and Rust test scopes.
- `just test --all`: passed complete Rust tests, frontend tests, bundle drift, release builds, and desktop E2E.
- `just cli update`: rebuilt and installed the CLI and daemon; both installed binaries compare byte-identically with their release artifacts.

## Review follow-up

- Clarified the public recipe-pin documentation: nonzero pin probes preserve the symbolic operation, while repository-top and Git transport failures return their respective `PinRecipeError` variants.
- `cargo doc -p application --no-deps` passed with four unrelated pre-existing rustdoc warnings; application recipe tests passed 10/10, real-Git recipe tests passed 7/7, and `just fmt-check` passed.
