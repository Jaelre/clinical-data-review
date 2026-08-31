use platform_db::connection::DatabaseConnection;
use platform_db::DatabaseConnectionType;
use std::path::PathBuf;
use uuid::Uuid;

fn database_url() -> String {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "clinical-platform-sqlite-health-{}.sqlite",
        Uuid::new_v4()
    ));
    format!("sqlite://{}", path.display())
}

#[tokio::test]
async fn test_sqlite_connection() {
    let connection_string = database_url();

    let db = DatabaseConnectionType::new(&connection_string)
        .await
        .expect("Failed to create database connection");
    db.run_migrations()
        .await
        .expect("Failed to apply SQLite migrations");

    let is_healthy = db.health_check().await.expect("Health check failed");

    assert!(is_healthy, "Database health check should return true");

    let version = db
        .get_version()
        .await
        .expect("Failed to get database version");

    println!("Connected to SQLite. Version: {}", version);
    assert!(
        version.contains("SQLite"),
        "Version should contain 'SQLite'"
    );
}
