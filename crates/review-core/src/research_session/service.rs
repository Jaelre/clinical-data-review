// Research session service for cohort-backed patient batch processing.

use crate::config::Config;
use crate::patient::models::{NavigationState, ProgressSummary};
use crate::research_session::dal;
use log::info;
use platform_db::DatabaseConnection;
use platform_errors::{PlatformError, Result};
use platform_models::ResearchSession;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Research session summary DTO for summary and batch progress UIs.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ResearchSessionSummary {
    pub active_chunk: Option<ResearchSessionActiveChunkSummary>,
    pub completed_chunks: Vec<i32>,
    pub total_patients: usize,
    pub judged_patients: usize,
}

/// Active chunk summary DTO for the current session.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ResearchSessionActiveChunkSummary {
    pub id: i32,
    pub patient_ids: Vec<String>,
    pub completed_patients: usize,
    pub total_patients: usize,
}

/// Helper struct for chunk completion status (private)
#[derive(Debug)]
struct ChunkCompletionStatus {
    is_chunk_complete: bool,
    unjudged_count: usize,
}

/// Research session service with database connection
/// Manages chunk-based patient processing workflows
pub struct ResearchSessionService<'a> {
    db: &'a dyn DatabaseConnection,
    tenant_id: Uuid,
}

impl<'a> ResearchSessionService<'a> {
    /// Create new research session service with database connection
    pub fn new(db: &'a dyn DatabaseConnection, tenant_id: Uuid, _config: Config) -> Self {
        Self { db, tenant_id }
    }

    /// Get active research session for current researcher
    pub async fn get_active_research_session(
        &self,
        researcher_id: Uuid,
    ) -> Result<Option<ResearchSession>> {
        // Use DAL wrapper
        dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id).await
    }

    /// Get current session state for UI initialization
    /// Returns the active session with patient data, or None if no active session exists
    pub async fn get_current_session_state(
        &self,
        researcher_id: Uuid,
    ) -> Result<Option<ResearchSession>> {
        info!(
            "Getting current session state for researcher: {}",
            researcher_id
        );

        // Get the active research session for this user
        let session =
            dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?;

        match session {
            Some(session) => {
                info!(
                    "Found active session {} with status: {}",
                    session.id, session.status
                );
                Ok(Some(session))
            }
            None => {
                info!("No active session found for researcher {}", researcher_id);
                Ok(None)
            }
        }
    }

    /// Get all patient external IDs that belong to the active session scope.
    /// This is the full ETL-authored cohort order.
    pub async fn get_active_session_patient_ids(&self, researcher_id: Uuid) -> Result<Vec<String>> {
        let session =
            match dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?
            {
                Some(session) => session,
                None => return Ok(Vec::new()),
            };

        self.get_patient_ids_for_session(&session).await
    }

    /// Get a session summary with real judged counts for both the active chunk
    /// and the entire active review session.
    pub async fn get_session_summary(
        &self,
        researcher_id: Uuid,
    ) -> Result<Option<ResearchSessionSummary>> {
        let session =
            match dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?
            {
                Some(session) => session,
                None => return Ok(None),
            };

        let all_patient_ids = self.get_patient_ids_for_session(&session).await?;
        let judged_patients_by_external_id = self
            .get_judged_patients_by_external_id(&all_patient_ids)
            .await?;

        let completed_patients = session
            .current_chunk_patients
            .iter()
            .filter(|patient_id| judged_patients_by_external_id.contains_key(*patient_id))
            .count();

        Ok(Some(ResearchSessionSummary {
            active_chunk: Some(ResearchSessionActiveChunkSummary {
                id: session.current_chunk_number,
                patient_ids: session.current_chunk_patients.clone(),
                completed_patients,
                total_patients: session.current_chunk_patients.len(),
            }),
            completed_chunks: session.completed_chunks.clone(),
            total_patients: all_patient_ids.len(),
            judged_patients: judged_patients_by_external_id.len(),
        }))
    }

    async fn get_cohort_batches_for_session(
        &self,
        session: &ResearchSession,
    ) -> Result<Vec<platform_models::ResearchCohortBatch>> {
        let cohort_id = session.cohort_id.ok_or_else(|| {
            PlatformError::unprocessable_entity_with_reason(
                "This session is not linked to an ETL-ingested cohort.",
                "no_cohort_batches",
            )
        })?;

        let batches = dal::get_cohort_batches_wrapper(self.db, cohort_id, self.tenant_id).await?;
        if batches.iter().all(|batch| batch.is_empty) {
            return Err(PlatformError::unprocessable_entity_with_reason(
                "This cohort has no pre-composed batches. Re-ingest the cohort data before continuing review.",
                "no_cohort_batches",
            ));
        }

        Ok(batches)
    }

    async fn get_patient_ids_for_session(&self, session: &ResearchSession) -> Result<Vec<String>> {
        let ordered_patient_ids = self
            .get_cohort_batches_for_session(session)
            .await?
            .into_iter()
            .filter(|batch| !batch.is_empty)
            .flat_map(|batch| batch.patient_external_ids)
            .collect();

        Ok(deduplicate_preserving_order(ordered_patient_ids))
    }

    async fn get_judged_patients_by_external_id(
        &self,
        patient_external_ids: &[String],
    ) -> Result<HashMap<String, platform_models::Judgment>> {
        if patient_external_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let patients_by_external_id = dal::batch_get_patients_by_external_ids_wrapper(
            self.db,
            patient_external_ids,
            self.tenant_id,
        )
        .await?;

        if patients_by_external_id.is_empty() {
            return Ok(HashMap::new());
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

        let judgments_by_patient_id =
            dal::batch_get_judgments_by_patient_ids_wrapper(self.db, &patient_ids, self.tenant_id)
                .await?;

        Ok(judgments_by_patient_id
            .into_iter()
            .filter_map(|(patient_id, judgment)| {
                external_ids_by_uuid
                    .get(&patient_id)
                    .cloned()
                    .map(|external_id| (external_id, judgment))
            })
            .collect())
    }

    /// Check completion status of current chunk (private helper)
    async fn check_chunk_completion(
        &self,
        session: &ResearchSession,
    ) -> Result<ChunkCompletionStatus> {
        let mut unjudged_count = 0;

        for patient_id in &session.current_chunk_patients {
            // Check if patient has judgment using DAL wrappers
            let patient =
                dal::get_patient_by_external_id_wrapper(self.db, patient_id, self.tenant_id)
                    .await?;
            let has_judgment =
                (dal::get_judgment_by_patient_id_wrapper(self.db, patient.id, self.tenant_id)
                    .await?)
                    .is_some();

            if !has_judgment {
                unjudged_count += 1;
            }
        }

        Ok(ChunkCompletionStatus {
            is_chunk_complete: unjudged_count == 0,
            unjudged_count,
        })
    }

    /// Get navigation state for a patient in the active review session.
    pub async fn get_enhanced_navigation_state(
        &self,
        patient_external_id: &str,
        researcher_id: Uuid,
    ) -> Result<NavigationState> {
        info!(
            "Getting enhanced navigation state for patient {} by researcher {}",
            patient_external_id, researcher_id
        );

        // Get active research session
        let session =
            match dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?
            {
                Some(session) => session,
                None => {
                    // No active session - patient is not in any chunk
                    return Ok(NavigationState {
                        // Core state
                        is_in_active_chunk: false,
                        is_chunk_complete: false,
                        is_session_complete: false,
                        current_position: None,
                        total_in_chunk: None,
                        unjudged_count: None,

                        // Pre-computed UI state
                        counter_display: "No active session".to_string(),

                        // Previous button (disabled - no navigation context)
                        previous_button_enabled: false,
                        previous_button_text: "← Previous".to_string(),
                        previous_patient_id: None,

                        // Next button (return to index)
                        next_button_enabled: true,
                        next_button_text: "Return to Index".to_string(),
                        next_button_action: "return_to_index".to_string(),
                        next_patient_id: None,

                        // Status and guidance
                        status_message: "No active research session".to_string(),
                        help_text: Some(
                            "Start a new research session from the index page".to_string(),
                        ),
                        loading_text: None,
                    });
                }
            };

        // Check if patient is in current active chunk
        let patient_position = session
            .current_chunk_patients
            .iter()
            .position(|id| id == patient_external_id);

        if patient_position.is_none() {
            // Patient not in active chunk - show warning with smart UI state
            let chunk_size = session.current_chunk_patients.len();
            return Ok(NavigationState {
                // Core state
                is_in_active_chunk: false,
                is_chunk_complete: false,
                is_session_complete: false,
                current_position: None,
                total_in_chunk: Some(chunk_size),
                unjudged_count: None,

                // Pre-computed UI state
                counter_display: format!("Not in active batch (batch has {} patients)", chunk_size),

                // Previous button (disabled - not in active flow)
                previous_button_enabled: false,
                previous_button_text: "← Previous".to_string(),
                previous_patient_id: None,

                // Next button (return to active batch)
                next_button_enabled: true,
                next_button_text: "Return to Active Batch".to_string(),
                next_button_action: "return_to_active_batch".to_string(),
                next_patient_id: None,

                // Status and guidance
                status_message: "Patient not in current research batch".to_string(),
                help_text: Some("This patient is not part of the current active research batch. Return to the active batch to continue reviewing.".to_string()),
                loading_text: None,

            });
        }

        let position = patient_position.unwrap();

        // Check chunk completion status
        let chunk_completion = self.check_chunk_completion(&session).await?;

        // Determine if entire session is complete
        let is_session_complete = self.is_session_complete(&session).await?;

        // Smart backend computation: Calculate ALL UI state based on position and completion status
        let current_position_1based = position + 1; // 1-based for UI
        let total_patients = session.current_chunk_patients.len();
        let previous_patient_id = position
            .checked_sub(1)
            .and_then(|previous_position| session.current_chunk_patients.get(previous_position))
            .cloned();
        let next_unjudged_patient_id = self.find_next_unjudged_patient(&session, position).await?;

        // Compute next button state and text with intelligent business logic
        let (next_button_text, next_button_action, next_button_enabled) =
            if chunk_completion.is_chunk_complete {
                if is_session_complete {
                    (
                        "Session Complete".to_string(),
                        "session_complete".to_string(),
                        false,
                    )
                } else {
                    (
                        "Load Next Batch →".to_string(),
                        "progress_to_next_chunk".to_string(),
                        true,
                    )
                }
            } else {
                // Find next unjudged patient
                match next_unjudged_patient_id.as_ref() {
                    Some(_) => (
                        "Next Unjudged Patient →".to_string(),
                        "navigate_next_unjudged".to_string(),
                        true,
                    ),
                    None => {
                        // No more unjudged patients, chunk should be complete
                        if position + 1 < session.current_chunk_patients.len() {
                            (
                                "Next Patient →".to_string(),
                                "navigate_next_unjudged".to_string(),
                                true,
                            )
                        } else {
                            (
                                "Review Incomplete".to_string(),
                                "review_incomplete".to_string(),
                                false,
                            )
                        }
                    }
                }
            };

        // Compute previous button state (smart backend logic)
        let previous_button_enabled = current_position_1based > 1;
        let previous_button_text = "← Previous".to_string();

        // Compute counter display (backend intelligence)
        let counter_display = format!("{} of {}", current_position_1based, total_patients);

        // Compute status message (contextual backend intelligence)
        let status_message = if chunk_completion.is_chunk_complete {
            if is_session_complete {
                "Research session complete".to_string()
            } else {
                format!(
                    "Batch {} complete - ready for next batch",
                    session.current_chunk_number
                )
            }
        } else {
            format!(
                "Review in progress ({} of {} patients judged)",
                total_patients - chunk_completion.unjudged_count,
                total_patients
            )
        };

        // Compute help text (contextual guidance)
        let help_text = if chunk_completion.is_chunk_complete && !is_session_complete {
            Some("All patients in this batch have been reviewed. Click 'Load Next Batch' to continue.".to_string())
        } else if chunk_completion.unjudged_count > 0 {
            Some(format!(
                "{} patients remaining in this batch.",
                chunk_completion.unjudged_count
            ))
        } else {
            None
        };

        Ok(NavigationState {
            // Core state
            is_in_active_chunk: true,
            is_chunk_complete: chunk_completion.is_chunk_complete,
            is_session_complete,
            current_position: Some(current_position_1based),
            total_in_chunk: Some(total_patients),
            unjudged_count: Some(chunk_completion.unjudged_count),

            // Pre-computed UI state (HIGH IMPACT - eliminates ALL frontend logic)
            counter_display,

            // Previous button state (fully computed)
            previous_button_enabled,
            previous_button_text,
            previous_patient_id,

            // Next button state (fully computed)
            next_button_enabled,
            next_button_text: next_button_text.clone(),
            next_button_action: next_button_action.clone(),
            next_patient_id: if next_button_action == "navigate_next_unjudged" {
                next_unjudged_patient_id
            } else {
                None
            },

            // Status and guidance (MEDIUM PRIORITY)
            status_message,
            help_text,
            loading_text: None, // Set by frontend during operations
        })
    }

    /// Find next unjudged patient in session starting from current position
    pub async fn get_next_unjudged_patient_id(
        &self,
        current_patient_id: &str,
        researcher_id: Uuid,
    ) -> Result<Option<String>> {
        info!(
            "Finding next unjudged patient after {} for researcher {}",
            current_patient_id, researcher_id
        );

        let session =
            match dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?
            {
                Some(session) => session,
                None => {
                    return Err(PlatformError::not_found(
                        "ResearchSession",
                        "No active research session found for user",
                    ));
                }
            };

        // Find current patient's position
        let current_position = session
            .current_chunk_patients
            .iter()
            .position(|id| id == current_patient_id)
            .ok_or_else(|| {
                PlatformError::not_found("Patient in current chunk", current_patient_id)
            })?;

        // Look for next unjudged patient
        self.find_next_unjudged_patient(&session, current_position)
            .await
    }

    /// Progress to next chunk with detailed progress summary.
    /// Reads ETL-authored cohort batches and stores only user-specific progress in the session.
    pub async fn progress_to_next_chunk_with_summary(
        &self,
        researcher_id: Uuid,
    ) -> Result<(ResearchSession, ProgressSummary)> {
        info!("Progressing to next chunk for researcher {}", researcher_id);

        let current_session =
            match dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?
            {
                Some(session) => session,
                None => {
                    return Err(PlatformError::not_found(
                        "ResearchSession",
                        "No active research session found for user",
                    ));
                }
            };

        // Verify current chunk is complete before progressing
        let chunk_completion = self.check_chunk_completion(&current_session).await?;
        if !chunk_completion.is_chunk_complete {
            return Err(PlatformError::conflict_with_details(
                "Current chunk is not complete",
                format!(
                    "{} unjudged patients remaining",
                    chunk_completion.unjudged_count
                ),
            ));
        }

        let batches = self
            .get_cohort_batches_for_session(&current_session)
            .await?;

        let current_batch_number = current_session.current_chunk_number;
        let completed_batches: std::collections::HashSet<i32> = current_session
            .completed_chunks
            .iter()
            .copied()
            .chain(std::iter::once(current_batch_number))
            .collect();

        // Compute totals for progress summary
        let total_non_empty_patients: usize = batches
            .iter()
            .filter(|b| !b.is_empty)
            .map(|b| b.patient_external_ids.len())
            .sum();

        let patients_in_completed: usize = batches
            .iter()
            .filter(|b| !b.is_empty && completed_batches.contains(&b.batch_number))
            .map(|b| b.patient_external_ids.len())
            .sum();

        // Find the next non-empty batch in ETL-authored order
        let next_batch = batches
            .iter()
            .filter(|b| b.batch_number > current_batch_number && !b.is_empty)
            .min_by_key(|b| b.batch_number);

        let completion_percentage =
            (patients_in_completed as f64 / total_non_empty_patients as f64) * 100.0;

        match next_batch {
            None => {
                // All non-empty batches done — complete the session
                let completed_session = dal::complete_research_session_wrapper(
                    self.db,
                    current_session.id,
                    self.tenant_id,
                )
                .await?;

                info!(
                    "Session {} completed after batch {}",
                    current_session.id, current_batch_number
                );

                let progress_summary = ProgressSummary {
                    previous_chunk_number: current_batch_number,
                    patients_completed: patients_in_completed,
                    total_patients: total_non_empty_patients,
                    completion_percentage: 100.0,
                    next_chunk_number: None,
                    next_chunk_size: None,
                };

                Ok((completed_session, progress_summary))
            }
            Some(next) => {
                // Sync session-local progress to the next ETL-authored batch.
                let updated_session = dal::update_research_session_chunk_wrapper(
                    self.db,
                    current_session.id,
                    self.tenant_id,
                    next.batch_number,
                    next.patient_external_ids.clone(),
                    current_batch_number,
                )
                .await?;

                info!(
                    "Advanced session {} from batch {} to batch {} ({} patients)",
                    current_session.id,
                    current_batch_number,
                    next.batch_number,
                    next.patient_external_ids.len()
                );

                let progress_summary = ProgressSummary {
                    previous_chunk_number: current_batch_number,
                    patients_completed: patients_in_completed,
                    total_patients: total_non_empty_patients,
                    completion_percentage,
                    next_chunk_number: Some(next.batch_number),
                    next_chunk_size: Some(next.patient_external_ids.len()),
                };

                Ok((updated_session, progress_summary))
            }
        }
    }

    /// Check if entire session is complete (all non-empty batches processed)
    async fn is_session_complete(&self, session: &ResearchSession) -> Result<bool> {
        let batches = self.get_cohort_batches_for_session(session).await?;

        if batches.is_empty() {
            return Ok(false);
        }

        // Session is on the final review batch when there are no later non-empty batches.
        let has_remaining_batches = batches
            .iter()
            .filter(|b| !b.is_empty)
            .any(|b| b.batch_number > session.current_chunk_number);

        Ok(!has_remaining_batches)
    }

    /// Find next unjudged patient starting from given position
    async fn find_next_unjudged_patient(
        &self,
        session: &ResearchSession,
        start_position: usize,
    ) -> Result<Option<String>> {
        let total_patients = session.current_chunk_patients.len();
        if total_patients <= 1 {
            return Ok(None);
        }

        // Search the rest of the current chunk in cyclic order, excluding the
        // current patient, so "next unjudged" can wrap from the end of the
        // batch back to earlier remaining patients.
        for offset in 1..total_patients {
            let index = (start_position + offset) % total_patients;
            let patient_id = &session.current_chunk_patients[index];
            let patient =
                dal::get_patient_by_external_id_wrapper(self.db, patient_id, self.tenant_id)
                    .await?;
            let has_judgment =
                dal::get_judgment_by_patient_id_wrapper(self.db, patient.id, self.tenant_id)
                    .await?
                    .is_some();

            if !has_judgment {
                info!(
                    "Found next unjudged patient at position {}: {}",
                    index + 1,
                    patient_id
                );
                return Ok(Some(patient_id.clone()));
            }
        }

        // No unjudged patients found after current position
        Ok(None)
    }
}

fn deduplicate_preserving_order(patient_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered_unique = Vec::with_capacity(patient_ids.len());

    for patient_id in patient_ids {
        if seen.insert(patient_id.clone()) {
            ordered_unique.push(patient_id);
        }
    }

    ordered_unique
}
