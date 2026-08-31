use crate::admin_flag::dal;
use crate::config::Config;
use log::info;
use platform_db::DatabaseConnection;
use platform_errors::{PlatformError, Result};
use platform_models::AdminFlag;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Admin flag summary for dashboard displays.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminFlagSummary {
    pub total_flagged_cases: usize,
    pub pending_review_count: usize,
    pub resolved_count: usize,
    pub flags_by_reason: HashMap<String, usize>,
    pub recent_flags: Vec<AdminFlag>,
}

/// Admin flag service with database connection
/// Maintains three-layer architecture by delegating to platform-db
pub struct AdminFlagService<'a> {
    db: &'a dyn DatabaseConnection,
    tenant_id: Uuid,
}

impl<'a> AdminFlagService<'a> {
    /// Create new admin flag service with database connection
    pub fn new(db: &'a dyn DatabaseConnection, tenant_id: Uuid, _config: Config) -> Self {
        Self { db, tenant_id }
    }

    /// Flag for admin review with specified flag type - creates canonical AdminFlag using platform-db
    pub async fn flag_for_admin_review_with_type(
        &self,
        patient_id: &str,
        reason: &str,
        flag_type: &str,
        created_by: Uuid,
    ) -> Result<AdminFlag> {
        // Business Rule: Validate inputs
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }
        if reason.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Reason cannot be empty",
                "reason",
            ));
        }
        if flag_type.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Flag type cannot be empty",
                "flag_type",
            ));
        }

        // Business Rule: Verify patient exists and get UUID
        let patient =
            dal::get_patient_by_external_id_wrapper(self.db, patient_id, self.tenant_id).await?;

        let admin_flag = dal::upsert_admin_flag_wrapper(
            self.db,
            patient.id,
            self.tenant_id,
            created_by,
            flag_type.trim(),
            reason.trim(),
        )
        .await?;

        info!(
            "✅ Admin flag created for patient {} with reason: {} (type: {})",
            patient_id, reason, flag_type
        );
        Ok(admin_flag)
    }

    /// Get admin flag for patient - uses canonical AdminFlag
    pub async fn get_admin_flag(&self, patient_id: &str) -> Result<Option<AdminFlag>> {
        // Business Rule: Validate patient ID
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }

        let patient = match dal::get_patient_by_external_id_wrapper(
            self.db,
            patient_id,
            self.tenant_id,
        )
        .await
        {
            Ok(p) => p,
            Err(PlatformError::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };

        dal::get_admin_flag_by_patient_id_wrapper(self.db, patient.id, self.tenant_id).await
    }

    /// Resolve admin flag by patient ID - updates status to resolved using platform-db
    pub async fn resolve_admin_flag(
        &self,
        patient_id: &str,
        resolution_notes: &str,
        resolved_by: Uuid,
    ) -> Result<()> {
        // Business Rule: Validate inputs
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }
        if resolution_notes.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Resolution notes cannot be empty",
                "resolution_notes",
            ));
        }

        let patient =
            dal::get_patient_by_external_id_wrapper(self.db, patient_id, self.tenant_id).await?;
        let flag = dal::get_admin_flag_by_patient_id_wrapper(self.db, patient.id, self.tenant_id)
            .await?
            .filter(|flag| flag.status == "active")
            .ok_or_else(|| PlatformError::not_found("active_admin_flag", patient_id))?;
        dal::update_admin_flag_status_wrapper(
            self.db,
            flag.id,
            self.tenant_id,
            "resolved",
            resolved_by,
            resolution_notes.trim(),
        )
        .await?;
        Ok(())
    }

    pub async fn get_admin_flag_statistics(&self) -> Result<AdminFlagSummary> {
        let flags = dal::list_admin_flags_wrapper(self.db, self.tenant_id).await?;
        let pending_review_count = flags.iter().filter(|flag| flag.status == "active").count();
        let resolved_count = flags
            .iter()
            .filter(|flag| flag.status == "resolved")
            .count();
        let mut flags_by_reason = HashMap::new();
        for flag in &flags {
            *flags_by_reason.entry(flag.reason.clone()).or_insert(0) += 1;
        }

        Ok(AdminFlagSummary {
            total_flagged_cases: flags.len(),
            pending_review_count,
            resolved_count,
            flags_by_reason,
            recent_flags: flags.into_iter().take(20).collect(),
        })
    }
}
