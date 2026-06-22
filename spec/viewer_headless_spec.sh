# shellcheck shell=bash
# Contract: viewer=app (the default) is NON-FATAL when there is no display.
# With DISPLAY and WAYLAND_DISPLAY unset the CLI must:
#   - exit 0
#   - print "wrote" on stdout (artifact written to the central store)
#   - leave the repo working tree clean (no .artifacts/ or other junk)

Describe 'viewer=app headless'
  setup() {
    REPO=$(mktemp -d)
    GIT_TOOLS_DATA_DIR=$(mktemp -d)
    export GIT_TOOLS_DATA_DIR
    # Suppress any open attempt — the no-display fallback lands on browser, which
    # respects this guard; nothing actually opens.
    export GIT_TOOLS_NO_OPEN=1
    # Ensure viewer=app is the active config regardless of the user's local config.
    export GIT_TOOLS_CONFIG=/dev/null
    # Remove display vars so viewer=app degrades to the browser path (non-fatal).
    unset DISPLAY WAYLAND_DISPLAY

    # Build a minimal two-commit repo so HEAD~1..HEAD has real content.
    git -C "$REPO" init -q
    git -C "$REPO" config user.email "test@example.com"
    git -C "$REPO" config user.name "Test"
    printf 'hello\n' >"$REPO/file.txt"
    git -C "$REPO" add file.txt
    git -C "$REPO" commit -q -m "initial"
    printf 'world\n' >>"$REPO/file.txt"
    git -C "$REPO" add file.txt
    git -C "$REPO" commit -q -m "second"
  }
  cleanup() {
    rm -rf "$REPO" "$GIT_TOOLS_DATA_DIR"
    unset GIT_TOOLS_DATA_DIR GIT_TOOLS_NO_OPEN GIT_TOOLS_CONFIG
  }
  BeforeEach 'setup'
  AfterEach 'cleanup'

  # Runs gtl diff -l 1 from inside the temp repo; used with `When run` (subshell).
  run_diff_in_repo() {
    cd "$REPO" || return 1
    "${SHELLSPEC_PROJECT_ROOT}/target/release/git-tools" diff -l 1
  }

  # Returns the git porcelain status output for $REPO; empty string means clean.
  repo_porcelain_status() {
    git -C "$REPO" status --porcelain
  }

  It 'writes to the store and never pollutes the repo when no display'
    When run run_diff_in_repo
    The status should be success
    The output should include "wrote"
    # shellcheck disable=SC2154
    The path "$REPO/.artifacts" should not be exist
  End

  It 'leaves the repo working tree clean'
    When run run_diff_in_repo
    The result of function repo_porcelain_status should equal ""
  End
End
