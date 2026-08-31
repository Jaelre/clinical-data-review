//! Integration tests for the clinical platform core
//!
//! These tests verify that all components work together correctly,
//! including database operations, migrations, and cross-crate functionality.

use chrono::Utc;
use platform_db::{DatabaseConnection, DatabaseConnectionType};
use platform_errors::Result;
use platform_models::*;
use std::path::PathBuf;
use uuid::Uuid;

/// Test that the workspace structure is correct
#[test]
fn test_workspace_structure() {
    // Verify that all expected crates are accessible
    let _ = platform_models::Tenant {
        id: Uuid::new_v4(),
        name: "Test".to_string(),
        slug: "test".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let error = platform_errors::PlatformError::not_found("test", "123");
    assert!(format!("{}", error).contains("not found"));
}

/// Test model serialization and deserialization
#[test]
fn test_model_serialization() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let tenant = Tenant {
        id: Uuid::new_v4(),
        name: "Example Research Workspace".to_string(),
        slug: "example-research-workspace".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Test JSON serialization
    let json = serde_json::to_string(&tenant)?;
    let deserialized: Tenant = serde_json::from_str(&json)?;

    assert_eq!(tenant.id, deserialized.id);
    assert_eq!(tenant.name, deserialized.name);
    assert_eq!(tenant.slug, deserialized.slug);

    Ok(())
}

/// Test error type conversions
#[test]
fn test_error_conversions() {
    // Test std::io::Error conversion
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
    let platform_error: platform_errors::PlatformError = io_error.into();

    match platform_error {
        platform_errors::PlatformError::DataAccessError { message, .. } => {
            assert!(message.contains("I/O operation failed"));
        }
        _ => panic!("Expected DataAccessError"),
    }

    // Test UUID error conversion
    let uuid_result = Uuid::parse_str("invalid-uuid");
    assert!(uuid_result.is_err());

    if let Err(uuid_error) = uuid_result {
        let platform_error: platform_errors::PlatformError = uuid_error.into();
        match platform_error {
            platform_errors::PlatformError::InvalidInput { message, .. } => {
                assert!(message.contains("Invalid UUID"));
            }
            _ => panic!("Expected InvalidInput"),
        }
    }
}

/// Test that all required model fields are present
#[test]
fn test_model_completeness() {
    // Test Patient model has all required fields
    let patient = Patient {
        id: Uuid::new_v4(),
        external_id: "PAT001".to_string(),
        age: Some(45),
        sex: Some("M".to_string()),
        tenant_id: Uuid::new_v4(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    assert!(!patient.external_id.is_empty());
    assert!(patient.age.is_some());
    assert!(patient.sex.is_some());

    // Test ClinicalJournalEntry model
    let entry = ClinicalJournalEntry {
        id: Uuid::new_v4(),
        patient_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        entry_timestamp: Utc::now(),
        entry_sequence: 1,
        role: Some("doctor".to_string()),
        content: "Patient examination notes".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    assert!(entry.entry_sequence > 0);
    assert!(!entry.content.is_empty());
}

/// Test database connection string parsing
#[tokio::test]
async fn test_connection_string_variants() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let sqlite_patterns = [
        format!("sqlite://{}", directory.path().join("first.db").display()),
        format!("sqlite://{}", directory.path().join("second.db").display()),
    ];

    for pattern in &sqlite_patterns {
        let result = DatabaseConnectionType::new(pattern).await;
        assert!(
            result.is_ok(),
            "sqlite URLs should be accepted: {}",
            pattern
        );
    }

    let invalid_patterns = ["https://example.invalid/database", "invalid-format"];

    for pattern in &invalid_patterns {
        let result = DatabaseConnectionType::new(pattern).await;
        assert!(result.is_err());
        if let Err(e) = result {
            match &e {
                platform_errors::PlatformError::ConfigError { .. } => {
                    // Expected for unsupported database types
                }
                _ => {
                    panic!("Unexpected error type for invalid pattern: {}", pattern);
                }
            }
        }
    }
}

fn test_database_url(test_name: &str) -> String {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "clinical-platform-{test_name}-{}.sqlite",
        Uuid::new_v4()
    ));
    format!("sqlite://{}", path.display())
}

#[tokio::test]
async fn test_sqlite_bootstrap_and_seed_data() -> Result<()> {
    let db = DatabaseConnectionType::new(&test_database_url("bootstrap")).await?;
    db.run_migrations().await?;
    db.seed_synthetic_fixture_data().await?;

    let tenant = db.get_tenant_by_slug("example-research-workspace").await?;
    let reviewer_id = Uuid::parse_str("00000000-0000-0000-0000-000000000003")?;
    let reviewer = db.get_user_by_id(reviewer_id).await?;
    let roles = db.get_user_roles_for_tenant(reviewer.id, tenant.id).await?;

    assert_eq!(tenant.slug, "example-research-workspace");
    assert!(roles.iter().any(|role| role.role == "reviewer"));

    Ok(())
}

#[tokio::test]
async fn test_local_operator_receives_seeded_cohort_access_and_session_lifecycle() -> Result<()> {
    let db = DatabaseConnectionType::new(&test_database_url("local-operator")).await?;
    db.run_migrations().await?;
    db.seed_synthetic_fixture_data().await?;

    let tenant = db.get_tenant_by_slug("example-research-workspace").await?;
    let seeded_reviewer = db
        .get_user_by_email("example-reviewer@example.invalid")
        .await?;
    let seeded_cohorts = db
        .get_research_cohorts_for_user(tenant.id, seeded_reviewer.id)
        .await?;
    assert!(
        !seeded_cohorts.is_empty(),
        "expected the SQLite bootstrap data to include at least one cohort"
    );

    let operator = db
        .create_local_operator(
            tenant.id,
            "SQLite Regression Reviewer",
            Some("sqlite-regression-reviewer"),
            None,
            "reviewer",
        )
        .await?;
    let operator_cohorts = db
        .get_research_cohorts_for_user(tenant.id, operator.id)
        .await?;
    assert_eq!(
        operator_cohorts.len(),
        seeded_cohorts.len(),
        "newly created local operators should inherit access to existing cohorts"
    );

    let session = db
        .start_local_work_session(tenant.id, operator.id, Some("Regression session"))
        .await?;
    let active_session = db
        .get_active_local_work_session(tenant.id, operator.id)
        .await?;
    assert_eq!(active_session.map(|entry| entry.id), Some(session.id));

    let completed_session = db.end_local_work_session(tenant.id, session.id).await?;
    assert_eq!(completed_session.status, "completed");
    assert!(
        db.get_active_local_work_session(tenant.id, operator.id)
            .await?
            .is_none(),
        "ending a local work session should clear the active session"
    );

    Ok(())
}

#[tokio::test]
async fn test_create_local_operator_rejects_blank_display_name() -> Result<()> {
    let db = DatabaseConnectionType::new(&test_database_url("invalid-operator")).await?;
    db.run_migrations().await?;
    db.seed_synthetic_fixture_data().await?;

    let tenant = db.get_tenant_by_slug("example-research-workspace").await?;
    let error = db
        .create_local_operator(tenant.id, "   ", None, None, "reviewer")
        .await
        .expect_err("blank display names must be rejected");

    match error {
        platform_errors::PlatformError::InvalidInput { field, .. } => {
            assert_eq!(field, Some("display_name".to_string()));
        }
        other => panic!("unexpected error type: {other}"),
    }

    Ok(())
}
