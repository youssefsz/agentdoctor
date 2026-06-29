## Summary

<!-- What changed and why? -->

## Verification

<!-- List commands run. If a check was not run, explain why. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --workspace --release`

## Checklist

- [ ] I kept engine logic out of the CLI crate.
- [ ] I added or updated tests for behavior changes.
- [ ] I updated docs or examples when user-facing behavior changed.
- [ ] I did not add network calls, telemetry, AI APIs, or heavy dependencies without discussion.
