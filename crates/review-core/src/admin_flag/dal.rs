//! Thin admin-flag data-access wrappers around the shared database contract.

use platform_db::DatabaseConnection;
use platform_errors::Result;
use platform_models::{AdminFlag, Patient};
use uuid::Uuid;

pub(super) async fn get_patient_by_external_id_wrapper(
    db: &dyn DatabaseConnection,
    external_id: &str,
    tenant_id: Uuid,
) -> Result<Patient> {
    db.get_patient_by_external_id(external_id, tenant_id).await
}

pub(super) async fn upsert_admin_flag_wrapper(
    db: &dyn DatabaseConnection,
    patient_id: Uuid,
    tenant_id: Uuid,
    created_by: Uuid,
    flag_type: &str,
    reason: &str,
) -> Result<AdminFlag> {
    db.upsert_admin_flag(patient_id, tenant_id, created_by, flag_type, reason)
        .await
}

pub(super) async fn get_admin_flag_by_patient_id_wrapper(
    db: &dyn DatabaseConnection,
    patient_id: Uuid,
    tenant_id: Uuid,
) -> Result<Option<AdminFlag>> {
    db.get_admin_flag_by_patient_id(patient_id, tenant_id).await
}

pub(super) async fn list_admin_flags_wrapper(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
) -> Result<Vec<AdminFlag>> {
    db.list_admin_flags(tenant_id).await
}

pub(super) async fn update_admin_flag_status_wrapper(
    db: &dyn DatabaseConnection,
    flag_id: Uuid,
    tenant_id: Uuid,
    new_status: &str,
    resolved_by: Uuid,
    resolution_notes: &str,
) -> Result<AdminFlag> {
    db.update_admin_flag_status(
        flag_id,
        tenant_id,
        new_status,
        resolved_by,
        resolution_notes,
    )
    .await
}
