use platform_db::{DatabaseConnection, DatabaseConnectionType};
use platform_errors::{PlatformError, Result};
use platform_models::{LocalOperator, Tenant, User};
use review_core::Config;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalOperatorSummary {
    pub id: Uuid,
    pub display_name: String,
    pub email: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OperatorSessionState {
    pub workspace_name: String,
    pub workspace_slug: String,
    pub database_backend: String,
    pub operator: Option<LocalOperatorSummary>,
    pub requires_operator_selection: bool,
}

#[derive(Debug, Clone)]
pub struct OperatorSession {
    pub user: User,
    pub operator: LocalOperatorSummary,
    pub tenant: Tenant,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Arc<DatabaseConnectionType>,
    pub workspace_tenant: Tenant,
    pub current_session: Option<OperatorSession>,
}

impl AppState {
    pub async fn new_with_database_url(config: Config, database_url: &str) -> Result<Self> {
        let db = Arc::new(DatabaseConnectionType::new(database_url).await?);
        db.run_migrations().await?;
        let workspace_tenant = resolve_workspace_tenant(&*db).await?;

        Ok(Self {
            config,
            db,
            workspace_tenant,
            current_session: None,
        })
    }

    pub async fn list_local_operators(&self) -> Result<Vec<LocalOperatorSummary>> {
        self.db
            .list_local_operators(self.workspace_tenant.id)
            .await
            .map(|operators| {
                operators
                    .into_iter()
                    .map(LocalOperatorSummary::from)
                    .collect()
            })
    }

    pub async fn create_local_operator(
        &mut self,
        display_name: &str,
    ) -> Result<OperatorSessionState> {
        let operator = self
            .db
            .create_local_operator(
                self.workspace_tenant.id,
                display_name,
                None,
                None,
                "reviewer",
            )
            .await
            .map(LocalOperatorSummary::from)?;

        self.activate_operator(operator.id).await?;
        self.operator_session_state().await
    }

    pub async fn activate_operator(&mut self, operator_id: Uuid) -> Result<User> {
        let operator = self
            .list_local_operators()
            .await?
            .into_iter()
            .find(|entry| entry.id == operator_id)
            .ok_or_else(|| PlatformError::not_found("local_operator", operator_id.to_string()))?;
        let user = self.db.get_user_by_id(operator_id).await?;

        let roles = self
            .db
            .get_user_roles_for_tenant(user.id, self.workspace_tenant.id)
            .await?;
        if roles.is_empty() {
            return Err(PlatformError::invalid_input(
                "Selected operator has no access to the local workspace",
            ));
        }

        if self
            .db
            .get_active_local_work_session(self.workspace_tenant.id, operator_id)
            .await?
            .is_none()
        {
            self.db
                .start_local_work_session(self.workspace_tenant.id, operator_id, None)
                .await?;
        }

        self.current_session = Some(OperatorSession {
            user: user.clone(),
            operator,
            tenant: self.workspace_tenant.clone(),
        });

        Ok(user)
    }

    pub async fn clear_operator_session(&mut self) -> Result<()> {
        if let Some(session) = &self.current_session {
            if let Some(work_session) = self
                .db
                .get_active_local_work_session(session.tenant.id, session.user.id)
                .await?
            {
                self.db
                    .end_local_work_session(session.tenant.id, work_session.id)
                    .await?;
            }
        }
        self.current_session = None;
        Ok(())
    }

    pub fn has_active_operator_session(&self) -> bool {
        self.current_session.is_some()
    }

    pub fn get_current_tenant_id(&self) -> Option<Uuid> {
        self.current_session
            .as_ref()
            .map(|session| session.tenant.id)
    }

    pub fn get_current_user_id(&self) -> Option<Uuid> {
        self.current_session.as_ref().map(|session| session.user.id)
    }

    pub fn get_current_operator(&self) -> Option<LocalOperatorSummary> {
        self.current_session
            .as_ref()
            .map(|session| session.operator.clone())
    }

    pub async fn operator_session_state(&self) -> Result<OperatorSessionState> {
        Ok(OperatorSessionState {
            workspace_name: self.workspace_tenant.name.clone(),
            workspace_slug: self.workspace_tenant.slug.clone(),
            database_backend: current_database_backend(),
            operator: self.get_current_operator(),
            requires_operator_selection: !self.has_active_operator_session(),
        })
    }
}

async fn resolve_workspace_tenant(db: &dyn DatabaseConnection) -> Result<Tenant> {
    let explicit_slug = std::env::var("REVIEW_APP_LOCAL_TENANT_SLUG")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(slug) = explicit_slug {
        match db.get_tenant_by_slug(&slug).await {
            Ok(tenant) => return Ok(tenant),
            Err(PlatformError::NotFound { .. }) => {
                let tenant_name = std::env::var("REVIEW_APP_LOCAL_TENANT_NAME")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Example Research Workspace".to_string());
                return db.create_tenant(&tenant_name, &slug).await;
            }
            Err(error) => return Err(error),
        }
    }

    let existing_tenants = db.list_tenants().await?;
    match existing_tenants.len() {
        0 => {
            db.create_tenant("Example Research Workspace", "example-research-workspace")
                .await
        }
        1 => Ok(existing_tenants.into_iter().next().expect("length checked")),
        count => Err(PlatformError::config_key(
            format!(
                "The local database contains {count} workspaces; choose one explicitly with REVIEW_APP_LOCAL_TENANT_SLUG"
            ),
            "REVIEW_APP_LOCAL_TENANT_SLUG",
        )),
    }
}

fn current_database_backend() -> String {
    "SQLite".to_string()
}

impl From<LocalOperator> for LocalOperatorSummary {
    fn from(value: LocalOperator) -> Self {
        Self {
            id: value.id,
            display_name: value.display_name.clone(),
            email: value
                .email
                .or(value
                    .local_identifier
                    .map(|identifier| format!("{identifier}@example.invalid")))
                .unwrap_or(value.display_name),
        }
    }
}
