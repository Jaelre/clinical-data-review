// Configuration module for app-specific environment settings.
// Database access is handled through platform_db::DatabaseConnectionType.

use platform_errors::{PlatformError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub environment: String,
    pub data_directory: String,
    pub output_directory: String,
    pub log_level: String,
    pub features: FeatureFlags,
}

/// Feature flags for enabling/disabling application features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub clinical_journal_privacy: bool,
    pub admin_review_flagging: bool,
    pub research_mode: bool,
}

/// Frontend data structure for client initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendData {
    pub config: Config,
    pub judgment_values: HashMap<String, String>,
    pub ui_messages: HashMap<String, String>,
}

/// UI theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UITheme {
    pub theme: String,
    pub dark_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            environment: "development".to_string(),
            data_directory: "data".to_string(),
            output_directory: "output".to_string(),
            log_level: "info".to_string(),
            features: FeatureFlags::default(),
        }
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            clinical_journal_privacy: true,
            admin_review_flagging: true,
            research_mode: true,
        }
    }
}

impl Config {
    /// Create new config with custom paths (for testing and custom deployments)
    pub fn new(data_dir: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            environment: "development".to_string(),
            data_directory: data_dir.to_string_lossy().to_string(),
            output_directory: output_dir.to_string_lossy().to_string(),
            log_level: "info".to_string(),
            features: FeatureFlags::default(),
        }
    }

    /// Load optional runtime settings from environment variables.
    pub fn from_env() -> Result<Self> {
        let runtime_root = default_runtime_root();
        let default_data_dir = runtime_root.join("data").to_string_lossy().to_string();
        let default_output_dir = runtime_root.join("output").to_string_lossy().to_string();

        Ok(Self {
            environment: env::var("APP_ENVIRONMENT").unwrap_or_else(|_| default_environment()),
            data_directory: env::var("DATA_DIRECTORY").unwrap_or(default_data_dir),
            output_directory: env::var("OUTPUT_DIRECTORY").unwrap_or(default_output_dir),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            features: FeatureFlags::from_env()?,
        })
    }

    /// Get full path for data files
    pub fn data_path(&self, filename: &str) -> String {
        format!("{}/{}", self.data_directory, filename)
    }

    /// Get data directory as PathBuf
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.data_directory)
    }

    /// Get output directory as PathBuf
    pub fn output_dir(&self) -> PathBuf {
        PathBuf::from(&self.output_directory)
    }

    /// Get full path for output files
    pub fn output_path(&self, filename: &str) -> String {
        format!("{}/{}", self.output_directory, filename)
    }

    /// Check if running in development mode
    pub fn is_development(&self) -> bool {
        self.environment == "development"
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }

    pub fn should_log_runtime_details(&self) -> bool {
        !self.is_production()
    }
}

impl FeatureFlags {
    /// Load feature flags from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            clinical_journal_privacy: parse_bool_env("FEATURE_CLINICAL_JOURNAL_PRIVACY", true)?,
            admin_review_flagging: parse_bool_env("FEATURE_ADMIN_REVIEW_FLAGGING", true)?,
            research_mode: parse_bool_env("FEATURE_RESEARCH_MODE", true)?,
        })
    }

    /// Get list of enabled features for logging
    pub fn enabled_features(&self) -> Vec<String> {
        let mut features = Vec::new();

        if self.clinical_journal_privacy {
            features.push("clinical_journal_privacy".to_string());
        }
        if self.admin_review_flagging {
            features.push("admin_review_flagging".to_string());
        }
        if self.research_mode {
            features.push("research_mode".to_string());
        }

        features
    }
}

/// Parse boolean environment variable with default value
fn parse_bool_env(key: &str, default: bool) -> Result<bool> {
    match env::var(key) {
        Ok(value) => match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(PlatformError::config_key(
                format!("Expected a boolean value, received `{value}`"),
                key,
            )),
        },
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(PlatformError::config_key(
            format!("Could not read environment value: {error}"),
            key,
        )),
    }
}

/// Load configuration with error handling and logging
pub async fn load_config() -> Result<Config> {
    #[cfg(debug_assertions)]
    if dotenvy::dotenv().is_ok() {
        log::info!("Loaded configuration from .env file");
    }

    let config = Config::from_env()?;

    log::info!(
        "Configuration loaded for environment: {}",
        config.environment
    );
    log::info!("Data directory: {}", config.data_directory);
    log::info!("Output directory: {}", config.output_directory);
    let enabled_features = config.features.enabled_features();
    if !enabled_features.is_empty() {
        log::info!("Enabled features: {}", enabled_features.join(", "));
    }

    Ok(config)
}

fn default_environment() -> String {
    if cfg!(debug_assertions) {
        "development".to_string()
    } else {
        "production".to_string()
    }
}

fn default_runtime_root() -> PathBuf {
    env::var_os("REVIEW_APP_RUNTIME_ROOT")
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
        })
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Get frontend initialization data with all required configuration
pub fn get_initial_frontend_data(config: &Config) -> FrontendData {
    log::info!("Preparing frontend data for client initialization");

    let judgment_values = get_judgment_values();
    let ui_messages = get_ui_messages();

    FrontendData {
        config: config.clone(),
        judgment_values,
        ui_messages,
    }
}

/// Get judgment values for clinical review workflow
pub fn get_judgment_values() -> HashMap<String, String> {
    let mut values = HashMap::new();
    values.insert("A".to_string(), "Accepted".to_string());
    values.insert("N".to_string(), "Needs Review".to_string());
    values.insert("U".to_string(), "Uncertain".to_string());
    values
}

/// Get UI messages for clinical review workflow
pub fn get_ui_messages() -> HashMap<String, String> {
    let mut messages = HashMap::new();
    messages.insert(
        "judge_patient_first".to_string(),
        "Please judge this patient first".to_string(),
    );
    messages.insert(
        "next_batch_available".to_string(),
        "Next batch of patients available".to_string(),
    );
    messages.insert(
        "all_patients_reviewed".to_string(),
        "All patients have been reviewed".to_string(),
    );
    messages.insert(
        "navigation_disabled".to_string(),
        "Navigation disabled - not in active chunk".to_string(),
    );
    messages
}

/// Check if a specific feature is enabled
pub fn is_feature_enabled(config: &Config, feature_name: &str) -> bool {
    match feature_name {
        "clinical_journal_privacy" => config.features.clinical_journal_privacy,
        "admin_review_flagging" => config.features.admin_review_flagging,
        "research_mode" => config.features.research_mode,
        _ => false,
    }
}

/// Get UI theme settings
pub fn get_ui_theme() -> UITheme {
    UITheme {
        theme: "healthcare_blue".to_string(),
        dark_mode: false,
    }
}

/// Patient selection info for filtering interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientSelectionInfo {
    pub is_filtered: bool,
    pub source_file: Option<String>,
    pub total_patients: u32,
    pub filtered_patients: u32,
    pub description: Option<String>,
    pub selected_count: u32,
    pub total_available: u32,
}

/// Research session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSession {
    pub current_chunk: u32,
    pub total_chunks: u32,
    pub patients_per_chunk: u32,
    pub active: bool,
}

/// Get patient selection info
pub fn get_patient_selection_info() -> PatientSelectionInfo {
    PatientSelectionInfo {
        is_filtered: false,
        source_file: None,
        total_patients: 0,
        filtered_patients: 0,
        description: None,
        selected_count: 0,
        total_available: 0,
    }
}

/// Get research session info
pub fn get_research_session() -> ResearchSession {
    ResearchSession {
        current_chunk: 1,
        total_chunks: 1,
        patients_per_chunk: 12,
        active: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_runtime_logging_is_hardened_by_default() {
        let config = Config {
            environment: "production".to_string(),
            ..Config::default()
        };

        assert!(!config.should_log_runtime_details());
    }

    #[test]
    fn patient_selection_info_placeholder_matches_frontend_shape() {
        let info = get_patient_selection_info();

        assert!(!info.is_filtered);
        assert_eq!(info.selected_count, 0);
        assert_eq!(info.total_available, 0);
        assert!(info.description.is_none());
    }
}
