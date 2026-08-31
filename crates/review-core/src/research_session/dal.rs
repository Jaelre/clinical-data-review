// Research session Data Access Layer (DAL)
// ARCHITECTURAL COMPLIANCE: Thin wrappers around platform-db operations
//
// CRITICAL RULES:
// 1. Each function is a thin wrapper - contains NO business logic
// 2. Functions are pub(super) - only accessible within research_session module
// 3. All functions delegate to a SINGLE platform-db method call
// 4. NO data aggregation, NO validation, NO calculations
// 5. Simply pass parameters through to platform-db and return the result

use platform_db::DatabaseConnection;
use platform_errors::Result;
use platform_models::{Judgment, Patient, ResearchCohortBatch, ResearchSession};
use std::collections::HashMap;
use uuid::Uuid;

/// DAL wrapper for retrieving active research session
pub(super) async fn get_active_research_session_wrapper(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<Option<ResearchSession>> {
    db.get_active_research_session(tenant_id, researcher_id)
        .await
}

/// DAL wrapper for updating research session chunk
pub(super) async fn update_research_session_chunk_wrapper(
    db: &dyn DatabaseConnection,
    session_id: Uuid,
    tenant_id: Uuid,
    next_chunk_number: i32,
    next_chunk_patients: Vec<String>,
    completed_chunk_number: i32,
) -> Result<ResearchSession> {
    db.update_research_session_chunk(
        session_id,
        tenant_id,
        next_chunk_number,
        next_chunk_patients,
        completed_chunk_number,
    )
    .await
}

/// DAL wrapper for retrieving patient by external ID
pub(super) async fn get_patient_by_external_id_wrapper(
    db: &dyn DatabaseConnection,
    external_id: &str,
    tenant_id: Uuid,
) -> Result<Patient> {
    db.get_patient_by_external_id(external_id, tenant_id).await
}

/// DAL wrapper for batch-fetching patients by external ID
pub(super) async fn batch_get_patients_by_external_ids_wrapper(
    db: &dyn DatabaseConnection,
    external_ids: &[String],
    tenant_id: Uuid,
) -> Result<HashMap<String, Patient>> {
    db.batch_get_patients_by_external_ids(external_ids, tenant_id)
        .await
}

/// DAL wrapper for retrieving judgment by patient ID
pub(super) async fn get_judgment_by_patient_id_wrapper(
    db: &dyn DatabaseConnection,
    patient_id: Uuid,
    tenant_id: Uuid,
) -> Result<Option<Judgment>> {
    db.get_judgment_by_patient_id(patient_id, tenant_id).await
}

/// DAL wrapper for batch-fetching judgments by patient ID
pub(super) async fn batch_get_judgments_by_patient_ids_wrapper(
    db: &dyn DatabaseConnection,
    patient_ids: &[Uuid],
    tenant_id: Uuid,
) -> Result<HashMap<Uuid, Judgment>> {
    db.batch_get_judgments_by_patient_ids(patient_ids, tenant_id)
        .await
}

/// DAL wrapper for fetching all ETL-authored batches for a cohort ordered by batch_number ASC
pub(super) async fn get_cohort_batches_wrapper(
    db: &dyn DatabaseConnection,
    cohort_id: Uuid,
    tenant_id: Uuid,
) -> Result<Vec<ResearchCohortBatch>> {
    db.get_cohort_batches(cohort_id, tenant_id).await
}

/// DAL wrapper for completing a research session
pub(super) async fn complete_research_session_wrapper(
    db: &dyn DatabaseConnection,
    session_id: Uuid,
    tenant_id: Uuid,
) -> Result<ResearchSession> {
    db.complete_research_session(session_id, tenant_id).await
}
