// Core business logic library for the local Clinical Data Review application.

pub mod admin_flag;
pub mod cohort;
pub mod config;
pub mod judgment;
pub mod patient;
pub mod research_session;

// Re-export commonly used types for convenience
pub use config::Config;
pub use platform_errors::{PlatformError, Result};
