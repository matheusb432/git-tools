# Conformance Harness

Run from the repo root:

```powershell
pwsh -NoProfile -File conformance/run-conformance.ps1 -Impl node
pwsh -NoProfile -File conformance/run-conformance.ps1 -Impl rust
```

Shared goldens are frozen from the legacy originals with `-Impl node -Update`. The legacy
Node/PowerShell originals are no longer vendored in sample_project; by default the runner reads
them from sample_project tag `git-tools-node-pre-port`. Override with `-OldToolsRoot`,
`-OldToolsArchiveRepo`, or `-OldToolsArchiveRef` when checking a different archive.
`merge-diff-advanced-base` is rust-only because the legacy node originals do not implement
`merge-diff`.

The runner sets `GIT_TOOLS_NO_OPEN=1`. Rust honors this directly; node fixtures run from temporary
copies of the legacy scripts with only their final `openFile(outFile)` call guarded, so goldens keep
legacy behavior without opening browser or Explorer windows.

Before Task 10 parity fixes, rust is expected to fail only the shared fixtures with CLI text
formatting differences: pluralized `stdout` for HTML commands and legacy error stderr shape
(`not-a-git-repo`, `no-upstream`, and `not-a-commit`).
