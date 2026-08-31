//! Database Connection Trait
//!
//! Defines the unified interface for all database operations across different database systems.

use crate::query_options::*;
use async_trait::async_trait;
use platform_errors::Result;
use platform_models::*;
use std::collections::HashMap;
use uuid::Uuid;

/// Unified database connection interface.
///
/// The workspace now treats SQLite as the only active backend, but the contract
/// remains domain-oriented so the review app and ETL code never need to know how
/// data is stored.
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait DatabaseConnection: Send + Sync {
    /// Run all pending migrations
    async fn run_migrations(&self) -> Result<()>;

    // Tenant operations
    /// Create a new tenant
    async fn create_tenant(&self, name: &str, slug: &str) -> Result<Tenant>;

    /// Get a tenant by its slug
    async fn get_tenant_by_slug(&self, slug: &str) -> Result<Tenant>;

    /// Get a tenant by its ID
    async fn get_tenant_by_id(&self, id: Uuid) -> Result<Tenant>;

    /// List active tenants available in the workspace database
    async fn list_tenants(&self) -> Result<Vec<Tenant>>;

    /// Get a user by email
    async fn get_user_by_email(&self, email: &str) -> Result<User>;

    /// Get a user by ID
    async fn get_user_by_id(&self, id: Uuid) -> Result<User>;

    // User-Tenant role operations
    /// Create a new user-tenant role assignment
    async fn create_user_tenant_role(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        role: &str,
    ) -> Result<UserTenantRole>;

    /// Get user roles for a specific tenant
    async fn get_user_roles_for_tenant(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<UserTenantRole>>;

    /// List local operators that can work within a tenant
    async fn list_local_operators(&self, tenant_id: Uuid) -> Result<Vec<LocalOperator>>;

    /// Create a lightweight local operator and attach a tenant role
    async fn create_local_operator(
        &self,
        tenant_id: Uuid,
        display_name: &str,
        local_identifier: Option<&str>,
        email: Option<&str>,
        role: &str,
    ) -> Result<LocalOperator>;

    /// Start a new local work session for a selected operator
    async fn start_local_work_session(
        &self,
        tenant_id: Uuid,
        operator_id: Uuid,
        session_label: Option<&str>,
    ) -> Result<LocalWorkSession>;

    /// Get the active local work session for an operator within a tenant
    async fn get_active_local_work_session(
        &self,
        tenant_id: Uuid,
        operator_id: Uuid,
    ) -> Result<Option<LocalWorkSession>>;

    /// End a local work session
    async fn end_local_work_session(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
    ) -> Result<LocalWorkSession>;

    // Patient operations
    /// Create or update a patient by external identifier.
    async fn create_patient(
        &self,
        external_id: &str,
        age: Option<i32>,
        sex: Option<&str>,
        tenant_id: Uuid,
    ) -> Result<Patient>;

    /// Get a patient by external ID within a tenant
    async fn get_patient_by_external_id(
        &self,
        external_id: &str,
        tenant_id: Uuid,
    ) -> Result<Patient>;

    /// Get a patient by ID
    async fn get_patient_by_id(&self, id: Uuid) -> Result<Patient>;

    /// Batch create or update patients.
    /// Returns a map of external_id -> UUID for the created/updated patients
    async fn batch_upsert_patients(
        &self,
        patients: &[(String, Option<i32>, Option<String>)], // (external_id, age, sex)
        tenant_id: Uuid,
    ) -> Result<std::collections::HashMap<String, Uuid>>;

    // Patient note operations
    /// Create a new patient note
    async fn create_patient_note(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        category: &str,
        content: &str,
    ) -> Result<PatientNote>;

    /// Get all notes for a patient
    async fn get_patient_notes(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PatientNote>>;

    /// Get notes for a patient by category
    async fn get_patient_notes_by_category(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        category: &str,
    ) -> Result<Vec<PatientNote>>;

    /// Create or update a patient note (UPSERT operation)
    async fn upsert_patient_note(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        category: &str,
        content: &str,
    ) -> Result<PatientNote>;

    // Clinical journal operations
    /// Create a new clinical journal entry.
    async fn create_clinical_journal_entry(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        entry_timestamp: chrono::DateTime<chrono::Utc>,
        entry_sequence: i32,
        role: Option<&str>,
        content: &str,
    ) -> Result<ClinicalJournalEntry>;

    /// Get all journal entries for a patient, ordered by timestamp and sequence
    async fn get_clinical_journal_entries(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ClinicalJournalEntry>>;

    /// Get journal entries for a patient within a time range
    async fn get_clinical_journal_entries_in_range(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ClinicalJournalEntry>>;

    // Utility operations
    /// Check if the database connection is healthy
    async fn health_check(&self) -> Result<bool>;

    /// Get database version information
    async fn get_version(&self) -> Result<String>;

    /// Get the number of records in each table (for diagnostics)
    async fn get_table_counts(&self) -> Result<HashMap<String, i64>>;

    // --- Patient Listing ---
    /// Get patients with filtering, sorting, and pagination
    async fn get_patients(
        &self,
        tenant_id: Uuid,
        filters: &PatientFilterOptions,
        sorting: &PatientSortOptions,
        pagination: &PaginationOptions,
    ) -> Result<Vec<PatientSummary>>;

    /// Get total count of patients matching filters
    async fn get_patients_count(
        &self,
        tenant_id: Uuid,
        filters: &PatientFilterOptions,
    ) -> Result<i64>;

    /// Get patients by a list of external IDs (efficient database-level filtering)
    /// Uses SQL WHERE IN clause for optimal performance with large ID lists
    async fn get_patients_by_external_ids(
        &self,
        external_ids: &[String],
        tenant_id: Uuid,
    ) -> Result<Vec<PatientSummary>>;

    // --- Judgment Management ---
    /// Create or update a judgment for a patient
    async fn upsert_judgment(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        reviewer_id: Option<Uuid>,
        judgment: &str,
        notes: Option<&str>,
    ) -> Result<Judgment>;

    /// Get judgment for a specific patient
    async fn get_judgment_by_patient_id(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<Judgment>>;

    // --- Admin Flag Management ---
    /// Create or update an admin flag for a patient
    async fn upsert_admin_flag(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        created_by: Uuid,
        flag_type: &str,
        reason: &str,
    ) -> Result<AdminFlag>;

    /// Get the most recent active or resolved flag for a patient.
    async fn get_admin_flag_by_patient_id(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<AdminFlag>>;

    /// List flags for a tenant, newest first.
    async fn list_admin_flags(&self, tenant_id: Uuid) -> Result<Vec<AdminFlag>>;

    /// Update the status of an admin flag
    async fn update_admin_flag_status(
        &self,
        flag_id: Uuid,
        tenant_id: Uuid,
        new_status: &str,
        resolved_by: Uuid,
        resolution_notes: &str,
    ) -> Result<AdminFlag>;

    // --- Research Session Management ---
    /// Get active research session for a researcher
    async fn get_active_research_session(
        &self,
        tenant_id: Uuid,
        researcher_id: Uuid,
    ) -> Result<Option<ResearchSession>>;

    /// Create a new research session
    async fn create_research_session(
        &self,
        tenant_id: Uuid,
        session_name: &str,
        primary_researcher_id: Option<Uuid>,
        current_chunk_number: i32,
        current_chunk_patients: Vec<String>,
        completed_chunks: Vec<i32>,
    ) -> Result<ResearchSession>;

    /// Update research session chunk progression
    async fn update_research_session_chunk(
        &self,
        session_id: Uuid,
        tenant_id: Uuid,
        new_chunk_number: i32,
        new_chunk_patients: Vec<String>,
        newly_completed_chunk: i32,
    ) -> Result<ResearchSession>;

    /// Complete a research session
    async fn complete_research_session(
        &self,
        session_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<ResearchSession>;

    /// Pause a research session (change status to 'paused')
    async fn pause_research_session(
        &self,
        session_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<ResearchSession>;

    /// Create a new active review session from ETL-authored cohort batches.
    /// This pauses any existing active session for the user so only one active session exists.
    /// The initial session chunk is populated from batch 1 of the cohort's pre-computed batches.
    async fn create_new_active_session_from_cohort(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        session_name: &str,
        cohort_id: Uuid,
    ) -> Result<ResearchSession>;

    // --- General Updates ---
    /// Update patient review status
    async fn update_patient_review_status(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        new_status: &str,
    ) -> Result<()>;

    // --- Research Cohort Management ---
    /// Get research cohorts available to a user
    async fn get_research_cohorts_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<ResearchCohort>>;

    /// Get a research cohort by ID with user access validation
    async fn get_research_cohort_with_access(
        &self,
        tenant_id: Uuid,
        cohort_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ResearchCohort>>;

    /// Get patients in a research cohort ordered by display_order
    async fn get_research_cohort_patients(
        &self,
        tenant_id: Uuid,
        cohort_id: Uuid,
    ) -> Result<Vec<ResearchCohortPatient>>;

    /// Get user's access permissions for a cohort
    async fn get_research_cohort_reviewer(
        &self,
        tenant_id: Uuid,
        cohort_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ResearchCohortReviewer>>;

    /// Bulk insert patient IDs into cohort_ingestion staging table
    /// Returns the number of records inserted
    async fn batch_insert_cohort_ingestion(
        &self,
        cohort_name: &str,
        tenant_slug: &str,
        patient_external_ids: &[String],
    ) -> Result<i32>;

    /// Process pending cohort ingestion records into research cohorts atomically
    /// Returns (cohort_id, total_patients, processed_patients, error_count, processing_summary)
    async fn process_pending_cohort_ingestion(
        &self,
        cohort_name: &str,
        tenant_slug: &str,
        user_id: Uuid,
        description: Option<&str>,
    ) -> Result<(Uuid, i32, i32, i32, serde_json::Value)>;

    /// Get patient by ID with tenant validation
    async fn get_patient_by_id_with_tenant(
        &self,
        tenant_id: Uuid,
        patient_id: Uuid,
    ) -> Result<Option<Patient>>;

    /// Create research session with enhanced parameter support
    async fn create_research_session_enhanced(
        &self,
        tenant_id: Uuid,
        session_name: &str,
        primary_researcher_id: Option<Uuid>,
        patient_external_ids: &[String],
        chunk_size: usize,
    ) -> Result<ResearchSession>;

    // --- Batch Query Methods (N+1 Remediation) ---
    /// Batch query: Get multiple patients by external IDs
    /// Returns a HashMap for O(1) lookups by external_id
    ///
    /// This method eliminates N+1 query patterns by fetching multiple patients
    /// in a single database query.
    async fn batch_get_patients_by_external_ids(
        &self,
        external_ids: &[String],
        tenant_id: Uuid,
    ) -> Result<HashMap<String, Patient>>;

    /// Batch query: Get multiple judgments by patient UUIDs
    /// Returns a HashMap for O(1) lookups by patient_id
    ///
    /// This method eliminates N+1 query patterns by fetching multiple judgments
    /// in a single database query.
    async fn batch_get_judgments_by_patient_ids(
        &self,
        patient_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<HashMap<Uuid, Judgment>>;

    /// Batch query: Check which patients have judgments (optimized boolean check)
    /// Returns a HashSet of patient UUIDs that have judgments
    ///
    /// This method provides an optimized way to check judgment existence without
    /// fetching full judgment records, using only DISTINCT patient_id.
    async fn batch_check_patients_have_judgments(
        &self,
        patient_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<std::collections::HashSet<Uuid>>;

    // --- Research Cohort Batch Management ---

    /// Create all ETL-authored batch records for a cohort atomically (single transaction).
    /// Slices patient_external_ids into batch_size chunks deterministically (preserving order).
    /// Appends one empty placeholder batch at the end when include_empty_placeholder is true.
    /// Each import writes a complete, deterministic batch set for its cohort.
    async fn create_cohort_batches(
        &self,
        cohort_id: Uuid,
        tenant_id: Uuid,
        patient_external_ids: &[String],
        batch_size: usize,
        include_empty_placeholder: bool,
    ) -> Result<Vec<ResearchCohortBatch>>;

    /// Get all pre-computed batches for a cohort ordered by batch_number ASC.
    async fn get_cohort_batches(
        &self,
        cohort_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ResearchCohortBatch>>;

    /// Batch resolve patient UUIDs → external_ids within a tenant.
    /// Returns HashMap<patient_uuid, external_id>.
    /// Used by the ETL cohort processor to convert cohort patient links to external IDs.
    async fn batch_get_patient_external_ids_by_uuids(
        &self,
        patient_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<HashMap<Uuid, String>>;
}
