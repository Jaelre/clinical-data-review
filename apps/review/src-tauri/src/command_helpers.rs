use crate::app_state::AppState;
use platform_errors::{PlatformError, Result};
use std::future::Future;
use tokio::sync::Mutex;

pub async fn extract_app_context(
    state: &tauri::State<'_, Mutex<AppState>>,
) -> Result<(
    uuid::Uuid,
    std::sync::Arc<dyn platform_db::DatabaseConnection>,
)> {
    let app_state = state.lock().await;
    let tenant_id = app_state.get_current_tenant_id().ok_or_else(|| {
        PlatformError::data_access("No active operator selected for the local workspace")
    })?;
    let db: std::sync::Arc<dyn platform_db::DatabaseConnection> = app_state.db.clone();
    Ok((tenant_id, db))
}

pub async fn run_command<F, T, Fut>(operation: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    operation().await
}
