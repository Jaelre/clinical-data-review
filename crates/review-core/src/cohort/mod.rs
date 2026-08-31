// Research cohort management module
// Database-centric cohort workflow replacing file-based patient loading

mod dal;
pub mod service; // Private to module - only service can access

// Re-export service for external use
pub use service::CohortService;
