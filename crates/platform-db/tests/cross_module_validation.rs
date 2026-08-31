use platform_db::{DatabaseConnection, DatabaseConnectionType, PatientFilterOptions};
use std::path::PathBuf;
use uuid::Uuid;

fn database_url() -> String {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "clinical-platform-cross-module-{}.sqlite",
        Uuid::new_v4()
    ));
    format!("sqlite://{}", path.display())
}

#[tokio::test]
async fn synthetic_seed_is_isolated_and_reviewable() {
    let db = DatabaseConnectionType::new(&database_url())
        .await
        .expect("failed to connect to local SQLite database");
    db.run_migrations()
        .await
        .expect("failed to apply SQLite schema");
    db.seed_synthetic_fixture_data()
        .await
        .expect("failed to seed synthetic fixtures");

    let tenant = db
        .get_tenant_by_slug("example-research-workspace")
        .await
        .expect("synthetic workspace should exist");
    let patient_count = db
        .get_patients_count(tenant.id, &PatientFilterOptions::default())
        .await
        .expect("failed to count synthetic patients");
    assert_eq!(patient_count, 3);

    let reviewer_id =
        Uuid::parse_str("00000000-0000-0000-0000-000000000003").expect("valid reviewer UUID");
    let reviewer_roles = db
        .get_user_roles_for_tenant(reviewer_id, tenant.id)
        .await
        .expect("failed to load reviewer roles");
    assert!(reviewer_roles.iter().any(|role| role.role == "reviewer"));

    let cohorts = db
        .get_research_cohorts_for_user(tenant.id, reviewer_id)
        .await
        .expect("failed to load reviewer cohorts");
    assert_eq!(cohorts.len(), 1);
    assert_eq!(cohorts[0].total_patients, 3);
}
