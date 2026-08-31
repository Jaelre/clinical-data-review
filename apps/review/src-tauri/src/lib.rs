// Library exports used by integration tests.

pub mod app_state;
pub mod command_helpers;
pub mod commands;
pub mod runtime_paths;

// Re-export commonly used types for easier testing
pub use app_state::AppState;
pub use commands::*;

// Re-export review-core types for testing
pub use review_core::{
    admin_flag::AdminFlagSummary,
    judgment::JudgmentSummary,
    patient::{PatientRecord, PatientStatistics, PatientSummary},
    Config,
};

// Re-export canonical models from platform-models
pub use platform_models::{AdminFlag, Judgment};

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::{Deref, DerefMut};

    struct TestAppState {
        state: AppState,
        _directory: tempfile::TempDir,
    }

    impl Deref for TestAppState {
        type Target = AppState;

        fn deref(&self) -> &Self::Target {
            &self.state
        }
    }

    impl DerefMut for TestAppState {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.state
        }
    }

    async fn setup_test_state() -> TestAppState {
        let directory = tempfile::tempdir().expect("temporary test database directory");
        let database_path = directory.path().join("review.sqlite3");
        let database_url = format!("sqlite://{}", database_path.display());
        let config = Config::default();
        let state = AppState::new_with_database_url(config, &database_url)
            .await
            .expect("Failed to create AppState");
        TestAppState {
            state,
            _directory: directory,
        }
    }

    #[tokio::test]
    async fn test_app_state_initialization() {
        let app_state = setup_test_state().await;
        assert!(!app_state.has_active_operator_session());
    }

    #[tokio::test]
    async fn test_local_operator_session_flow() {
        let mut app_state = setup_test_state().await;

        let session = app_state
            .create_local_operator("Unit Test Operator")
            .await
            .expect("Failed to create local operator session");

        let operator = session.operator.expect("operator session should be active");
        assert!(app_state.has_active_operator_session());
        assert_eq!(app_state.get_current_user_id().unwrap(), operator.id);
        assert!(app_state.get_current_tenant_id().is_some());

        app_state
            .clear_operator_session()
            .await
            .expect("Failed to clear operator session");
        assert!(!app_state.has_active_operator_session());
    }

    /*
        #[tokio::test]
        async fn test_patient_command_delegation() {
    ...
            let result = get_admin_flag_summary(state).await;
            assert!(result.is_ok());
            let summary = result.unwrap();
            assert_eq!(summary.total_flagged_cases, 0); // No flags initially
        }
        */

    #[test]
    fn test_config_from_environment() {
        // Test that config can be loaded from environment
        let config = Config::default();
        assert_eq!(config.environment, "development");
        assert_eq!(config.data_directory, "data");
        assert_eq!(config.output_directory, "output");
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_feature_flags_default() {
        // Test that feature flags have expected defaults
        let features = review_core::config::FeatureFlags::default();
        assert!(features.clinical_journal_privacy);
        assert!(features.admin_review_flagging);
        assert!(features.research_mode);
    }
}
