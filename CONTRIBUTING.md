# Contributing

Thank you for helping improve Clinical Data Review. By contributing, you agree to follow these requirements.

## Data safety

Never submit PII/PHI, real clinical records, private cohort or patient lists, production databases, credentials, environment files, private logs, screenshots of real data, proprietary source headers, or files derived from a real workbook. Reproduce issues only with minimal synthetic data using `SYNTH-*` identifiers and reserved `.invalid` addresses.

## Contribution license

The project itself remains licensed under PolyForm Noncommercial 1.0.0. Unless you conspicuously state otherwise before submission, you grant the project and recipients an inbound license to your contribution under the Apache License 2.0, whose complete text is in [`LICENSES/Apache-2.0.txt`](LICENSES/Apache-2.0.txt). This inbound grant does not relicense the project as a whole.

You represent that you have the right to submit the contribution and any included assets.

## Developer Certificate of Origin

Every commit must include a `Signed-off-by` trailer certifying the [Developer Certificate of Origin 1.1](https://developercertificate.org/). Sign a commit with:

```sh
git commit -s
```

The sign-off uses your real name and an email address you are authorized to use. It is a certification of provenance, not a cryptographic signature.

## Workflow

Open an issue for substantial behavioral or schema changes. Keep each pull request focused, describe privacy and migration impact, and include tests. New failure handling must be explicit and diagnosable; do not silently recover or add compatibility fallbacks.

Before opening a pull request, run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
npm --prefix apps/review ci
npm --prefix apps/review run build
python3 scripts/check-synthetic-workbooks.py
scripts/check-public-tree.sh
```

UI changes should include synthetic-data screenshots when useful. Never include a screenshot containing real or plausibly real personal data.
