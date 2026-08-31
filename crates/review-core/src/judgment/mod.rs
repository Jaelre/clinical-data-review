// Judgment module - handles patient judgment and review logic
// Implements three-layer architecture using platform-models as canonical data contract

mod dal;
pub mod service; // Private to module - only service can access

// Re-export commonly used types
pub use service::*;
