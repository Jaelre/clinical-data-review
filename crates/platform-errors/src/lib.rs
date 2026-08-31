//! Platform Errors
//!
//! Shared error types for consistent error handling across the clinical platform.

use serde::Serialize;
use thiserror::Error;

pub const PATIENT_DETAILS_RATE_LIMIT_MESSAGE: &str =
    "Requests too frequent. Please wait before requesting another patient.";

/// The main error type for the platform, encompassing all possible error conditions
#[derive(Error, Debug, Serialize)]
pub enum PlatformError {
    /// Database access errors (connection issues, query failures, etc.)
    #[error("Database access error: {message}")]
    DataAccessError {
        /// Human-readable error message
        message: String,
        /// Optional source error for debugging
        #[serde(skip)]
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Input validation errors (malformed data, constraint violations, etc.)
    #[error("Invalid input: {message}")]
    InvalidInput {
        /// Description of what input was invalid
        message: String,
        /// Optional field name that caused the error
        field: Option<String>,
    },

    /// Resource not found errors (missing records, files, etc.)
    #[error("Resource not found: {resource_type} with identifier '{identifier}'")]
    NotFound {
        /// Type of resource that wasn't found
        resource_type: String,
        /// The identifier that was used to search
        identifier: String,
    },

    /// Configuration errors (missing environment variables, invalid settings, etc.)
    #[error("Configuration error: {message}")]
    ConfigError {
        /// Description of the configuration issue
        message: String,
        /// Optional configuration key that caused the issue
        key: Option<String>,
    },

    /// Database migration errors
    #[error("Migration error: {message}")]
    MigrationError {
        /// Description of the migration issue
        message: String,
        /// Optional migration version that failed
        version: Option<String>,
    },

    /// Business rule conflict errors (409 Conflict)
    #[error("Conflict: {message}")]
    Conflict {
        /// Description of the conflict
        message: String,
        /// Optional details about what caused the conflict
        details: Option<String>,
    },

    /// Unprocessable entity errors (422 Unprocessable Entity)
    #[error("Cannot process: {message}")]
    UnprocessableEntity {
        /// Description of why the entity cannot be processed
        message: String,
        /// Optional reason code for the processing failure
        reason: Option<String>,
    },

    /// Authentication errors
    #[error("Authentication failed: {message}")]
    AuthenticationError {
        /// Description of the authentication failure
        message: String,
    },
}

impl PlatformError {
    /// Create a new DataAccessError with a message
    pub fn data_access<S: Into<String>>(message: S) -> Self {
        Self::DataAccessError {
            message: message.into(),
            source: None,
        }
    }

    /// Create a new DataAccessError with a message and source
    pub fn data_access_with_source<S: Into<String>, E>(message: S, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::DataAccessError {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a new InvalidInput error
    pub fn invalid_input<S: Into<String>>(message: S) -> Self {
        Self::InvalidInput {
            message: message.into(),
            field: None,
        }
    }

    /// Create a new InvalidInput error with field information
    pub fn invalid_input_field<S: Into<String>, F: Into<String>>(message: S, field: F) -> Self {
        Self::InvalidInput {
            message: message.into(),
            field: Some(field.into()),
        }
    }

    /// Create the canonical patient-details rate-limit error.
    pub fn patient_details_rate_limited() -> Self {
        Self::invalid_input(PATIENT_DETAILS_RATE_LIMIT_MESSAGE)
    }

    /// Create a new NotFound error
    pub fn not_found<R: Into<String>, I: Into<String>>(resource_type: R, identifier: I) -> Self {
        Self::NotFound {
            resource_type: resource_type.into(),
            identifier: identifier.into(),
        }
    }

    /// Create a new ConfigError
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::ConfigError {
            message: message.into(),
            key: None,
        }
    }

    /// Create a new ConfigError with key information
    pub fn config_key<S: Into<String>, K: Into<String>>(message: S, key: K) -> Self {
        Self::ConfigError {
            message: message.into(),
            key: Some(key.into()),
        }
    }

    /// Create a new MigrationError
    pub fn migration<S: Into<String>>(message: S) -> Self {
        Self::MigrationError {
            message: message.into(),
            version: None,
        }
    }

    /// Create a new MigrationError with version information
    pub fn migration_version<S: Into<String>, V: Into<String>>(message: S, version: V) -> Self {
        Self::MigrationError {
            message: message.into(),
            version: Some(version.into()),
        }
    }

    /// Create a new Conflict error
    pub fn conflict<S: Into<String>>(message: S) -> Self {
        Self::Conflict {
            message: message.into(),
            details: None,
        }
    }

    /// Create a new Conflict error with details
    pub fn conflict_with_details<S: Into<String>, D: Into<String>>(message: S, details: D) -> Self {
        Self::Conflict {
            message: message.into(),
            details: Some(details.into()),
        }
    }

    /// Create a new UnprocessableEntity error
    pub fn unprocessable_entity<S: Into<String>>(message: S) -> Self {
        Self::UnprocessableEntity {
            message: message.into(),
            reason: None,
        }
    }

    /// Create a new UnprocessableEntity error with reason
    pub fn unprocessable_entity_with_reason<S: Into<String>, R: Into<String>>(
        message: S,
        reason: R,
    ) -> Self {
        Self::UnprocessableEntity {
            message: message.into(),
            reason: Some(reason.into()),
        }
    }

    /// Create a new AuthenticationError
    pub fn authentication<S: Into<String>>(message: S) -> Self {
        Self::AuthenticationError {
            message: message.into(),
        }
    }
}

/// Convenience Result type alias for the platform
pub type Result<T> = std::result::Result<T, PlatformError>;

// Implement From for common error types

impl From<std::io::Error> for PlatformError {
    fn from(err: std::io::Error) -> Self {
        Self::data_access_with_source("I/O operation failed", err)
    }
}

impl From<sqlx::Error> for PlatformError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Self::not_found("database record", "query result"),
            sqlx::Error::Database(db_err) => {
                Self::data_access_with_source(format!("Database error: {db_err}"), err)
            }
            sqlx::Error::Io(io_err) => {
                Self::data_access_with_source(format!("Database I/O error: {io_err}"), err)
            }
            sqlx::Error::Configuration(msg) => {
                Self::config(format!("Database configuration error: {msg}"))
            }
            _ => Self::data_access_with_source("Database operation failed", err),
        }
    }
}

impl From<sqlx::migrate::MigrateError> for PlatformError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Self::migration(format!("Migration failed: {err}"))
    }
}

impl From<uuid::Error> for PlatformError {
    fn from(err: uuid::Error) -> Self {
        Self::invalid_input(format!("Invalid UUID: {err}"))
    }
}

impl From<serde_json::Error> for PlatformError {
    fn from(err: serde_json::Error) -> Self {
        Self::invalid_input(format!("JSON parsing error: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = PlatformError::not_found("user", "123");
        assert_eq!(
            format!("{err}"),
            "Resource not found: user with identifier '123'"
        );
    }

    #[test]
    fn test_error_serialization() {
        let err = PlatformError::invalid_input_field("Age must be positive", "age");
        let serialized = serde_json::to_string(&err).unwrap();
        println!("Serialized error: {serialized}");
        assert!(serialized.contains("InvalidInput"));
        assert!(serialized.contains("age"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let platform_err: PlatformError = io_err.into();

        match platform_err {
            PlatformError::DataAccessError { message, .. } => {
                assert!(message.contains("I/O operation failed"));
            }
            _ => panic!("Expected DataAccessError"),
        }
    }

    #[test]
    fn test_result_type_alias() {
        fn example_function() -> Result<String> {
            Ok("success".to_string())
        }

        let result = example_function();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_helper_methods() {
        let err1 = PlatformError::data_access("Connection failed");
        let err2 = PlatformError::invalid_input_field("Invalid email", "email");
        let err3 = PlatformError::config_key("Missing API key", "API_KEY");

        match err1 {
            PlatformError::DataAccessError { message, .. } => {
                assert_eq!(message, "Connection failed");
            }
            _ => panic!("Expected DataAccessError"),
        }

        match err2 {
            PlatformError::InvalidInput { message, field } => {
                assert_eq!(message, "Invalid email");
                assert_eq!(field.unwrap(), "email");
            }
            _ => panic!("Expected InvalidInput"),
        }

        match err3 {
            PlatformError::ConfigError { message, key } => {
                assert_eq!(message, "Missing API key");
                assert_eq!(key.unwrap(), "API_KEY");
            }
            _ => panic!("Expected ConfigError"),
        }
    }

    #[test]
    fn test_patient_details_rate_limit_error_helper() {
        let err = PlatformError::patient_details_rate_limited();

        match err {
            PlatformError::InvalidInput { message, field } => {
                assert_eq!(message, PATIENT_DETAILS_RATE_LIMIT_MESSAGE);
                assert!(field.is_none());
            }
            _ => panic!("Expected InvalidInput"),
        }
    }

    #[test]
    fn test_conflict_error() {
        let err1 = PlatformError::conflict("Resource is locked");
        let err2 = PlatformError::conflict_with_details(
            "Chunk not complete",
            "5 unjudged patients remaining",
        );

        // Test error formatting before consuming in match
        let err1_formatted = format!("{}", err1);

        match err1 {
            PlatformError::Conflict { message, details } => {
                assert_eq!(message, "Resource is locked");
                assert!(details.is_none());
            }
            _ => panic!("Expected Conflict"),
        }

        match err2 {
            PlatformError::Conflict { message, details } => {
                assert_eq!(message, "Chunk not complete");
                assert_eq!(details.unwrap(), "5 unjudged patients remaining");
            }
            _ => panic!("Expected Conflict"),
        }

        assert_eq!(err1_formatted, "Conflict: Resource is locked");
    }

    #[test]
    fn test_unprocessable_entity_error() {
        let err1 = PlatformError::unprocessable_entity("Session is finished");
        let err2 = PlatformError::unprocessable_entity_with_reason(
            "Cannot process",
            "final_chunk_complete",
        );

        // Test error formatting before consuming in match
        let err1_formatted = format!("{}", err1);

        match err1 {
            PlatformError::UnprocessableEntity { message, reason } => {
                assert_eq!(message, "Session is finished");
                assert!(reason.is_none());
            }
            _ => panic!("Expected UnprocessableEntity"),
        }

        match err2 {
            PlatformError::UnprocessableEntity { message, reason } => {
                assert_eq!(message, "Cannot process");
                assert_eq!(reason.unwrap(), "final_chunk_complete");
            }
            _ => panic!("Expected UnprocessableEntity"),
        }

        assert_eq!(err1_formatted, "Cannot process: Session is finished");
    }

    #[test]
    fn test_new_error_serialization() {
        let conflict_err =
            PlatformError::conflict_with_details("Chunk not ready", "unjudged patients");
        let unprocessable_err =
            PlatformError::unprocessable_entity_with_reason("Session done", "complete");

        let conflict_json = serde_json::to_string(&conflict_err).unwrap();
        let unprocessable_json = serde_json::to_string(&unprocessable_err).unwrap();

        assert!(conflict_json.contains("Conflict"));
        assert!(conflict_json.contains("unjudged patients"));
        assert!(unprocessable_json.contains("UnprocessableEntity"));
        assert!(unprocessable_json.contains("complete"));
    }
}
