# Desktop scroll fixture

This immutable synthetic Git workload reconstructs one base commit followed by exactly ten measured commits. The base snapshot retains full source context, and `patches/` contains the compact applyable commit series.

Regenerate it only with `just bench-scroll-fixture-update`. The command runs under repository-owned resource bounds, generates two independent candidates, requires byte-for-byte equality, hydrates the result without network access, and verifies `manifest.toml` before replacing these files.
