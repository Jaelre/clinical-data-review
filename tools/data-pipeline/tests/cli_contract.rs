use predicates::prelude::*;

fn fixture_path(relative: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn etl_without_purging_prints_a_conspicuous_warning() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let database_url = format!(
        "sqlite://{}",
        directory.path().join("unpurged.sqlite3").display()
    );
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("clinical-data-pipeline");

    command
        .args([
            "etl",
            &fixture_path(""),
            "--mapping",
            &fixture_path("mapping.toml"),
            "--database-url",
            &database_url,
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("PII PURGING IS DISABLED"));
}

#[test]
fn etl_rejects_unsupported_database_schemes() {
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("clinical-data-pipeline");

    command
        .args([
            "etl",
            &fixture_path(""),
            "--mapping",
            &fixture_path("mapping.toml"),
            "--database-url",
            "https://example.invalid/database",
            "--purge-pii",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only sqlite:// URLs are accepted"));
}
