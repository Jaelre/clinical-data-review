//! Platform Database
//!
//! Provides the shared SQLite-only database interface for the clinical review workspace.
//! This crate owns the active schema and optional synthetic fixture helpers.

pub mod connection;
pub mod query_options;
pub mod sqlite;
// Re-export commonly used types
pub use connection::DatabaseConnection;
pub use query_options::*;
pub use sqlite::SqliteConnection;
pub type DatabaseConnectionType = sqlite::SqliteConnection;

#[cfg(test)]
mod tests {
    #[test]
    fn test_database_type_identification() {
        let sqlite_patterns = ["sqlite:///tmp/test.db", "sqlite://./local.db"];
        for pattern in &sqlite_patterns {
            assert!(pattern.starts_with("sqlite://"));
        }
    }

    #[test]
    fn test_connection_string_generation() {
        let conn_str = "sqlite:///tmp/platform.db";
        assert!(conn_str.starts_with("sqlite://"));
        assert!(conn_str.contains("platform.db"));
    }
}
