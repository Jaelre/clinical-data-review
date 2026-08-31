// Admin flag module - handles complex case review and escalation workflow
// Implements three-layer architecture using platform-models as canonical data contract

mod dal;
pub mod service; // Private to module - only service can access

// Re-export commonly used types
pub use service::*;
