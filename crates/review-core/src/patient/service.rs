// Patient service layer - business logic and validation
// Contains no direct file I/O - delegates to DAL layer

use platform_db::DatabaseConnection;
use platform_errors::{PlatformError, Result};
// Import presentation models (API DTOs)
use crate::patient::models::*;
// Import canonical database models from platform-models
use crate::config::{Config, PatientSelectionInfo};
use crate::patient::dal;
use crate::research_session::ResearchSessionService;
use log::{info, warn};
use platform_models::ClinicalJournalEntry as PlatformClinicalJournalEntry;
use platform_models::{Patient, PatientNote};
use std::collections::HashMap;
use uuid::Uuid;

/// Patient service with database connection
pub struct PatientService<'a> {
    db: &'a dyn DatabaseConnection,
    tenant_id: Uuid,
    config: Config,
}

impl<'a> PatientService<'a> {
    /// Create new patient service with database connection
    pub fn new(db: &'a dyn DatabaseConnection, tenant_id: Uuid, config: Config) -> Self {
        Self {
            db,
            tenant_id,
            config,
        }
    }

    /// Get patient details by ID with full clinical data
    pub async fn get_patient_details(&self, patient_id: &str) -> Result<Option<PatientRecord>> {
        // Business rule: Validate patient ID format
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }

        // Architecture compliance: Service -> DAL -> platform-db
        // Get patient using thin DAL wrapper
        let patient = match dal::get_patient_by_external_id_thin(
            self.db,
            patient_id,
            self.tenant_id,
        )
        .await?
        {
            Some(patient) => patient,
            None => return Ok(None),
        };

        // Business logic: Aggregate patient data from multiple sources
        // Get patient notes using thin DAL wrapper
        let notes = dal::get_patient_notes_thin(self.db, patient.id, self.tenant_id).await?;

        // Get clinical journal entries using thin DAL wrapper
        let journal_entries =
            dal::get_clinical_journal_entries_thin(self.db, patient.id, self.tenant_id).await?;

        // Business logic: Convert and aggregate data into PatientRecord
        let patient_record = self.convert_to_patient_record(patient, notes, journal_entries)?;

        Ok(Some(patient_record))
    }

    /// Get comprehensive patient details with judgment and navigation info using specific researcher ID
    pub async fn get_patient_details_with_context_and_researcher(
        &self,
        patient_id: &str,
        researcher_id: Uuid,
    ) -> Result<PatientDetailsResponse> {
        // Business rule: Validate patient ID format
        if patient_id.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Patient ID cannot be empty",
                "patient_id",
            ));
        }

        info!("🔍 Loading patient details for: {}", patient_id);

        // Architecture compliance: Service -> DAL -> platform-db
        let platform_patient = match dal::get_patient_by_external_id_thin(
            self.db,
            patient_id,
            self.tenant_id,
        )
        .await?
        {
            Some(patient) => patient,
            None => return Err(PlatformError::not_found("Patient", patient_id)),
        };

        // Business logic: Aggregate patient data from multiple sources using DAL wrappers
        let notes =
            dal::get_patient_notes_thin(self.db, platform_patient.id, self.tenant_id).await?;
        let journal_entries =
            dal::get_clinical_journal_entries_thin(self.db, platform_patient.id, self.tenant_id)
                .await?;

        // Business logic: Convert and aggregate data into PatientRecord
        let patient =
            self.convert_to_patient_record(platform_patient.clone(), notes, journal_entries)?;

        info!("✅ Patient found: {}", patient.id);

        // Architecture compliance: Service -> DAL -> platform-db
        let judgment = match dal::get_judgment_by_patient_id_thin(
            self.db,
            platform_patient.id,
            self.tenant_id,
        )
        .await?
        {
            Some(j) => {
                info!(
                    "✅ Found judgment for patient {}: {}",
                    patient_id, j.judgment
                );
                Some(j.judgment)
            }
            None => {
                info!("ℹ️ No judgment found for patient {}", patient_id);
                None
            }
        };

        // New navigation logic: Get enhanced navigation state
        let navigation_state = {
            info!(
                "Getting enhanced navigation state for patient: {} with researcher: {}",
                patient_id, researcher_id
            );
            // Create ResearchSessionService to get enhanced navigation state
            let research_service =
                ResearchSessionService::new(self.db, self.tenant_id, self.config.clone());

            match research_service
                .get_enhanced_navigation_state(patient_id, researcher_id)
                .await
            {
                Ok(nav_state) => {
                    info!("✅ Enhanced navigation state calculated successfully");
                    Some(nav_state)
                }
                Err(e) => {
                    warn!("⚠️ Failed to get enhanced navigation state: {:?}", e);
                    None
                }
            }
        };

        info!(
            "🔍 Building response - navigation_enabled={}",
            navigation_state.is_some()
        );

        Ok(PatientDetailsResponse {
            patient,
            judgment,
            navigation_state,
        })
    }

    /// Get all patients as summary list for navigation (architecture compliance: Service -> DAL -> platform-db)
    pub async fn get_all_patients_for_researcher(
        &self,
        researcher_id: Uuid,
    ) -> Result<Vec<PatientSummary>> {
        let target_patient_ids =
            match dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?
            {
                Some(session) => {
                    info!(
                        "Using active research session '{}' with {} patients",
                        session.session_name,
                        session.current_chunk_patients.len()
                    );
                    session.current_chunk_patients
                }
                None => {
                    warn!("No active research session found, returning empty patient list");
                    return Ok(Vec::new());
                }
            };

        if target_patient_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Architecture compliance: Service -> DAL -> platform-db
        let platform_summaries =
            dal::get_patients_by_external_ids_thin(self.db, &target_patient_ids, self.tenant_id)
                .await?;

        // Convert platform-models::PatientSummary to presentation PatientSummary
        let patients: Vec<PatientSummary> = platform_summaries
            .into_iter()
            .map(|ps| PatientSummary {
                id: ps.external_id,
                age: ps.age.map(|a| a.to_string()),
                sex: ps.sex,
                has_judgment: ps.has_judgment,
            })
            .collect();

        info!(
            "Loaded {} patients from active research session",
            patients.len()
        );
        Ok(patients)
    }

    pub async fn get_all_patients(&self) -> Result<Vec<PatientSummary>> {
        Err(PlatformError::invalid_input(
            "Researcher context is required to load patients",
        ))
    }

    pub async fn get_patient_selection_info_for_researcher(
        &self,
        researcher_id: Uuid,
    ) -> Result<PatientSelectionInfo> {
        let active_session =
            dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?;

        let Some(session) = active_session else {
            return Ok(PatientSelectionInfo {
                is_filtered: false,
                source_file: None,
                total_patients: 0,
                filtered_patients: 0,
                description: None,
                selected_count: 0,
                total_available: 0,
            });
        };

        let selected_count = session.current_chunk_patients.len() as u32;
        let total_available = session.total_patients.max(selected_count as i32) as u32;

        Ok(PatientSelectionInfo {
            is_filtered: true,
            source_file: Some(session.session_name.clone()),
            total_patients: total_available,
            filtered_patients: selected_count,
            description: Some(format!("active review session '{}'", session.session_name)),
            selected_count,
            total_available,
        })
    }

    pub async fn get_patient_groups_for_researcher(
        &self,
        researcher_id: Uuid,
        group_by: &str,
    ) -> Result<HashMap<String, Vec<String>>> {
        let active_session =
            dal::get_active_research_session_wrapper(self.db, self.tenant_id, researcher_id)
                .await?;

        let Some(session) = active_session else {
            return Ok(HashMap::new());
        };

        if session.current_chunk_patients.is_empty() {
            return Ok(HashMap::new());
        }

        let platform_summaries = dal::get_patients_by_external_ids_thin(
            self.db,
            &session.current_chunk_patients,
            self.tenant_id,
        )
        .await?;

        let mut groups: HashMap<String, Vec<String>> = HashMap::new();

        for patient in platform_summaries {
            let group_key = match group_by {
                "judgment_status" => {
                    if patient.has_judgment {
                        "Reviewed".to_string()
                    } else {
                        "Pending Review".to_string()
                    }
                }
                "age_range" => match patient.age.unwrap_or_default() {
                    ..=17 => "Under 18".to_string(),
                    18..=29 => "18-29".to_string(),
                    30..=49 => "30-49".to_string(),
                    50..=69 => "50-69".to_string(),
                    _ => "70+".to_string(),
                },
                "sex" => match patient.sex.as_deref() {
                    Some("M") => "Male".to_string(),
                    Some("F") => "Female".to_string(),
                    Some(value) if !value.trim().is_empty() => value.to_string(),
                    _ => "Unknown".to_string(),
                },
                _ => "All Patients".to_string(),
            };

            groups
                .entry(group_key)
                .or_default()
                .push(patient.external_id);
        }

        Ok(groups)
    }

    /// Get patients by their external IDs - for research session chunks
    pub async fn get_patients_by_external_ids(
        &self,
        patient_ids: &[String],
    ) -> Result<Vec<PatientSummary>> {
        info!("Loading {} patients by external IDs", patient_ids.len());

        if patient_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Architecture compliance: Service -> DAL -> platform-db
        let platform_summaries =
            dal::get_patients_by_external_ids_thin(self.db, patient_ids, self.tenant_id).await?;

        // Convert platform-models::PatientSummary to presentation PatientSummary
        let patients: Vec<PatientSummary> = platform_summaries
            .into_iter()
            .map(|ps| PatientSummary {
                id: ps.external_id,
                age: ps.age.map(|a| a.to_string()),
                sex: ps.sex,
                has_judgment: ps.has_judgment,
            })
            .collect();

        info!("Loaded {} patients by external IDs", patients.len());
        Ok(patients)
    }

    /// Search patients by various criteria
    pub async fn search_patients_for_researcher(
        &self,
        query: &str,
        researcher_id: Uuid,
    ) -> Result<Vec<PatientSummary>> {
        // Business rule: Minimum search length
        if query.trim().len() < 2 {
            return Err(PlatformError::invalid_input_field(
                "Search query must be at least 2 characters",
                "query",
            ));
        }

        // Delegate to data access layer
        dal::search_patients_by_query(self.db, query, self.tenant_id, researcher_id).await
    }

    /// Get comprehensive patient statistics
    pub async fn get_patient_statistics_for_researcher(
        &self,
        researcher_id: Uuid,
    ) -> Result<PatientStatistics> {
        // Delegate to data access layer
        dal::calculate_patient_statistics(self.db, self.tenant_id, researcher_id).await
    }

    /// Business logic: Convert platform models to presentation PatientRecord format
    /// This aggregation logic was moved from DAL to Service for architecture compliance
    fn convert_to_patient_record(
        &self,
        patient: Patient,
        notes: Vec<PatientNote>,
        journal_entries: Vec<PlatformClinicalJournalEntry>,
    ) -> Result<PatientRecord> {
        // Business logic: Group notes by category
        let mut past_history = Vec::new();
        let mut medication = Vec::new();
        let mut allergies = Vec::new();
        let mut recent_history = Vec::new();
        let mut medical_examination = Vec::new();

        for note in notes {
            match note.category.as_str() {
                "past_history" => past_history.push(note.content),
                "medication" => medication.push(note.content),
                "allergies" => allergies.push(note.content),
                "recent_history" => recent_history.push(note.content),
                "medical_examination" => medical_examination.push(note.content),
                _ => {
                    return Err(PlatformError::invalid_input_field(
                        format!("Unsupported patient note category `{}`", note.category),
                        "note.category",
                    ));
                }
            }
        }

        // Business logic: Convert clinical journal entries to presentation format
        let clinical_journal: Vec<crate::patient::models::ClinicalJournalEntry> = journal_entries
            .into_iter()
            .map(|entry| crate::patient::models::ClinicalJournalEntry {
                role: entry.role,
                timestamp: Some(entry.entry_timestamp),
                content: entry.content,
            })
            .collect();

        // Business logic: Assemble final PatientRecord presentation format
        Ok(PatientRecord {
            id: patient.external_id,
            age: patient.age.map(|a| a.to_string()),
            sex: patient.sex,
            past_history,
            medication,
            allergies,
            recent_history,
            medical_examination,
            clinical_journal,
        })
    }
}
