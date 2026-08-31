#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_dir="${repo_root}/fixtures/synthetic"
validation_dir="$(mktemp -d "${TMPDIR:-/tmp}/clinical-data-review.XXXXXX")"
database_path="${validation_dir}/review.sqlite3"
database_url="sqlite://${database_path}"

cleanup() {
    rm -f "${database_path}" "${database_path}-shm" "${database_path}-wal"
    rmdir "${validation_dir}"
}
trap cleanup EXIT

cd "${repo_root}"
cargo run --quiet -p clinical-data-pipeline -- etl "${fixture_dir}" \
    --mapping "${fixture_dir}/mapping.toml" \
    --database-url "${database_url}" \
    --purge-pii \
    --name-dictionary "${fixture_dir}/names.txt"

cargo run --quiet -p clinical-data-pipeline -- cohort "${fixture_dir}/cohort.txt" \
    --database-url "${database_url}" \
    --cohort-name "Synthetic Review" \
    --tenant-slug example-research-workspace \
    --operator-handle example-reviewer \
    --session-name "Synthetic Session"

VALIDATION_DATABASE="${database_path}" python3 - <<'PY'
import os
import sqlite3

connection = sqlite3.connect(os.environ["VALIDATION_DATABASE"])
expected = {
    "patients": 3,
    "patient_notes": 12,
    "clinical_journal": 3,
    "research_cohorts": 1,
    "research_sessions": 1,
}
for table, count in expected.items():
    actual = connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
    if actual != count:
        raise SystemExit(f"{table}: expected {count} rows, found {actual}")

text = "\n".join(
    row[0]
    for table, column in (("patient_notes", "content"), ("clinical_journal", "content"))
    for row in connection.execute(f"SELECT {column} FROM {table}")
)
for raw_span in (
    "Alex Example",
    "Casey Example",
    "Taylor Synthetic",
    "alex@example.invalid",
    "555-123-4567",
):
    if raw_span in text:
        raise SystemExit(f"unredacted synthetic sensitive span: {raw_span}")
if "[PERSON]" not in text or "[EMAIL]" not in text or "[PHONE]" not in text:
    raise SystemExit("expected typed redaction placeholders were not produced")
print("synthetic end-to-end validation passed")
PY
