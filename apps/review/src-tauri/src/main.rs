// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Core transport layer for the local-only review client.
mod app_state;
mod command_helpers;
mod commands;
mod runtime_paths;

use app_state::AppState;
use commands::*;
use log::{error, info};
use std::process::Command;
use tokio::sync::Mutex;

fn main() {
    env_logger::init();

    info!("Starting Clinical Data Review System in local workspace mode");

    let app_state = match initialize_app_state() {
        Ok(state) => state,
        Err(error) => {
            report_startup_failure(&*error);
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_patient_details,
            save_patient_judgment,
            get_patient_judgment,
            get_judgment_summary,
            load_all_patient_data,
            load_patients_by_ids,
            search_patients,
            get_comprehensive_statistics,
            load_app_settings,
            get_feature_flags,
            is_feature_enabled,
            save_patient_judgment_with_chunk_detection,
            get_available_cohorts,
            start_review_session_for_cohort,
            get_initial_data,
            get_ui_theme,
            flag_for_admin_review,
            get_admin_review_status,
            clear_admin_review_flag,
            get_admin_flag_summary,
            get_next_patient_in_session,
            get_next_unjudged_patient,
            progress_to_next_chunk,
            get_navigation_state,
            get_patient_selection_info,
            get_patient_groups,
            get_research_session,
            get_current_session_state,
            get_operator_session_state,
            list_local_operators,
            create_local_operator,
            select_local_operator,
            clear_operator_session
        ])
        .manage(Mutex::new(app_state))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn initialize_app_state() -> Result<AppState, Box<dyn std::error::Error>> {
    tauri::async_runtime::block_on(async {
        let resolved_database = runtime_paths::resolve_database()?;
        let config = review_core::config::load_config().await?;
        info!(
            "Configuration loaded successfully for environment: {}",
            config.environment
        );

        log_database_runtime(&config, &resolved_database);

        let app_state = AppState::new_with_database_url(config, &resolved_database.database_url)
            .await
            .map_err(|error| format!("Database initialization failed: {error}"))?;

        info!("Local workspace database connection established");
        info!("Application state initialized");

        Ok::<AppState, Box<dyn std::error::Error>>(app_state)
    })
}

fn log_database_runtime(
    config: &review_core::Config,
    resolved_database: &runtime_paths::ResolvedDatabase,
) {
    if config.should_log_runtime_details() {
        info!(
            "Using database source `{}` at {}",
            resolved_database.source,
            resolved_database
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| resolved_database.database_url.clone())
        );
    } else {
        info!(
            "Using database source `{}` with production-safe runtime logging",
            resolved_database.source
        );
    }
}

fn report_startup_failure(error: &dyn std::error::Error) {
    let message = format!("Application startup failed: {error}");
    error!("{message}");
    eprintln!("{message}");

    #[cfg(target_os = "macos")]
    show_macos_startup_alert(&message);
}

#[cfg(target_os = "macos")]
fn show_macos_startup_alert(message: &str) {
    let escaped_message = message.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display alert \"Clinical Data Review failed to start\" message \"{}\" as critical",
        escaped_message
    );

    if let Err(alert_error) = Command::new("osascript").arg("-e").arg(script).status() {
        eprintln!("Failed to show macOS startup alert: {alert_error}");
    }
}
