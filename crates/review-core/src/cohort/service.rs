// Research cohort service - database-centric cohort management
// Replaces file-based patient loading with structured database operations

use crate::config::Config;
use log::info;
use platform_db::DatabaseConnection;
use platform_errors::{PlatformError, Result};
use platform_models::{ResearchCohort, ResearchSession};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::dal;

/// Research cohort service with database connection
/// Manages cohort-based patient organization and access control
pub struct CohortService<'a> {
    db: &'a dyn DatabaseConnection,
    tenant_id: Uuid,
}

/// Response DTO for available cohorts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableCohort {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub total_patients: i32,
    pub cohort_type: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub user_role: String,
    pub can_review: bool,
    pub can_export: bool,
}

/// Response DTO for cohort session creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortSessionResponse {
    pub session: ResearchSession,
    pub cohort: ResearchCohort,
    pub patient_count: i32,
    pub chunks_created: i32,
}

impl<'a> CohortService<'a> {
    /// Create new cohort service with database connection
    pub fn new(db: &'a dyn DatabaseConnection, tenant_id: Uuid, _config: Config) -> Self {
        Self { db, tenant_id }
    }

    /// Get available cohorts for the current user
    /// Returns cohorts the user has access to review
    pub async fn get_available_cohorts(&self, user_id: Uuid) -> Result<Vec<AvailableCohort>> {
        info!(
            "Getting available cohorts for user {} in tenant {}",
            user_id, self.tenant_id
        );

        // Business Rule: Get cohorts user has access to
        let cohorts = dal::get_user_cohorts(self.db, self.tenant_id, user_id).await?;

        if cohorts.is_empty() {
            info!("No cohorts found for user {}", user_id);
            return Ok(vec![]);
        }

        // Business Rule: Enrich cohorts with user access information
        let mut available_cohorts = Vec::new();
        for cohort in cohorts {
            // Get user's access role for this cohort
            let access =
                dal::get_user_cohort_access(self.db, self.tenant_id, cohort.id, user_id).await?;

            if let Some(reviewer_access) = access {
                available_cohorts.push(AvailableCohort {
                    id: cohort.id,
                    name: cohort.name,
                    description: cohort.description,
                    total_patients: cohort.total_patients,
                    cohort_type: cohort.cohort_type,
                    status: cohort.status,
                    created_at: cohort.created_at,
                    user_role: reviewer_access.role,
                    can_review: reviewer_access.can_review,
                    can_export: reviewer_access.can_export,
                });
            }
        }

        info!(
            "Found {} available cohorts for user",
            available_cohorts.len()
        );
        Ok(available_cohorts)
    }

    /// Start a review session for a specific cohort.
    /// The session keeps user-specific progress while consuming ETL-authored cohort batches.
    pub async fn start_review_session_for_cohort(
        &self,
        cohort_id: Uuid,
        user_id: Uuid,
        session_name: Option<String>,
    ) -> Result<CohortSessionResponse> {
        info!(
            "Starting review session for cohort {} by user {}",
            cohort_id, user_id
        );

        // Business Rule: Validate user has access to this cohort
        let cohort = dal::get_cohort_by_id(self.db, self.tenant_id, cohort_id, user_id).await?;
        let cohort =
            cohort.ok_or_else(|| PlatformError::not_found("cohort", cohort_id.to_string()))?;

        // Business Rule: Verify user can review this cohort
        let access =
            dal::get_user_cohort_access(self.db, self.tenant_id, cohort_id, user_id).await?;
        let access = access.ok_or_else(|| {
            PlatformError::invalid_input("User does not have access to this cohort")
        })?;

        if !access.can_review {
            return Err(PlatformError::invalid_input(
                "User does not have review permissions for this cohort",
            ));
        }

        let cohort_batches =
            dal::get_cohort_batches_wrapper(self.db, self.tenant_id, cohort_id).await?;
        let review_batches: Vec<_> = cohort_batches
            .iter()
            .filter(|batch| !batch.is_empty)
            .collect();

        if review_batches.is_empty() {
            return Err(PlatformError::unprocessable_entity_with_reason(
                "This cohort does not have any ETL-authored batches yet. Re-ingest the cohort data before starting a review session.",
                "no_cohort_batches",
            ));
        }

        let session_name = match session_name {
            Some(name) if name.trim().is_empty() => {
                return Err(PlatformError::invalid_input_field(
                    "Session name cannot be empty",
                    "session_name",
                ));
            }
            Some(name) => name,
            None => format!("Review Session: {}", cohort.name),
        };

        // Business Rule: Create a user-specific session that consumes ETL-authored cohort batches.
        let session = dal::create_new_active_session_from_cohort_wrapper(
            self.db,
            self.tenant_id,
            user_id,
            &session_name,
            cohort_id,
        )
        .await?;

        info!(
            "✅ Atomically created new active session {} for user {} with {} patients",
            session.id,
            user_id,
            session.current_chunk_patients.len()
        );

        // Calculate chunks created
        let chunks_created = review_batches.len() as i32;

        info!(
            "Created review session {} for cohort {} with {} patients in {} chunks",
            session.id, cohort_id, cohort.total_patients, chunks_created
        );

        let patient_count = cohort.total_patients;

        Ok(CohortSessionResponse {
            session,
            cohort,
            patient_count,
            chunks_created,
        })
    }

    /// Get cohort details with user access validation
    pub async fn get_cohort_details(
        &self,
        cohort_id: Uuid,
        user_id: Uuid,
    ) -> Result<ResearchCohort> {
        info!(
            "Getting cohort details for cohort {} by user {}",
            cohort_id, user_id
        );

        let cohort = dal::get_cohort_by_id(self.db, self.tenant_id, cohort_id, user_id).await?;
        cohort.ok_or_else(|| PlatformError::not_found("cohort", cohort_id.to_string()))
    }
}
