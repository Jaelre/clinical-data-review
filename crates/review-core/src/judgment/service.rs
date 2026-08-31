// Judgment service layer - business logic for clinical judgments
// Contains validation rules and orchestrates DAL operations

use crate::config::Config;
use crate::judgment::dal;
use log::info;
use platform_db::DatabaseConnection;
use platform_errors::{PlatformError, Result};
use platform_models::Judgment;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecentJudgmentActivity {
    pub patient_id: String,
    pub judgment: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Judgment summary for dashboard displays.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JudgmentSummary {
    pub total_judgments: usize,
    pub accepted_count: usize,
    pub needs_review_count: usize,
    pub uncertain_count: usize,
    pub judgment_distribution: HashMap<String, usize>,
    pub recent_judgments: Vec<RecentJudgmentActivity>,
}

/// Judgment service with database connection
/// Maintains three-layer architecture by delegating to DAL
pub struct JudgmentService<'a> {
    db: &'a dyn DatabaseConnection,
    tenant_id: Uuid,
    config: Config,
}

impl<'a> JudgmentService<'a> {
    /// Create new judgment service with database connection
    pub fn new(db: &'a dyn DatabaseConnection, tenant_id: Uuid, config: Config) -> Self {
        Self {
            db,
            tenant_id,
            config,
        }
    }

    /// Save judgment - delegates to DAL
    pub async fn save_judgment(
        &self,
        patient_id: &str,
        judgment: &str,
        reviewer_id: Option<Uuid>,
    ) -> Result<()> {
        // Business Rule: Validate patient ID
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }

        // Business Rule: Validate judgment value
        let judgment_trimmed = judgment.trim();
        if judgment_trimmed.is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Judgment cannot be empty",
                "judgment",
            ));
        }

        // Business Rule: Map judgment value to database format (VARCHAR(10) constraint)
        let final_judgment_code = map_judgment_to_code(judgment_trimmed)?;

        // Business Rule: Verify patient exists
        let patient =
            dal::get_patient_by_external_id_wrapper(self.db, patient_id, self.tenant_id).await?;

        // Use DAL wrapper to maintain three-layer architecture
        let _created_judgment = dal::upsert_judgment_wrapper(
            self.db,
            patient.id,
            self.tenant_id,
            reviewer_id,
            &final_judgment_code,
            None,
        )
        .await?;

        Ok(())
    }

    /// Save judgment and progress research session atomically
    /// This is a transactional operation that coordinates judgment saving with session progress
    pub async fn save_judgment_and_progress_session(
        &self,
        patient_id: &str,
        judgment: &str,
        operator_id: Option<Uuid>,
    ) -> Result<bool> {
        info!(
            "💾 Starting atomic judgment save and session progress for patient: {}",
            patient_id
        );

        self.save_judgment(patient_id, judgment, operator_id)
            .await?;
        info!("✅ Judgment saved successfully for patient: {}", patient_id);

        let Some(effective_researcher_id) = operator_id else {
            return Ok(false);
        };

        // Business Logic: Get active research session
        match dal::get_active_research_session_wrapper(
            self.db,
            self.tenant_id,
            effective_researcher_id,
        )
        .await?
        {
            Some(session) => {
                // Business Rule: Check if current chunk is complete
                // (All patients in current chunk have judgments)
                let mut chunk_complete = true;
                let mut completed_patients = 0;

                for chunk_patient_id in &session.current_chunk_patients {
                    match self.get_judgment(chunk_patient_id).await? {
                        Some(_) => {
                            completed_patients += 1;
                            info!("✅ Patient {} has judgment", chunk_patient_id);
                        }
                        None => {
                            chunk_complete = false;
                            info!("⏳ Patient {} still needs judgment", chunk_patient_id);
                        }
                    }
                }

                info!(
                    "📊 Research session '{}': {}/{} patients judged in current chunk",
                    session.session_name,
                    completed_patients,
                    session.current_chunk_patients.len()
                );

                if chunk_complete {
                    info!(
                        "🎯 Chunk complete! Progressing to next chunk for session: {}",
                        session.id
                    );

                    let research_service =
                        crate::research_session::service::ResearchSessionService::new(
                            self.db,
                            self.tenant_id,
                            self.config.clone(),
                        );
                    research_service
                        .progress_to_next_chunk_with_summary(effective_researcher_id)
                        .await?;
                    Ok(true)
                } else {
                    info!(
                        "⏳ Chunk still in progress: {}/{} patients judged",
                        completed_patients,
                        session.current_chunk_patients.len()
                    );
                    Ok(false)
                }
            }
            None => {
                info!(
                    "ℹ️ No active research session found - judgment saved but no progression possible"
                );
                Ok(false)
            }
        }
    }

    /// Get judgment - delegates to platform-db
    pub async fn get_judgment(&self, patient_id: &str) -> Result<Option<Judgment>> {
        // Business Rule: Validate patient ID
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }

        // Get patient UUID first
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

        // Use DAL wrapper to maintain three-layer architecture
        match dal::get_judgment_by_patient_id_wrapper(self.db, patient.id, self.tenant_id).await? {
            Some(judgment) => Ok(Some(judgment)),
            None => Ok(None),
        }
    }

    /// Get judgment summary for the active review session of a specific researcher.
    pub async fn get_judgment_summary_for_researcher(
        &self,
        researcher_id: Uuid,
    ) -> Result<JudgmentSummary> {
        let research_session_service =
            crate::research_session::service::ResearchSessionService::new(
                self.db,
                self.tenant_id,
                self.config.clone(),
            );
        let patient_external_ids = research_session_service
            .get_active_session_patient_ids(researcher_id)
            .await?;

        if patient_external_ids.is_empty() {
            return Ok(empty_summary());
        }

        let patients_by_external_id = self
            .db
            .batch_get_patients_by_external_ids(&patient_external_ids, self.tenant_id)
            .await?;

        if patients_by_external_id.is_empty() {
            return Ok(empty_summary());
        }

        let patient_ids: Vec<Uuid> = patient_external_ids
            .iter()
            .filter_map(|patient_external_id| {
                patients_by_external_id
                    .get(patient_external_id)
                    .map(|patient| patient.id)
            })
            .collect();

        let external_ids_by_uuid: HashMap<Uuid, String> = patient_external_ids
            .iter()
            .filter_map(|patient_external_id| {
                patients_by_external_id
                    .get(patient_external_id)
                    .map(|patient| (patient.id, patient_external_id.clone()))
            })
            .collect();

        let judgments_by_patient_id = self
            .db
            .batch_get_judgments_by_patient_ids(&patient_ids, self.tenant_id)
            .await?;

        let mut judgment_distribution = HashMap::new();
        let mut accepted_count = 0;
        let mut needs_review_count = 0;
        let mut uncertain_count = 0;
        let mut recent_judgments = Vec::with_capacity(judgments_by_patient_id.len());

        for (patient_id, judgment_record) in judgments_by_patient_id {
            let Some(patient_external_id) = external_ids_by_uuid.get(&patient_id) else {
                continue;
            };

            match judgment_record.judgment.as_str() {
                "A" => accepted_count += 1,
                "N" => needs_review_count += 1,
                "U" => uncertain_count += 1,
                _ => {}
            }

            *judgment_distribution
                .entry(judgment_record.judgment.clone())
                .or_insert(0) += 1;

            recent_judgments.push(RecentJudgmentActivity {
                patient_id: patient_external_id.clone(),
                judgment: judgment_record.judgment,
                timestamp: judgment_record.judgment_made_at,
            });
        }

        recent_judgments.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));

        Ok(JudgmentSummary {
            total_judgments: recent_judgments.len(),
            accepted_count,
            needs_review_count,
            uncertain_count,
            judgment_distribution,
            recent_judgments,
        })
    }
}

/// Map user-facing judgment values to stored codes.
pub fn map_judgment_to_code(judgment: &str) -> Result<String> {
    let mapped = match judgment.to_lowercase().as_str() {
        "appropriate" | "a" | "accepted" | "applicable" => "A",
        "inappropriate" | "not_appropriate" | "n" | "needs_review" | "needs review"
        | "not applicable" => "N",
        "uncertain" | "u" | "unsure" => "U",
        "flagged" | "f" => "F",
        // Direct codes pass through
        "A" | "N" | "U" | "F" => judgment,
        _ => {
            return Err(PlatformError::invalid_input_field(
                format!(
                    "Invalid judgment value: '{}'. Must be A/Appropriate, N/Needs Review, U/Uncertain, or F/Flagged",
                    judgment
                ),
                "judgment",
            ));
        }
    };

    Ok(mapped.to_string())
}

fn empty_summary() -> JudgmentSummary {
    JudgmentSummary {
        total_judgments: 0,
        accepted_count: 0,
        needs_review_count: 0,
        uncertain_count: 0,
        judgment_distribution: HashMap::new(),
        recent_judgments: Vec::new(),
    }
}
