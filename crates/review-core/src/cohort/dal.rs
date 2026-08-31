// Research cohort Data Access Layer (DAL)
// Thin wrappers around platform-db trait methods for cohort operations

use platform_db::DatabaseConnection;
use platform_errors::Result;
use platform_models::{ResearchCohort, ResearchCohortBatch, ResearchCohortReviewer};
use uuid::Uuid;

/// Get available cohorts for a user within a tenant
pub(super) async fn get_user_cohorts(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Vec<ResearchCohort>> {
    // Use platform-db trait method to get cohorts with user access
    db.get_research_cohorts_for_user(tenant_id, user_id).await
}

/// Get cohort details by ID with access validation
pub(super) async fn get_cohort_by_id(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    cohort_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ResearchCohort>> {
    // Use platform-db trait method to get cohort with access validation
    db.get_research_cohort_with_access(tenant_id, cohort_id, user_id)
        .await
}

/// Get user access details for a cohort
pub(super) async fn get_user_cohort_access(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    cohort_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ResearchCohortReviewer>> {
    // Use platform-db trait method to get user access
    db.get_research_cohort_reviewer(tenant_id, cohort_id, user_id)
        .await
}

/// Get ETL-authored batches for a cohort (thin wrapper) - ARCHITECTURE COMPLIANT
pub(super) async fn get_cohort_batches_wrapper(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    cohort_id: Uuid,
) -> Result<Vec<ResearchCohortBatch>> {
    db.get_cohort_batches(cohort_id, tenant_id).await
}

/// Create new active research session (thin wrapper) - ARCHITECTURE COMPLIANT
pub(super) async fn create_new_active_session_from_cohort_wrapper(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    session_name: &str,
    cohort_id: Uuid,
) -> Result<platform_models::ResearchSession> {
    db.create_new_active_session_from_cohort(tenant_id, user_id, session_name, cohort_id)
        .await
}
