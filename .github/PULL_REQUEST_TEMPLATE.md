## Summary

Describe the focused change and why it is needed.

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `npm --prefix apps/review ci && npm --prefix apps/review run build`
- [ ] `python3 scripts/check-synthetic-workbooks.py`
- [ ] `scripts/check-public-tree.sh`

## Privacy and safety

- [ ] This change contains no PII/PHI, real clinical data, credentials, private logs, private schemas, or identifying screenshots.
- [ ] Fixtures use only fictional `SYNTH-*` identifiers and reserved `.invalid` addresses.
- [ ] I described data, schema, migration, security, and clinical-safety impact.
- [ ] Every commit includes a DCO `Signed-off-by` trailer.
