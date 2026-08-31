// Patient module - handles all patient-related business logic
// Implements three-layer architecture using platform-models as canonical data contract

mod dal;
pub mod models; // Presentation DTOs that aggregate platform-models data
pub mod service; // Private to module - only service can access

// Re-export commonly used types
pub use models::*; // Presentation DTOs for API responses
pub use service::*;
// Import canonical database models from platform-models
pub use platform_models::ClinicalJournalEntry as PlatformClinicalJournalEntry;
pub use platform_models::PatientSummary as PlatformPatientSummary;
pub use platform_models::{Patient, PatientNote};
