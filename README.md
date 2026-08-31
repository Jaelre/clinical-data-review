# Clinical Data Review

Clinical Data Review is a source-available, local-only desktop workflow for importing, organizing, and reviewing research datasets in SQLite. It includes a Rust/Tauri review application and an explicit-mapping ETL command.

> **Research use only.** This software is not clinically validated, is not a medical device, and must not be used to make clinical decisions. Running it does not make a workflow compliant with GDPR, HIPAA, or any other health-data rule. You are responsible for authorization, governance, security, validation, and lawful use.

## What is included

- One Cargo workspace and lockfile for seven Rust packages.
- SQLite as the only supported database backend.
- A local Tauri application with operator selection and no password authentication.
- An ETL command that requires an explicit neutral TOML mapping.
- Tiny, fictional `SYNTH-*` fixtures for tests and demonstrations.

There is no hosted service, telemetry, cloud database integration, or automatic upload path.

## Quick start with synthetic data

Requirements: a current stable Rust toolchain, Node.js 20.19 or newer, npm, Python 3, and the platform dependencies required by Tauri.

```sh
cargo run -p clinical-data-pipeline -- etl fixtures/synthetic \
  --mapping fixtures/synthetic/mapping.toml \
  --database-url sqlite://./data/clinical-data-review.sqlite3 \
  --purge-pii \
  --name-dictionary fixtures/synthetic/names.txt

cargo run -p clinical-data-pipeline -- cohort fixtures/synthetic/cohort.txt \
  --database-url sqlite://./data/clinical-data-review.sqlite3 \
  --cohort-name "Synthetic Review" \
  --tenant-slug example-research-workspace \
  --operator-handle example-reviewer \
  --session-name "Synthetic Session"

npm --prefix apps/review ci
DATABASE_URL="sqlite://${PWD}/data/clinical-data-review.sqlite3" \
  npm --prefix apps/review run tauri dev
```

For a platform installer, run `npm --prefix apps/review run bundle`. This generates ignored native icons from the tracked SVG before invoking Tauri.

PII purging is intentionally opt-in at the command line. The ETL prints a conspicuous warning when it is disabled. Published examples enable it, but automated redaction is not a guarantee of de-identification; inspect outputs under an approved protocol.

## Development checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
npm --prefix apps/review ci
npm --prefix apps/review run build
python3 scripts/check-synthetic-workbooks.py
scripts/check-public-tree.sh
```

See [architecture](docs/architecture.md), [data mapping](docs/data-mapping.md), [privacy](docs/privacy.md), [fixture provenance](fixtures/synthetic/README.md), [contributing](CONTRIBUTING.md), and [security](SECURITY.md).

## License

Copyright 2026 Arturo de Buoi.

This project is **source-available**, not open source. Noncommercial use is licensed under [PolyForm Noncommercial 1.0.0](LICENSE). Commercial use requires a [separate written agreement](COMMERCIAL-LICENSE.md). The commercial licensing contact is `jaelre(at)gmail(dot)com`.
