//! Platform Models
//!
//! Canonical data structures that directly map to database tables.
//! This crate serves as the data contract for the entire clinical platform.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tenant represents a separate workspace/organization with isolated data
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    /// Unique identifier for the tenant
    pub id: Uuid,
    /// Human-readable name of the tenant
    pub name: String,
    /// URL-safe unique identifier for the tenant
    pub slug: String,
    /// When the tenant was created
    pub created_at: DateTime<Utc>,
    /// When the tenant was last updated
    pub updated_at: DateTime<Utc>,
}

/// User represents a system user who can access the platform
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    /// Unique identifier for the user
    pub id: Uuid,
    /// Profile information
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: Option<String>,
    /// When the user was created
    pub created_at: DateTime<Utc>,
    /// When the user was last updated
    pub updated_at: DateTime<Utc>,
}

/// UserTenantRole represents the role a user has within a specific tenant
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserTenantRole {
    /// Unique identifier for this role assignment
    pub id: Uuid,
    /// Reference to the user
    pub user_id: Uuid,
    /// Reference to the tenant
    pub tenant_id: Uuid,
    /// The role name (e.g., "admin", "reviewer", "viewer")
    pub role: String,
    /// When the role was assigned
    pub created_at: DateTime<Utc>,
    /// When the role was last updated
    pub updated_at: DateTime<Utc>,
}

/// LocalOperator represents a machine-local operator profile within a tenant
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LocalOperator {
    /// Underlying user identifier for attribution
    pub id: Uuid,
    /// Reference to the tenant this operator can work in
    pub tenant_id: Uuid,
    /// Human-readable label shown in the local login mask
    pub display_name: String,
    /// Optional machine-local identifier used for quick selection
    pub local_identifier: Option<String>,
    /// Optional contact-style identifier for local operator attribution.
    pub email: Option<String>,
    /// Role within the tenant
    pub role: String,
    /// When the operator profile was created
    pub created_at: DateTime<Utc>,
    /// When the operator profile was last updated
    pub updated_at: DateTime<Utc>,
}

/// LocalWorkSession represents a machine-local operator work session
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LocalWorkSession {
    /// Unique identifier for the work session
    pub id: Uuid,
    /// Reference to the tenant
    pub tenant_id: Uuid,
    /// Reference to the selected operator
    pub operator_id: Uuid,
    /// Human-readable session label
    pub session_label: String,
    /// Current status (active, paused, completed)
    pub status: String,
    /// When the work session started
    pub started_at: DateTime<Utc>,
    /// When the work session was last active
    pub last_activity_at: DateTime<Utc>,
    /// When the work session ended, if it has ended
    pub ended_at: Option<DateTime<Utc>>,
}

/// Patient represents a clinical patient within a tenant's data
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Patient {
    /// Unique identifier for the patient
    pub id: Uuid,
    /// Original patient identifier from source data (e.g., Excel import)
    pub external_id: String,
    /// Patient's age (optional)
    pub age: Option<i32>,
    /// Patient's biological sex (optional)
    pub sex: Option<String>,
    /// Reference to the tenant this patient belongs to
    pub tenant_id: Uuid,
    /// When the patient record was created
    pub created_at: DateTime<Utc>,
    /// When the patient record was last updated
    pub updated_at: DateTime<Utc>,
}

/// PatientNote represents additional information about a patient
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PatientNote {
    /// Unique identifier for the note
    pub id: Uuid,
    /// Reference to the patient this note belongs to
    pub patient_id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// Category of the note (e.g., "past_history", "medication", "allergies")
    pub category: String,
    /// The note content (aggregated text)
    pub content: String,
    /// When the note was created
    pub created_at: DateTime<Utc>,
    /// When the note was last updated
    pub updated_at: DateTime<Utc>,
}

/// ClinicalJournalEntry represents a timestamped clinical observation or event
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ClinicalJournalEntry {
    /// Unique identifier for the journal entry
    pub id: Uuid,
    /// Reference to the patient this entry belongs to
    pub patient_id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// When this clinical event occurred (from source data)
    pub entry_timestamp: DateTime<Utc>,
    /// Sequential ordering within the patient's timeline
    pub entry_sequence: i32,
    /// Role of the person who made this entry (optional)
    pub role: Option<String>,
    /// The clinical observation or event content
    pub content: String,
    /// When this record was created in our system
    pub created_at: DateTime<Utc>,
    /// When this record was last updated
    pub updated_at: DateTime<Utc>,
}

/// PatientSummary represents a lightweight patient record for list views
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PatientSummary {
    /// Unique identifier for the patient
    pub id: Uuid,
    /// Original patient identifier from source data
    pub external_id: String,
    /// Patient's age (optional)
    pub age: Option<i32>,
    /// Patient's biological sex (optional)
    pub sex: Option<String>,
    /// Review status for patient workflow
    pub review_status: String,
    /// Priority level for review ordering
    pub priority_level: i32,
    /// Whether patient has a judgment record
    pub has_judgment: bool,
    /// Whether patient has active admin flags
    pub is_flagged: bool,
    /// When the patient record was created
    pub created_at: DateTime<Utc>,
}

/// Judgment represents a basic clinical review judgment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Judgment {
    /// Unique identifier for the judgment
    pub id: Uuid,
    /// Reference to the patient this judgment belongs to
    pub patient_id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// Reference to the reviewer who made the judgment
    pub reviewer_id: Option<Uuid>,
    /// The judgment decision (A, N, U, F)
    pub judgment: String,
    /// Optional notes about the judgment
    pub judgment_notes: Option<String>,
    /// When the judgment was made
    pub judgment_made_at: DateTime<Utc>,
}

/// AdminFlag represents a basic administrative flag for patient workflow
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminFlag {
    /// Unique identifier for the flag
    pub id: Uuid,
    /// Reference to the patient this flag belongs to
    pub patient_id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// Reference to the user who created the flag
    pub created_by: Option<Uuid>,
    /// Type of the flag (e.g., "data_quality", "clinical_concern")
    pub flag_type: String,
    /// Human-readable reason for the flag
    pub reason: String,
    /// Current status of the flag (active, resolved, dismissed)
    pub status: String,
    /// When the flag was created
    pub created_at: DateTime<Utc>,
}

/// ResearchSession represents a research session for patient batch processing.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResearchSession {
    /// Unique identifier for the research session
    pub id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// Human-readable name for the session
    pub session_name: String,
    /// Reference to the primary researcher
    pub primary_researcher_id: Option<Uuid>,
    /// Current status of the session (active, completed, paused)
    pub status: String,
    /// Total number of patients in session (NOT NULL in database)
    pub total_patients: i32,
    /// Current chunk number being processed
    pub current_chunk_number: i32,
    /// Patient external IDs in current chunk
    pub current_chunk_patients: Vec<String>,
    /// Array of completed chunk numbers
    pub completed_chunks: Vec<i32>,
    /// Reference to the source cohort when the session was cohort-created.
    pub cohort_id: Option<Uuid>,
}

/// ResearchCohortBatch represents a single ETL-authored patient batch for a research cohort.
/// These batches are created when new cohort data is ingested and later consumed read-only by
/// review sessions in the app.
/// Matches the research cohort batch table in the bundled SQLite schema.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResearchCohortBatch {
    /// Unique identifier for this batch record
    pub id: Uuid,
    /// Reference to the parent research cohort
    pub cohort_id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// 1-based batch number; deterministic ordering derived from cohort display_order
    pub batch_number: i32,
    /// External patient IDs assigned to this batch (empty only for is_empty placeholder)
    pub patient_external_ids: Vec<String>,
    /// True only for the single optional trailing empty placeholder batch
    pub is_empty: bool,
    /// When the batch record was created
    pub created_at: DateTime<Utc>,
    /// When the batch record was last updated
    pub updated_at: DateTime<Utc>,
}

/// CohortIngestion represents raw cohort data pending validation and processing
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CohortIngestion {
    /// Unique identifier for the ingestion record
    pub id: Uuid,
    /// Name of the cohort being ingested
    pub cohort_name: String,
    /// External patient identifier from source data
    pub patient_external_id: String,
    /// Tenant slug for multi-tenant isolation
    pub tenant_slug: String,
    /// Processing status (pending, processed, error, duplicate)
    pub status: String,
    /// Error message if processing failed
    pub error_message: Option<String>,
    /// Display order for patient presentation
    pub display_order: Option<i32>,
    /// Additional ingestion metadata
    pub ingestion_metadata: Option<serde_json::Value>,
    /// When the record was created
    pub created_at: DateTime<Utc>,
    /// When the record was processed
    pub processed_at: Option<DateTime<Utc>>,
}

/// ResearchCohort represents a validated cohort definition
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResearchCohort {
    /// Unique identifier for the cohort
    pub id: Uuid,
    /// Reference to the tenant (for data isolation)
    pub tenant_id: Uuid,
    /// Human-readable name for the cohort
    pub name: String,
    /// Optional description of the cohort
    pub description: Option<String>,
    /// Optional identifier supplied by the importing research workflow.
    pub external_cohort_id: Option<String>,
    /// Cached count of patients in cohort
    pub total_patients: i32,
    /// Type of cohort (manual, automated, imported)
    pub cohort_type: String,
    /// JSONB selection criteria used to define the cohort
    pub selection_criteria: Option<serde_json::Value>,
    /// Reference to the user who created the cohort
    pub created_by: Option<Uuid>,
    /// Research protocol description
    pub research_protocol: Option<String>,
    /// Additional study metadata
    pub study_metadata: Option<serde_json::Value>,
    /// Current status (active, archived, or draft)
    pub status: String,
    /// When the cohort was created
    pub created_at: DateTime<Utc>,
    /// When the cohort was last updated
    pub updated_at: DateTime<Utc>,
    /// When the cohort was archived
    pub archived_at: Option<DateTime<Utc>>,
    /// Version number for change tracking
    pub version: i32,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// ResearchCohortPatient represents the link between cohorts and patients
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResearchCohortPatient {
    /// Reference to the cohort
    pub cohort_id: Uuid,
    /// Reference to the patient
    pub patient_id: Uuid,
    /// Display order within the cohort
    pub display_order: i32,
    /// Reason for including this patient
    pub inclusion_reason: Option<String>,
    /// Patient-specific metadata within cohort context
    pub patient_metadata: Option<serde_json::Value>,
    /// When the patient was added to the cohort
    pub added_at: DateTime<Utc>,
    /// Reference to the user who added the patient
    pub added_by: Option<Uuid>,
}

/// ResearchCohortReviewer represents role-based access control for cohorts
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResearchCohortReviewer {
    /// Reference to the cohort
    pub cohort_id: Uuid,
    /// Reference to the user
    pub user_id: Uuid,
    /// Access role (owner, reviewer, observer, analyst)
    pub role: String,
    /// Whether user can review patients in this cohort
    pub can_review: bool,
    /// Whether user can export cohort data
    pub can_export: bool,
    /// Whether user can modify cohort definition
    pub can_modify_cohort: bool,
    /// When access was granted
    pub granted_at: DateTime<Utc>,
    /// Reference to the user who granted access
    pub granted_by: Option<Uuid>,
    /// When access expires (optional)
    pub expires_at: Option<DateTime<Utc>>,
    /// Additional access metadata
    pub access_metadata: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_serialization() {
        let tenant = Tenant {
            id: Uuid::new_v4(),
            name: "Example Research Workspace".to_string(),
            slug: "example-research-workspace".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let serialized = serde_json::to_string(&tenant).unwrap();
        let deserialized: Tenant = serde_json::from_str(&serialized).unwrap();

        assert_eq!(tenant.id, deserialized.id);
        assert_eq!(tenant.name, deserialized.name);
        assert_eq!(tenant.slug, deserialized.slug);
    }

    #[test]
    fn test_patient_optional_fields() {
        let patient = Patient {
            id: Uuid::new_v4(),
            external_id: "PAT001".to_string(),
            age: None,
            sex: Some("F".to_string()),
            tenant_id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(patient.age.is_none());
        assert!(patient.sex.is_some());
    }

    #[test]
    fn test_clinical_journal_entry_ordering() {
        let entry1 = ClinicalJournalEntry {
            id: Uuid::new_v4(),
            patient_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            entry_timestamp: Utc::now(),
            entry_sequence: 1,
            role: Some("nurse".to_string()),
            content: "Patient admitted".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let entry2 = ClinicalJournalEntry {
            entry_sequence: 2,
            content: "Vitals recorded".to_string(),
            ..entry1.clone()
        };

        assert!(entry2.entry_sequence > entry1.entry_sequence);
    }
}
