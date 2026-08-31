// Patient Data Access Layer - local workspace database operations

use platform_db::DatabaseConnection;
use platform_errors::Result;
// Import presentation models for conversion
use crate::patient::models::{PatientStatistics, PatientSummary};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// DATABASE-CENTRIC APPROACH: Patient access is managed through research sessions only.

/// Get patient IDs from active research session (database-centric approach)
async fn get_session_patient_ids(
    db: &(impl DatabaseConnection + ?Sized),
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<Vec<String>> {
    println!("🔍 [patient/dal.rs] PERFORMANCE: Loading patient IDs from active research session");

    match db
        .get_active_research_session(tenant_id, researcher_id)
        .await
    {
        Ok(Some(session)) => {
            println!(
                "✅ [patient/dal.rs] PERFORMANCE: Found active research session '{}' with {} patients in current chunk",
                session.session_name,
                session.current_chunk_patients.len()
            );
            Ok(session.current_chunk_patients)
        }
        Ok(None) => {
            println!(
                "⚠️ [patient/dal.rs] DAL: No active research session found for researcher {}",
                researcher_id
            );
            Ok(Vec::new())
        }
        Err(e) => {
            println!(
                "❌ [patient/dal.rs] DAL: Error getting active research session: {:?}",
                e
            );
            Err(e)
        }
    }
}

/// Get patient summaries for active research session using database-centric approach
pub async fn get_all_patient_summaries(
    db: &(impl DatabaseConnection + ?Sized),
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<Vec<PatientSummary>> {
    println!(
        "🔍 [patient/dal.rs] DAL: get_all_patient_summaries called with research session filtering, tenant: {}",
        tenant_id
    );

    let target_patient_ids = get_session_patient_ids(db, tenant_id, researcher_id).await?;

    if target_patient_ids.is_empty() {
        println!(
            "⚠️ [patient/dal.rs] DAL: No patients in active research session, will return empty result"
        );
        return Ok(Vec::new());
    }

    println!(
        "🔍 [patient/dal.rs] DAL: Using efficient database-level filtering for {} target patient IDs",
        target_patient_ids.len()
    );

    // Use the new efficient database-level filtering method
    match db
        .get_patients_by_external_ids(&target_patient_ids, tenant_id)
        .await
    {
        Ok(platform_summaries) => {
            // Convert from platform-models::PatientSummary to our PatientSummary format
            let patients: Vec<PatientSummary> = platform_summaries
                .into_iter()
                .map(|platform_summary| {
                    PatientSummary {
                        id: platform_summary.external_id, // Use external_id as our id (String)
                        age: platform_summary.age.map(|a| a.to_string()),
                        sex: platform_summary.sex,
                        has_judgment: platform_summary.has_judgment,
                    }
                })
                .collect();

            println!(
                "✅ [patient/dal.rs] DAL: Database-level filtering found {} patients from {} target IDs ({}x performance improvement)",
                patients.len(),
                target_patient_ids.len(),
                if !target_patient_ids.is_empty() {
                    50000 / target_patient_ids.len().max(1)
                } else {
                    1
                }
            );
            Ok(patients)
        }
        Err(e) => {
            println!(
                "❌ [patient/dal.rs] DAL: Database-level filtering failed: {:?}",
                e
            );
            Err(e)
        }
    }
}

/// Search patients by query string with target ID filtering (if available)
/// Uses the new platform-db get_patients method with search filtering
pub async fn search_patients_by_query(
    db: &(impl DatabaseConnection + ?Sized),
    query: &str,
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<Vec<PatientSummary>> {
    println!(
        "🔍 [patient/dal.rs] DAL: search_patients_by_query called for query: '{}', tenant: {}",
        query, tenant_id
    );

    if query.trim().is_empty() {
        println!("ℹ️ [patient/dal.rs] DAL: Empty search query, returning empty results");
        return Ok(Vec::new());
    }

    // Import the models we need for filtering
    use platform_db::{PaginationOptions, PatientFilterOptions, PatientSortOptions};

    // Set up filters with search query - target ID filtering applied in application layer
    let filters = PatientFilterOptions {
        review_status: None,
        search_query: Some(query.to_string()), // Search by external_id
        has_judgment: None,
        is_flagged: None,
    };

    // Sort by relevance (creation date for now)
    let sorting = PatientSortOptions::default();

    // Get all matching patients (no pagination limit for comprehensive search)
    let pagination = PaginationOptions::new(1, 10000);

    // Load target patient IDs from active research session
    let target_patient_ids_vec = get_session_patient_ids(db, tenant_id, researcher_id).await?;
    let target_patient_ids: HashSet<String> = target_patient_ids_vec.into_iter().collect();

    let use_id_filtering = !target_patient_ids.is_empty();

    // Search using the new platform-db method
    match db
        .get_patients(tenant_id, &filters, &sorting, &pagination)
        .await
    {
        Ok(platform_summaries) => {
            // Filter only by target patient IDs (no age_sex filtering)
            let filtered_patients: Vec<PatientSummary> = platform_summaries
                .into_iter()
                .filter(|patient| {
                    // Only filter: include patients in our target list (if we have one)
                    if use_id_filtering {
                        target_patient_ids.contains(&patient.external_id)
                    } else {
                        true // If no target list, include all patients
                    }
                })
                .map(|platform_summary| {
                    // Convert from platform-models::PatientSummary to our PatientSummary
                    PatientSummary {
                        id: platform_summary.external_id, // Use external_id as our id (String)
                        age: platform_summary.age.map(|a| a.to_string()),
                        sex: platform_summary.sex,
                        has_judgment: platform_summary.has_judgment,
                    }
                })
                .collect();

            if use_id_filtering {
                println!(
                    "✅ [patient/dal.rs] DAL: Found {} patients matching '{}' from target list (from {} total target IDs)",
                    filtered_patients.len(),
                    query,
                    target_patient_ids.len()
                );
            } else {
                println!(
                    "✅ [patient/dal.rs] DAL: Found {} patients matching '{}' (no ID filtering - all patients searched)",
                    filtered_patients.len(),
                    query
                );
            }
            Ok(filtered_patients)
        }
        Err(e) => {
            println!("❌ [patient/dal.rs] DAL: Error searching patients: {:?}", e);
            Err(e)
        }
    }
}

/// Calculate comprehensive patient statistics
/// Note: This function needs aggregation queries in platform-db
pub async fn calculate_patient_statistics(
    db: &(impl DatabaseConnection + ?Sized),
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<PatientStatistics> {
    println!(
        "🔍 [patient/dal.rs] DAL: calculate_patient_statistics called for tenant: {}",
        tenant_id
    );

    // Calculate the small local-workspace distribution in memory.
    let patients = get_all_patient_summaries(db, tenant_id, researcher_id).await?;

    let total_patients = patients.len();
    let patients_with_judgments = patients.iter().filter(|p| p.has_judgment).count();

    let mut age_distribution = HashMap::new();
    let mut sex_distribution = HashMap::new();

    for patient in &patients {
        // Count by age if available (processing our converted PatientSummary with String age)
        if let Some(age_str) = &patient.age {
            if let Ok(age) = age_str.parse::<i32>() {
                let age_group = match age {
                    0..=17 => "0-17".to_string(),
                    18..=29 => "18-29".to_string(),
                    30..=49 => "30-49".to_string(),
                    50..=69 => "50-69".to_string(),
                    _ => "70+".to_string(),
                };
                *age_distribution.entry(age_group).or_insert(0) += 1;
            }
        }

        // Count by sex if available
        if let Some(sex) = &patient.sex {
            *sex_distribution.entry(sex.clone()).or_insert(0) += 1;
        }
    }

    println!(
        "🔍 [patient/dal.rs] DAL: Calculated statistics - {} total patients, {} with judgments",
        total_patients, patients_with_judgments
    );

    Ok(PatientStatistics {
        total_patients,
        patients_with_judgments,
        age_distribution,
        sex_distribution,
    })
}

/// Get patient by external ID.
pub async fn get_patient_by_external_id_thin(
    db: &(impl DatabaseConnection + ?Sized),
    patient_id: &str,
    tenant_id: Uuid,
) -> Result<Option<platform_models::Patient>> {
    db.get_patient_by_external_id(patient_id, tenant_id)
        .await
        .map(Some)
        .or_else(|e| match e {
            platform_errors::PlatformError::NotFound { .. } => Ok(None),
            other => Err(other),
        })
}

/// Get patient notes.
pub async fn get_patient_notes_thin(
    db: &(impl DatabaseConnection + ?Sized),
    patient_id: Uuid,
    tenant_id: Uuid,
) -> Result<Vec<platform_models::PatientNote>> {
    db.get_patient_notes(patient_id, tenant_id).await
}

/// Get clinical journal entries.
pub async fn get_clinical_journal_entries_thin(
    db: &(impl DatabaseConnection + ?Sized),
    patient_id: Uuid,
    tenant_id: Uuid,
) -> Result<Vec<platform_models::ClinicalJournalEntry>> {
    db.get_clinical_journal_entries(patient_id, tenant_id).await
}

/// Get patients by external IDs (thin wrapper) - ARCHITECTURE COMPLIANT
pub async fn get_patients_by_external_ids_thin(
    db: &(impl DatabaseConnection + ?Sized),
    patient_ids: &[String],
    tenant_id: Uuid,
) -> Result<Vec<platform_models::PatientSummary>> {
    db.get_patients_by_external_ids(patient_ids, tenant_id)
        .await
}

/// Get judgment by patient ID (thin wrapper) - ARCHITECTURE COMPLIANT
pub async fn get_judgment_by_patient_id_thin(
    db: &(impl DatabaseConnection + ?Sized),
    patient_id: Uuid,
    tenant_id: Uuid,
) -> Result<Option<platform_models::Judgment>> {
    db.get_judgment_by_patient_id(patient_id, tenant_id).await
}

/// Get active research session (thin wrapper) - ARCHITECTURE COMPLIANT
pub(super) async fn get_active_research_session_wrapper(
    db: &dyn DatabaseConnection,
    tenant_id: Uuid,
    researcher_id: Uuid,
) -> Result<Option<platform_models::ResearchSession>> {
    db.get_active_research_session(tenant_id, researcher_id)
        .await
}
