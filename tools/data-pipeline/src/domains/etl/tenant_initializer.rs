use platform_db::{DatabaseConnection, DatabaseConnectionType};
use platform_errors::{PlatformError, Result};
use platform_models::{Tenant, User};
use std::env;
use uuid::Uuid;

/// Initializes tenant and user context for ETL operations
pub struct TenantInitializer;

impl TenantInitializer {
    pub const DEFAULT_LOCAL_OPERATOR_IDENTIFIER: &str = "example-reviewer";
    pub const DEFAULT_LOCAL_OPERATOR_NAME: &str = "Example Reviewer";

    /// Create or retrieve default tenant for ETL operations
    pub async fn ensure_default_tenant(
        db: &DatabaseConnectionType,
        tenant_name: Option<String>,
    ) -> Result<Tenant> {
        let tenant_name = tenant_name.unwrap_or_else(|| "Example Research Workspace".to_string());
        let slug = Self::configured_tenant_slug(&tenant_name);

        // Check if tenant already exists
        match db.get_tenant_by_slug(&slug).await {
            Ok(existing_tenant) => {
                println!(
                    "ℹ️  Using existing tenant: {} ({})",
                    existing_tenant.name, existing_tenant.slug
                );
                return Ok(existing_tenant);
            }
            Err(PlatformError::NotFound { .. }) => {
                // Tenant doesn't exist, continue to create it
            }
            Err(e) => return Err(e),
        }

        // Create new tenant using platform-db
        let tenant = db.create_tenant(&tenant_name, &slug).await?;

        println!("✅ Created new tenant: {} ({})", tenant.name, tenant.slug);
        Ok(tenant)
    }

    /// Retrieve the pre-defined local workspace operator
    pub async fn ensure_local_operator(
        db: &DatabaseConnectionType,
        tenant: &Tenant,
    ) -> Result<User> {
        let operator_identifier = Self::configured_local_operator_identifier();
        let operator_name = Self::configured_local_operator_name();
        let operator_email = Self::configured_local_operator_email(&operator_identifier);

        let existing_operator =
            db.list_local_operators(tenant.id)
                .await?
                .into_iter()
                .find(|operator| {
                    operator.local_identifier.as_deref() == Some(operator_identifier.as_str())
                });

        let operator = match existing_operator {
            Some(operator) => operator,
            None => {
                println!(
                    "ℹ️  Creating ETL local operator '{}' for tenant {}",
                    operator_identifier, tenant.slug
                );
                db.create_local_operator(
                    tenant.id,
                    &operator_name,
                    Some(&operator_identifier),
                    Some(&operator_email),
                    "reviewer",
                )
                .await?
            }
        };

        let user = db.get_user_by_id(operator.id).await?;
        println!("✅ Local operator verified: {}", user.id);
        Ok(user)
    }

    /// Initialize complete tenant context (tenant + local operator)
    pub async fn initialize_tenant_context(
        db: &DatabaseConnectionType,
        tenant_name: Option<String>,
    ) -> Result<(Tenant, User)> {
        println!("🏗️  Initializing tenant context...");

        let tenant = Self::ensure_default_tenant(db, tenant_name).await?;
        let user = Self::ensure_local_operator(db, &tenant).await?;

        println!("✅ Tenant context initialized:");
        println!("   Tenant: {} ({})", tenant.name, tenant.id);
        println!("   Local Operator ID: {}", user.id);

        Ok((tenant, user))
    }

    /// Generate URL-safe slug from tenant name
    fn generate_slug(name: &str) -> String {
        name.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>()
            .join("-")
    }

    fn configured_tenant_slug(tenant_name: &str) -> String {
        env::var("PLATFORM_TENANT_SLUG")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Self::generate_slug(tenant_name))
    }

    fn configured_local_operator_identifier() -> String {
        env::var("PLATFORM_OPERATOR_IDENTIFIER")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_LOCAL_OPERATOR_IDENTIFIER.to_string())
    }

    fn configured_local_operator_name() -> String {
        env::var("PLATFORM_OPERATOR_NAME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_LOCAL_OPERATOR_NAME.to_string())
    }

    fn configured_local_operator_email(operator_identifier: &str) -> String {
        env::var("PLATFORM_OPERATOR_EMAIL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{operator_identifier}@example.invalid"))
    }

    /// Validate tenant context for ETL operations
    pub fn validate_tenant_context(tenant: &Tenant, user: &User) -> Result<()> {
        if tenant.id == Uuid::nil() {
            return Err(PlatformError::invalid_input_field(
                "Invalid tenant ID - cannot be nil UUID",
                "tenant_id",
            ));
        }

        if user.id == Uuid::nil() {
            return Err(PlatformError::invalid_input_field(
                "Invalid user ID - cannot be nil UUID",
                "user_id",
            ));
        }

        if tenant.name.is_empty() || tenant.slug.is_empty() {
            return Err(PlatformError::invalid_input(
                "Tenant name and slug cannot be empty",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_generate_slug() {
        assert_eq!(
            TenantInitializer::generate_slug("Example Research Workspace"),
            "example-research-workspace"
        );

        assert_eq!(
            TenantInitializer::generate_slug("Workspace@2024_Test!"),
            "workspace-2024-test"
        );

        assert_eq!(
            TenantInitializer::generate_slug("  Multiple   Spaces  "),
            "multiple-spaces"
        );
    }

    #[test]
    fn test_validate_tenant_context() {
        let tenant = Tenant {
            id: Uuid::new_v4(),
            name: "Test Tenant".to_string(),
            slug: "test-tenant".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let user = User {
            id: Uuid::new_v4(),
            first_name: None,
            last_name: None,
            display_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(TenantInitializer::validate_tenant_context(&tenant, &user).is_ok());

        // Test with nil UUID
        let invalid_tenant = Tenant {
            id: Uuid::nil(),
            name: tenant.name.clone(),
            slug: tenant.slug.clone(),
            created_at: tenant.created_at,
            updated_at: tenant.updated_at,
        };

        assert!(TenantInitializer::validate_tenant_context(&invalid_tenant, &user).is_err());
    }
}
