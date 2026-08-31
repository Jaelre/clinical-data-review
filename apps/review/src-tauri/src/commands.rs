// Thin Tauri command handlers backed by review-core services.
// These handlers contain NO business logic - they only delegate to review-core services
use crate::app_state::AppState;
use crate::app_state::LocalOperatorSummary;
use crate::command_helpers::{extract_app_context, run_command};
use platform_errors::{PlatformError, Result};
pub use review_core::admin_flag::AdminFlagSummary;
pub use review_core::judgment::JudgmentSummary;
pub use review_core::patient::{PatientStatistics, PatientSummary};
pub use review_core::Config;
use review_core::{admin_flag, config, judgment, patient, research_session};
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;
// Import canonical models from platform-models
pub use platform_models::{AdminFlag, Judgment};

fn empty_research_session_payload() -> serde_json::Value {
    serde_json::json!({
        "active_chunk": null,
        "completed_chunks": [],
        "total_patients": 0,
        "judged_patients": 0
    })
}

fn empty_current_session_state_payload() -> serde_json::Value {
    serde_json::json!({
        "session_id": null,
        "has_active_session": false
    })
}

const PATIENT_DETAILS_RATE_LIMIT_INTERVAL_MS: u64 = 10;

fn log_patient_details_rate_limit_blocked(elapsed_ms: u64) {
    log::debug!(
        "Request blocked by patient-details rate limit: {}ms since last request (minimum {}ms)",
        elapsed_ms,
        PATIENT_DETAILS_RATE_LIMIT_INTERVAL_MS
    );
}

/// Get patient details by ID
#[tauri::command]
pub async fn get_patient_details(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
) -> Result<patient::models::PatientDetailsResponse> {
    // CRASH PREVENTION: Backend rate limiting to prevent connection pool exhaustion
    static LAST_REQUEST_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let last_time = LAST_REQUEST_TIME.load(std::sync::atomic::Ordering::Relaxed);
    let time_since_last = now - last_time;

    if time_since_last < PATIENT_DETAILS_RATE_LIMIT_INTERVAL_MS {
        log_patient_details_rate_limit_blocked(time_since_last);
        return Err(PlatformError::patient_details_rate_limited());
    }

    LAST_REQUEST_TIME.store(now, std::sync::atomic::Ordering::Relaxed);

    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service =
            patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
        service
            .get_patient_details_with_context_and_researcher(&patient_id, operator_id)
            .await
    })
    .await
}
/// Save patient judgment
#[tauri::command]
pub async fn save_patient_judgment(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
    judgment: String,
) -> Result<()> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;
        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };
        let service =
            judgment::service::JudgmentService::new(&*db, tenant_id, config::Config::default());
        service
            .save_judgment(&patient_id, &judgment, Some(operator_id))
            .await
    })
    .await
}
/// Get patient judgment
#[tauri::command]
pub async fn get_patient_judgment(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
) -> Result<Option<Judgment>> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;
        // ARCHITECTURAL COMPLIANCE: Route through JudgmentService instead of direct DAL access
        let service =
            judgment::service::JudgmentService::new(&*db, tenant_id, config::Config::default());
        service.get_judgment(&patient_id).await
    })
    .await
}
/// Get judgment summary
#[tauri::command]
pub async fn get_judgment_summary(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<JudgmentSummary> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service =
            judgment::service::JudgmentService::new(&*db, tenant_id, config::Config::default());
        service
            .get_judgment_summary_for_researcher(operator_id)
            .await
    })
    .await
}
/// Load patient data as summaries - filtered by patient ID list
#[tauri::command]
pub async fn load_all_patient_data(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<PatientSummary>> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service =
            patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
        service.get_all_patients_for_researcher(operator_id).await
    })
    .await
}

/// Load patients by their external IDs - for research session chunks
#[tauri::command]
pub async fn load_patients_by_ids(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_ids: Vec<String>,
) -> Result<Vec<PatientSummary>> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let service =
            patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
        service.get_patients_by_external_ids(&patient_ids).await
    })
    .await
}
/// Search patients with patient ID filtering
#[tauri::command]
pub async fn search_patients(
    state: tauri::State<'_, Mutex<AppState>>,
    query: String,
) -> Result<Vec<PatientSummary>> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service =
            patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
        service
            .search_patients_for_researcher(&query, operator_id)
            .await
    })
    .await
}
/// Get patient statistics with patient ID filtering
#[tauri::command]
pub async fn get_comprehensive_statistics(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<PatientStatistics> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service =
            patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
        service
            .get_patient_statistics_for_researcher(operator_id)
            .await
    })
    .await
}
/// Load app configuration
#[tauri::command]
pub async fn load_app_settings(state: tauri::State<'_, Mutex<AppState>>) -> Result<Config> {
    let app_state = state.lock().await;
    Ok(app_state.config.clone())
}
/// Get feature flags
#[tauri::command]
pub async fn get_feature_flags(state: tauri::State<'_, Mutex<AppState>>) -> Result<Vec<String>> {
    let app_state = state.lock().await;
    Ok(app_state.config.features.enabled_features())
}
/// Check if feature is enabled
#[tauri::command]
pub async fn is_feature_enabled(
    state: tauri::State<'_, Mutex<AppState>>,
    feature_name: String,
) -> Result<bool> {
    let app_state = state.lock().await;
    Ok(config::is_feature_enabled(&app_state.config, &feature_name))
}
/// Get initial data for app bootstrap
#[tauri::command]
pub async fn get_initial_data(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<config::FrontendData> {
    let app_state = state.lock().await;
    Ok(config::get_initial_frontend_data(&app_state.config))
}
/// Get UI theme settings
#[tauri::command]
pub async fn get_ui_theme(_state: tauri::State<'_, Mutex<AppState>>) -> Result<config::UITheme> {
    Ok(config::get_ui_theme())
}
/// Save judgment with chunk detection (research workflow)
#[tauri::command]
pub async fn save_patient_judgment_with_chunk_detection(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
    judgment: String,
) -> Result<bool> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        // Use atomic judgment save and session progress service method
        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service =
            judgment::service::JudgmentService::new(&*db, tenant_id, config::Config::default());
        service
            .save_judgment_and_progress_session(&patient_id, &judgment, Some(operator_id))
            .await
    })
    .await
}
/// Flag patient for admin review
#[tauri::command]
pub async fn flag_for_admin_review(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
    reason: String,
    flag_type: Option<String>,
) -> Result<AdminFlag> {
    let (tenant_id, db) = extract_app_context(&state).await?;
    let operator_id = {
        let app_state = state.lock().await;
        app_state
            .get_current_user_id()
            .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
    };
    let service =
        admin_flag::service::AdminFlagService::new(&*db, tenant_id, config::Config::default());
    let flag_type = flag_type.unwrap_or_else(|| "admin_review".to_string());
    service
        .flag_for_admin_review_with_type(&patient_id, &reason, &flag_type, operator_id)
        .await
}
/// Get admin review status for patient
#[tauri::command]
pub async fn get_admin_review_status(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
) -> Result<Option<AdminFlag>> {
    let (tenant_id, db) = extract_app_context(&state).await?;
    let service =
        admin_flag::service::AdminFlagService::new(&*db, tenant_id, config::Config::default());
    service.get_admin_flag(&patient_id).await
}
/// Clear admin review flag for patient
#[tauri::command]
pub async fn clear_admin_review_flag(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
    resolution_notes: Option<String>,
) -> Result<()> {
    let (tenant_id, db) = extract_app_context(&state).await?;
    let service =
        admin_flag::service::AdminFlagService::new(&*db, tenant_id, config::Config::default());

    let notes = resolution_notes.unwrap_or_else(|| "Flag resolved by user".to_string());
    let operator_id = {
        let app_state = state.lock().await;
        app_state
            .get_current_user_id()
            .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
    };
    service
        .resolve_admin_flag(&patient_id, &notes, operator_id)
        .await
}
/// Get admin flag summary for dashboard
#[tauri::command]
pub async fn get_admin_flag_summary(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<AdminFlagSummary> {
    let (tenant_id, db) = extract_app_context(&state).await?;
    let service =
        admin_flag::service::AdminFlagService::new(&*db, tenant_id, config::Config::default());
    service.get_admin_flag_statistics().await
}
/// Get the next patient to review in the current research session
#[tauri::command]
pub async fn get_next_patient_in_session(
    state: tauri::State<'_, Mutex<AppState>>,
    current_patient_id: String,
) -> Result<Option<String>> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        // Use ResearchSessionService to get the next patient for the selected operator.
        let service = research_session::service::ResearchSessionService::new(
            &*db,
            tenant_id,
            config::Config::default(),
        );
        service
            .get_next_unjudged_patient_id(&current_patient_id, operator_id)
            .await
    })
    .await
}
// Patient selection and session commands.
#[tauri::command]
pub async fn get_patient_selection_info(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<config::PatientSelectionInfo> {
    let (tenant_id, db) = extract_app_context(&state).await?;

    let operator_id = {
        let app_state = state.lock().await;
        app_state
            .get_current_user_id()
            .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
    };

    let service = patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
    service
        .get_patient_selection_info_for_researcher(operator_id)
        .await
}
#[tauri::command]
pub async fn get_patient_groups(
    state: tauri::State<'_, Mutex<AppState>>,
    group_by: Option<String>,
) -> Result<HashMap<String, Vec<String>>> {
    let (tenant_id, db) = extract_app_context(&state).await?;

    let operator_id = {
        let app_state = state.lock().await;
        app_state
            .get_current_user_id()
            .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
    };

    let service = patient::service::PatientService::new(&*db, tenant_id, config::Config::default());
    service
        .get_patient_groups_for_researcher(operator_id, group_by.as_deref().unwrap_or("none"))
        .await
}
#[tauri::command]
pub async fn get_research_session(
    state: tauri::State<'_, Mutex<AppState>>,
    _session_id: Option<String>,
) -> Result<serde_json::Value> {
    let has_authenticated_context = {
        let app_state = state.lock().await;
        app_state.get_current_tenant_id().is_some() && app_state.get_current_user_id().is_some()
    };

    if !has_authenticated_context {
        log::info!("No authenticated session available; returning empty research session");
        return Ok(empty_research_session_payload());
    }

    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };
        let service = research_session::service::ResearchSessionService::new(
            &*db,
            tenant_id,
            config::Config::default(),
        );

        match service.get_session_summary(operator_id).await? {
            Some(summary) => serde_json::to_value(summary).map_err(|error| {
                PlatformError::data_access_with_source(
                    "Failed to serialize research session summary",
                    error,
                )
            }),
            None => Ok(empty_research_session_payload()),
        }
    })
    .await
}

#[tauri::command]
pub async fn get_current_session_state(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<serde_json::Value> {
    let has_authenticated_context = {
        let app_state = state.lock().await;
        app_state.get_current_tenant_id().is_some() && app_state.get_current_user_id().is_some()
    };

    if !has_authenticated_context {
        log::info!("No authenticated session available; returning empty current session state");
        return Ok(empty_current_session_state_payload());
    }

    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        // ARCHITECTURAL COMPLIANCE: Route through research session service
        let service = research_session::service::ResearchSessionService::new(
            &*db,
            tenant_id,
            config::Config::default(),
        );

        // Get current session state (active session for this user)
        match service.get_current_session_state(operator_id).await {
            Ok(Some(session)) => {
                log::info!(
                    "Found active session {} for operator {}",
                    session.id,
                    operator_id
                );
                Ok(serde_json::json!({
                    "session_id": session.id,
                    "session_name": session.session_name,
                    "status": session.status,
                    "active_chunk": {
                        "id": session.current_chunk_number,
                        "patient_ids": session.current_chunk_patients,
                        "total_patients": session.current_chunk_patients.len()
                    },
                    "completed_chunks": session.completed_chunks,
                    "has_active_session": true
                }))
            }
            Ok(None) => {
                log::info!("No active session found for operator {}", operator_id);
                Ok(empty_current_session_state_payload())
            }
            Err(e) => Err(e),
        }
    })
    .await
}

/// Get available research cohorts for the current user
#[tauri::command]
pub async fn get_available_cohorts(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<review_core::cohort::service::AvailableCohort>> {
    let has_authenticated_context = {
        let app_state = state.lock().await;
        app_state.get_current_tenant_id().is_some() && app_state.get_current_user_id().is_some()
    };

    if !has_authenticated_context {
        log::info!("No authenticated session available; returning no available cohorts");
        return Ok(vec![]);
    }

    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        // Get the current selected operator ID
        let user_id = {
            let app_state = state.lock().await;
            app_state.get_current_user_id().ok_or_else(|| {
                platform_errors::PlatformError::invalid_input("No active operator selected")
            })?
        };

        // Get config from app state
        let config = {
            let app_state = state.lock().await;
            app_state.config.clone()
        };

        // Use cohort service to get available cohorts
        let service = review_core::cohort::service::CohortService::new(&*db, tenant_id, config);
        service.get_available_cohorts(user_id).await
    })
    .await
}

/// Start a review session for a specific research cohort
#[tauri::command]
pub async fn start_review_session_for_cohort(
    state: tauri::State<'_, Mutex<AppState>>,
    cohort_id: String,
    session_name: Option<String>,
) -> Result<review_core::cohort::service::CohortSessionResponse> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        // Parse cohort_id as UUID
        let cohort_uuid = Uuid::parse_str(&cohort_id).map_err(|_| {
            platform_errors::PlatformError::invalid_input_field(
                "Invalid cohort ID format",
                "cohort_id",
            )
        })?;

        // Get the current selected operator ID
        let user_id = {
            let app_state = state.lock().await;
            app_state.get_current_user_id().ok_or_else(|| {
                platform_errors::PlatformError::invalid_input("No active operator selected")
            })?
        };

        // Get config from app state
        let config = {
            let app_state = state.lock().await;
            app_state.config.clone()
        };

        // Use cohort service to start review session
        let service = review_core::cohort::service::CohortService::new(&*db, tenant_id, config);
        service
            .start_review_session_for_cohort(cohort_uuid, user_id, session_name)
            .await
    })
    .await
}
/// Get next unjudged patient in current research session
#[tauri::command]
pub async fn get_next_unjudged_patient(
    state: tauri::State<'_, Mutex<AppState>>,
    current_patient_id: String,
) -> Result<serde_json::Value> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service = research_session::service::ResearchSessionService::new(
            &*db,
            tenant_id,
            config::Config::default(),
        );
        let next_patient_id = service
            .get_next_unjudged_patient_id(&current_patient_id, operator_id)
            .await?;

        // Return JSON response matching API contract
        Ok(serde_json::json!({
            "next_patient_id": next_patient_id
        }))
    })
    .await
}

/// Progress to next chunk in research session
#[tauri::command]
pub async fn progress_to_next_chunk(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<serde_json::Value> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state.get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service = research_session::service::ResearchSessionService::new(&*db, tenant_id, config::Config::default());
        let (session, progress_summary) = service.progress_to_next_chunk_with_summary(operator_id).await?;

        // Get first patient ID from new chunk for navigation
        let first_patient_id = session.current_chunk_patients
            .first()
            .cloned()
            .unwrap_or_else(|| "".to_string());

        // Return response matching exact API contract
        Ok(serde_json::json!({
            "new_chunk_number": session.current_chunk_number,
            "first_patient_id": first_patient_id,
            "progress_summary": {
                "judged_in_last_chunk": progress_summary.patients_completed,
                "total_judged_in_session": progress_summary.patients_completed * session.completed_chunks.len().max(1),
                "total_remaining_in_session": progress_summary.total_patients - (progress_summary.patients_completed * session.completed_chunks.len().max(1))
            }
        }))
    }).await
}

/// Get enhanced navigation state for patient details
#[tauri::command]
pub async fn get_navigation_state(
    state: tauri::State<'_, Mutex<AppState>>,
    patient_id: String,
) -> Result<review_core::patient::models::NavigationState> {
    run_command(|| async {
        let (tenant_id, db) = extract_app_context(&state).await?;

        let operator_id = {
            let app_state = state.lock().await;
            app_state
                .get_current_user_id()
                .ok_or_else(|| PlatformError::data_access("No active operator selected"))?
        };

        let service = research_session::service::ResearchSessionService::new(
            &*db,
            tenant_id,
            config::Config::default(),
        );
        service
            .get_enhanced_navigation_state(&patient_id, operator_id)
            .await
    })
    .await
}

/// Return current operator session state for the local workspace UI
#[tauri::command]
pub async fn get_operator_session_state(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<crate::app_state::OperatorSessionState> {
    let app_state = state.lock().await;
    app_state.operator_session_state().await
}

/// List operators available for selection in the local workspace
#[tauri::command]
pub async fn list_local_operators(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<LocalOperatorSummary>> {
    let app_state = state.lock().await;
    app_state.list_local_operators().await
}

/// Create a new local operator and activate it immediately
#[tauri::command]
pub async fn create_local_operator(
    state: tauri::State<'_, Mutex<AppState>>,
    display_name: String,
) -> Result<crate::app_state::OperatorSessionState> {
    let mut app_state = state.lock().await;
    app_state.create_local_operator(&display_name).await
}

/// Activate an existing local operator for the current app runtime
#[tauri::command]
pub async fn select_local_operator(
    state: tauri::State<'_, Mutex<AppState>>,
    operator_id: String,
) -> Result<crate::app_state::OperatorSessionState> {
    let operator_id = Uuid::parse_str(&operator_id).map_err(|_| {
        PlatformError::invalid_input_field("Invalid operator ID format", "operator_id")
    })?;

    let mut app_state = state.lock().await;
    app_state.activate_operator(operator_id).await?;
    app_state.operator_session_state().await
}

/// Clear the current local operator session
#[tauri::command]
pub async fn clear_operator_session(state: tauri::State<'_, Mutex<AppState>>) -> Result<()> {
    let mut app_state = state.lock().await;
    app_state.clear_operator_session().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_research_session_payload_is_safe_default() {
        let payload = empty_research_session_payload();

        assert!(payload["active_chunk"].is_null());
        assert_eq!(payload["completed_chunks"], serde_json::json!([]));
        assert_eq!(payload["total_patients"], 0);
        assert_eq!(payload["judged_patients"], 0);
    }

    #[test]
    fn empty_current_session_state_payload_has_no_active_session() {
        let payload = empty_current_session_state_payload();

        assert!(payload["session_id"].is_null());
        assert_eq!(payload["has_active_session"], false);
    }
}
