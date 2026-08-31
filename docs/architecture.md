# Architecture

The repository is one Cargo workspace with a single `Cargo.lock`.

```text
fixtures or approved local files
            |
            v
clinical-data-pipeline --mapping ... --purge-pii
            |
            v
     local SQLite database
            |
            v
review-core -> Tauri commands -> static Vite UI
```

## Workspace boundaries

- `crates/platform-models` defines shared domain records.
- `crates/platform-errors` defines contextual, typed failures.
- `crates/platform-db` owns the SQLite schema and persistence API.
- `crates/review-core` contains review, cohort, and session business logic.
- `apps/review/src-tauri` exposes the local desktop command boundary.
- `apps/review/ui` is the static browser UI bundled by Tauri.
- `tools/data-pipeline` imports mapped workbooks and cohort files.
- `tests/platform-integration` tests the shared SQLite boundary.

SQLite is the only storage implementation. A database URL with another scheme fails explicitly. Each desktop integration test creates, migrates, seeds, and discards an isolated temporary database.

The application does not provide a server, remote authentication, synchronization, telemetry, or a cloud fallback. Adding networked storage would be a new architecture and security decision, not a compatible configuration change.
