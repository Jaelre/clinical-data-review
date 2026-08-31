//! Thin judgment data-access wrappers around the shared database contract.

use platform_db::DatabaseConnection;
use platform_errors::Result;
use platform_models::{Judgment, Patient, ResearchSession};
use uuid::Uuid;

/// DAL wrapper for upserting judgment
/// This is a THIN wrapper - contains NO business logic
pub(super) async fn upsert_judgment_wrapper(
    db: &dyn DatabaseConnection,
    patient_id: Uuid,
    tenant_id: Uuid,
    reviewer_id: Option<Uuid>,
    judgment_code: &str,
    notes: Option<&str>,
) -> Result<Judgment> {
    db.upsert_judgment(patient_id, tenant_id, reviewer_id, judgment_code, notes)
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

/// DAL wrapper for retrieving judgment by patient ID
pub(super) async fn get_judgment_by_patient_id_wrapper(
    db: &dyn DatabaseConnection,
    patient_id: Uuid,
    tenant_id: Uuid,
) -> Result<Option<Judgment>> {
    db.get_judgment_by_patient_id(patient_id, tenant_id).await
}

/// DAL wrapper for retrieving active research session
pub(super) async fn get_active_research_session_wrapper(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<Option<ResearchSession>> {
    db.get_active_research_session(tenant_id, researcher_id)
        .await
}
