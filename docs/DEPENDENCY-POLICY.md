# Dependency and release policy

Latest stable, always. This is not a preference; it is the rule for this project.

## Pinning and updating

- Every dependency is pinned to the latest stable release available when it is
  added. Pre-releases are not used in the reference implementation.
- `Cargo.lock` is committed. Applications get reproducible builds; updates are
  deliberate, reviewed changes, never silent drift.
- Updates land on a short cadence. Security advisories take priority:
  - Critical or high severity: patch or mitigate the same day.
  - Medium: within 7 days.
  - Low and routine version bumps: weekly.
- Deprecated or archived crates are not added. If one becomes archived, it is
  replaced or the risk is documented in an issue immediately.

## CI gates

Every PR runs:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --exclude callcraft-app --all-targets -- -D warnings`
- `cargo test --workspace --exclude callcraft-app`
- `cargo audit` (RustSec advisories)
- `cargo deny check` (advisories, licenses, sources, wildcard bans)
- Windows build of the Tauri shell (`cargo build -p callcraft-app`)

GitHub Actions are pinned by commit SHA, not floating tags.

## Releases

- Binaries are built in CI from a tagged commit with a committed lockfile.
- Each release records its dependency versions (`cargo tree` output attached to
  the release) so an agency can answer "what is inside the binary we were given."
- Code signing is required before distribution to a comm center. Unsigned
  Windows executables hit SmartScreen, AppLocker and WDAC.

## What is deliberately not in the dependency tree

- No Node.js toolchain. The UI is static HTML/CSS/JS served by the Rust binary.
- No in-process speech or model runtimes in v0.1. STT, TTS and the caller LLM
  are out-of-process adapters behind loopback HTTP, which keeps the MIT core
  clean and the supply chain small.
- No telemetry.
