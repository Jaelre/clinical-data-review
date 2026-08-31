// Research session module - database-centric research workflow management
// Implements chunk-based patient processing using platform-models::ResearchSession

mod dal;
pub mod service; // Private to module - only service can access

// Re-export commonly used types
pub use platform_models::ResearchSession;
pub use service::*;
