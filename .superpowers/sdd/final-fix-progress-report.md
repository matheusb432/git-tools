# Final Branch Review Fix: Preserve Completed Git Operation Progress

## Result

- Base: `c50253987fa2762ab70517d4b42f47b9fbdebaf2`.
- Scope: final branch review Important #2 only; no CLI text, JSON shape, stdout/stderr routing, exit-code, refspec, or dependency change.
- Closed nonzero Git outcomes retain their exact existing detail strings. Unexpected Git transport errors retain their exact display text and original source.

## Repository sync and managed commit

- Current-repository commit and push results now carry `CommitProgress` (`Unchanged`, `Staged`, or `Created { identity }`); push adds an explicit remote-push completion bit.
- Successful commit output is parsed without a second Git probe. Creation remains truthful even when identity parsing returns `None`.
- The shared parser scans summary candidates from the end, rejects one- and two-field bracketed hook noise before the real summary, accepts ANSI-colored Git summaries, and is reused by managed commit.
- Managed `CommitResult` now reports whether `git add -A` completed; commit rejection and transport failure preserve that staged state without changing managed JSON/table projection.

## Branch transitions

- Rebase results and transport errors carry whether the target switch completed, whether promoted commits are known, whether the fast-forward completed, and recovery data after a partial switch.
- A rejected promoted-commit log remains the legacy closed fallback and renders the same `+0 commits` detail; transport remains sourced.
- Revert results and transport errors carry whether `git switch -` and the force move completed.
- Partial rebase/revert failures include the original branch and exact `git switch <branch>` recovery command.

## Tags

- Tag action outcomes and operation errors carry fully qualified created refs, remote-push state, updated tracking refs, and tracking refs still pending.
- Remote push is explicit as `NotStarted`, `Completed { pushed_refs }`, or `Indeterminate { attempted_refs }`; a rejected or transport-failed multi-ref push therefore never claims that no remote mutation occurred.
- Successful remote push is recorded before local tracking updates. A later nonzero or transport failure therefore names the exact `refs/remotes/origin/tags/...` refs requiring recovery.
- Add, add-and-push, label, and push share the same unchanged progress type; validation and pre-mutation failures carry empty progress.

## TDD evidence

- Initial progress assertions failed compilation on the missing result/error fields and responsibility-named progress types.
- Commit-summary regressions failed behaviorally by selecting hexadecimal hook noise; the parser now scans from the end and selects the later Git summary identity.
- Fully qualified tag-ref assertions failed against bare names before ref construction moved into `TagOperationProgress`.
- Re-review found that a nonzero multi-ref push could partially update the remote; the new indeterminate state was required by failing closed-outcome and transport-error assertions.
- Value tests script Git outcomes and assert returned progress plus original sources; no collaborator call-list assertion was added.

## Verification

- `cargo test -p application --lib`: 286 passed.
- `cargo test -p cli --test e2e push -- --nocapture`: 17 passed.
- `cargo test -p cli --test e2e commit -- --nocapture`: 17 passed.
- `cargo test -p cli --test e2e tag -- --nocapture`: 16 passed.
- `cargo test -p cli --test sw_e2e`: 5 passed.
- `cargo test -p cli commands::managed::commit -- --nocapture`: 5 passed.
- `just fmt-check`: passed warning-clean.
- `just test`: passed the default formatting and Rust test scopes.
- `just test --all`: passed complete Rust tests, frontend tests, bundle drift, release builds, and desktop E2E.

## Follow-up review: commit identity stream boundary

- `created_commit_identity` now accepts `GitOutput` and scans only stdout, selecting the last valid Git summary there. Hook diagnostics and other stderr text cannot become a reported commit identity.
- Current-repository commit, commit-and-push, and managed commit pass the captured Git result directly. Managed user-facing detail still combines both streams independently of identity parsing.
- The pure stream regression separates a real stdout summary from `[hook deadbeef]` on stderr. A real temporary-Git regression installs a `post-commit` hook that emits the same diagnostic and proves both current-repository and managed identities equal `git rev-parse --short HEAD`.
- TDD evidence: before the fix, the real-Git regression reported `Some("deadbeef")` instead of the actual abbreviated HEAD; after the stdout-only boundary, the focused application, managed, and real-Git tests pass without interaction assertions.
- Public `PromotedCommit`, `sha`, and `subject` now carry rustdoc; `cargo doc -p application --no-deps` passes with four pre-existing unrelated warnings.
- Follow-up verification: `just fmt-check`, `just test`, and `just test --all` all pass, including the complete Rust, frontend, bundle-drift, release-build, and desktop-E2E scopes.
