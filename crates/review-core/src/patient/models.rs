// Patient presentation models - API response DTOs that aggregate platform-models data
// These are presentation layer models, not database entities
// Database entities are provided by platform-models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Core patient record with demographics and clinical data (aggregated from platform-models)
/// This is a presentation DTO that combines Patient + PatientNote + ClinicalJournalEntry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatientRecord {
    pub id: String,
    pub age: Option<String>,
    pub sex: Option<String>,
    #[serde(rename = "pastHistory")]
    pub past_history: Vec<String>,
    pub medication: Vec<String>,
    pub allergies: Vec<String>,
    #[serde(rename = "recentHistory")]
    pub recent_history: Vec<String>,
    #[serde(rename = "medicalExamination")]
    pub medical_examination: Vec<String>,
    #[serde(rename = "clinicalJournal")]
    pub clinical_journal: Vec<ClinicalJournalEntry>,
}

/// Clinical journal entry for presentation (matches platform-models::ClinicalJournalEntry format)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClinicalJournalEntry {
    pub role: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub content: String,
}

/// Patient summary for list views and navigation (mirrors platform-models::PatientSummary)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatientSummary {
    pub id: String,
    pub age: Option<String>,
    pub sex: Option<String>,
    pub has_judgment: bool,
}

/// Patient data aggregated for statistics and reporting
#[derive(Debug, Serialize, Deserialize)]
pub struct PatientStatistics {
    pub total_patients: usize,
    pub patients_with_judgments: usize,
    pub age_distribution: HashMap<String, usize>,
    pub sex_distribution: HashMap<String, usize>,
}

/// Response model for patient details API (aggregates multiple platform-models)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatientDetailsResponse {
    pub patient: PatientRecord,
    pub judgment: Option<String>,
    pub navigation_state: Option<NavigationState>,
}

/// Patient context information for enhanced API responses
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatientContext {
    pub has_judgment: bool,
    pub judgment_value: Option<String>,
    pub admin_review_flag: bool,
    pub last_viewed: Option<DateTime<Utc>>,
}

/// Navigation state for guided navigation system
/// Backend pre-computes ALL UI state following "Dumb Frontend, Smart Backend" principle
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NavigationState {
    pub is_in_active_chunk: bool,
    pub is_chunk_complete: bool,
    pub is_session_complete: bool,
    pub current_position: Option<usize>,
    pub total_in_chunk: Option<usize>,
    pub unjudged_count: Option<usize>,

    pub counter_display: String,

    pub previous_button_enabled: bool,
    pub previous_button_text: String,
    pub previous_patient_id: Option<String>,

    pub next_button_enabled: bool,
    pub next_button_text: String,
    pub next_button_action: String,
    pub next_patient_id: Option<String>,

    pub status_message: String,
    pub help_text: Option<String>,

    pub loading_text: Option<String>,
}

/// Progress summary when advancing to next chunk
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProgressSummary {
    pub previous_chunk_number: i32,
    pub patients_completed: usize,
    pub total_patients: usize,
    pub completion_percentage: f64,
    pub next_chunk_number: Option<i32>,
    pub next_chunk_size: Option<usize>,
}
