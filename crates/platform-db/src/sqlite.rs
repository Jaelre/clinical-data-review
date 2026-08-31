//! SQLite database implementation.
//!
//! This is the only persistence backend for the workspace. It exposes local
//! operator and work-session primitives for the desktop application.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use platform_errors::{PlatformError, Result};
use platform_models::*;
use serde::de::DeserializeOwned;
use serde_json::json;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::connection::DatabaseConnection;
use crate::query_options::*;

const SQLITE_SCHEMA: &str = include_str!("sqlite_schema.sql");
const SQLITE_SYNTHETIC_SEED: &str = include_str!("sqlite_synthetic_seed.sql");

pub struct SqliteConnection {
    pool: SqlitePool,
}

impl SqliteConnection {
    pub async fn new(connection_string: &str) -> Result<Self> {
        let normalized = normalize_sqlite_connection_string(connection_string)?;
        ensure_sqlite_parent_directory(&normalized)?;
        let options = SqliteConnectOptions::from_str(&normalized)
            .map_err(|e| PlatformError::config(format!("Invalid SQLite connection string: {e}")))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(30))
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options)
            .await
            .map_err(|e| {
                PlatformError::data_access_with_source("Failed to connect to SQLite", e)
            })?;

        Ok(Self { pool })
    }

    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn database_type(&self) -> &'static str {
        "SQLite"
    }

    pub async fn seed_synthetic_fixture_data(&self) -> Result<()> {
        sqlx::raw_sql(SQLITE_SYNTHETIC_SEED)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                PlatformError::migration(format!("SQLite synthetic fixture seeding failed: {e}"))
            })?;
        Ok(())
    }

    async fn fetch_local_operator(&self, tenant_id: Uuid, user_id: Uuid) -> Result<LocalOperator> {
        sqlx::query_as::<_, LocalOperator>(
            r#"
            SELECT
                u.id,
                utr.tenant_id,
                COALESCE(u.display_name, u.local_identifier, u.email, hex(u.id)) AS display_name,
                u.local_identifier,
                u.email,
                utr.role,
                u.created_at,
                u.updated_at
            FROM users u
            JOIN user_tenant_roles utr ON utr.user_id = u.id
            WHERE u.id = ?1 AND utr.tenant_id = ?2
            ORDER BY CASE utr.role
                WHEN 'admin' THEN 0
                WHEN 'reviewer' THEN 1
                ELSE 2
            END
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("local_operator", user_id.to_string()))
    }

    async fn fetch_local_work_session(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
    ) -> Result<LocalWorkSession> {
        sqlx::query_as::<_, LocalWorkSession>(
            r#"
            SELECT id, tenant_id, operator_id, session_label, status, started_at, last_activity_at, ended_at
            FROM local_work_sessions
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("local_work_session", session_id.to_string()))
    }

    async fn fetch_research_session_by_id(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
    ) -> Result<ResearchSession> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, session_name, primary_researcher_id, status, total_patients,
                   current_chunk_number, current_chunk_patients, completed_chunks, cohort_id
            FROM research_sessions
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("research_session", session_id.to_string()))?;

        map_research_session(&row)
    }
}

#[async_trait]
impl DatabaseConnection for SqliteConnection {
    async fn run_migrations(&self) -> Result<()> {
        sqlx::raw_sql(SQLITE_SCHEMA)
            .execute(&self.pool)
            .await
            .map_err(|e| PlatformError::migration(format!("SQLite migration failed: {e}")))?;
        Ok(())
    }

    async fn create_tenant(&self, name: &str, slug: &str) -> Result<Tenant> {
        let id = Uuid::new_v4();
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            INSERT INTO tenants (id, name, slug, created_at, updated_at)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            RETURNING id, name, slug, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .fetch_one(&self.pool)
        .await?;

        Ok(tenant)
    }

    async fn get_tenant_by_slug(&self, slug: &str) -> Result<Tenant> {
        sqlx::query_as::<_, Tenant>(
            "SELECT id, name, slug, created_at, updated_at FROM tenants WHERE slug = ?1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("tenant", slug))
    }

    async fn get_tenant_by_id(&self, id: Uuid) -> Result<Tenant> {
        sqlx::query_as::<_, Tenant>(
            "SELECT id, name, slug, created_at, updated_at FROM tenants WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("tenant", id.to_string()))
    }

    async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        Ok(sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, slug, created_at, updated_at
            FROM tenants
            WHERE is_active = 1
            ORDER BY created_at ASC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn get_user_by_email(&self, email: &str) -> Result<User> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT id, first_name, last_name, display_name, created_at, updated_at
            FROM users
            WHERE email = ?1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("user", email))
    }

    async fn get_user_by_id(&self, id: Uuid) -> Result<User> {
        sqlx::query_as::<_, User>(
            "SELECT id, first_name, last_name, display_name, created_at, updated_at FROM users WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("user", id.to_string()))
    }

    async fn create_user_tenant_role(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        role: &str,
    ) -> Result<UserTenantRole> {
        let id = Uuid::new_v4();
        let role_assignment = sqlx::query_as::<_, UserTenantRole>(
            r#"
            INSERT INTO user_tenant_roles (id, user_id, tenant_id, role, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(user_id, tenant_id, role)
            DO UPDATE SET updated_at = CURRENT_TIMESTAMP
            RETURNING id, user_id, tenant_id, role, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(role)
        .fetch_one(&self.pool)
        .await?;

        Ok(role_assignment)
    }

    async fn get_user_roles_for_tenant(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<UserTenantRole>> {
        let roles = sqlx::query_as::<_, UserTenantRole>(
            r#"
            SELECT id, user_id, tenant_id, role, created_at, updated_at
            FROM user_tenant_roles
            WHERE user_id = ?1 AND tenant_id = ?2
            ORDER BY role ASC
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(roles)
    }

    async fn list_local_operators(&self, tenant_id: Uuid) -> Result<Vec<LocalOperator>> {
        let operators = sqlx::query_as::<_, LocalOperator>(
            r#"
            SELECT
                u.id,
                utr.tenant_id,
                COALESCE(u.display_name, u.local_identifier, u.email, hex(u.id)) AS display_name,
                u.local_identifier,
                u.email,
                utr.role,
                u.created_at,
                u.updated_at
            FROM users u
            JOIN user_tenant_roles utr ON utr.user_id = u.id
            WHERE utr.tenant_id = ?1
            ORDER BY LOWER(COALESCE(u.display_name, u.local_identifier, u.email, hex(u.id))) ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(operators)
    }

    async fn create_local_operator(
        &self,
        tenant_id: Uuid,
        display_name: &str,
        local_identifier: Option<&str>,
        email: Option<&str>,
        role: &str,
    ) -> Result<LocalOperator> {
        let trimmed_name = display_name.trim();
        if trimmed_name.is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Display name cannot be empty",
                "display_name",
            ));
        }

        let user_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let derived_identifier = local_identifier
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| Some(slugify(trimmed_name)));

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO users (
                id,
                email,
                local_identifier,
                display_name,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(user_id)
        .bind(email.map(str::trim))
        .bind(derived_identifier.as_deref())
        .bind(trimmed_name)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO user_tenant_roles (id, user_id, tenant_id, role, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(role_id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(role)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO research_cohort_reviewers (
                cohort_id,
                user_id,
                role,
                can_review,
                can_export,
                can_modify_cohort,
                granted_at,
                granted_by,
                access_metadata
            )
            SELECT
                rc.id,
                ?1,
                'reviewer',
                1,
                0,
                0,
                CURRENT_TIMESTAMP,
                ?1,
                ?2
            FROM research_cohorts rc
            WHERE rc.tenant_id = ?3
            ON CONFLICT(cohort_id, user_id)
            DO UPDATE SET
                role = excluded.role,
                can_review = excluded.can_review,
                granted_at = CURRENT_TIMESTAMP,
                granted_by = excluded.granted_by,
                access_metadata = excluded.access_metadata
            "#,
        )
        .bind(user_id)
        .bind(json!({"source": "create_local_operator"}).to_string())
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        self.fetch_local_operator(tenant_id, user_id).await
    }

    async fn start_local_work_session(
        &self,
        tenant_id: Uuid,
        operator_id: Uuid,
        session_label: Option<&str>,
    ) -> Result<LocalWorkSession> {
        let session_id = Uuid::new_v4();
        let session_label = session_label
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Local review session");

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            UPDATE local_work_sessions
            SET status = 'paused',
                ended_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP,
                last_activity_at = CURRENT_TIMESTAMP
            WHERE tenant_id = ?1 AND operator_id = ?2 AND status = 'active'
            "#,
        )
        .bind(tenant_id)
        .bind(operator_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO local_work_sessions (
                id,
                tenant_id,
                operator_id,
                session_label,
                status,
                started_at,
                last_activity_at,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, 'active', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(operator_id)
        .bind(session_label)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        self.fetch_local_work_session(tenant_id, session_id).await
    }

    async fn get_active_local_work_session(
        &self,
        tenant_id: Uuid,
        operator_id: Uuid,
    ) -> Result<Option<LocalWorkSession>> {
        let session = sqlx::query_as::<_, LocalWorkSession>(
            r#"
            SELECT id, tenant_id, operator_id, session_label, status, started_at, last_activity_at, ended_at
            FROM local_work_sessions
            WHERE tenant_id = ?1 AND operator_id = ?2 AND status = 'active'
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(operator_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    async fn end_local_work_session(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
    ) -> Result<LocalWorkSession> {
        sqlx::query(
            r#"
            UPDATE local_work_sessions
            SET status = 'completed',
                ended_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP,
                last_activity_at = CURRENT_TIMESTAMP
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        self.fetch_local_work_session(tenant_id, session_id).await
    }

    async fn create_patient(
        &self,
        external_id: &str,
        age: Option<i32>,
        sex: Option<&str>,
        tenant_id: Uuid,
    ) -> Result<Patient> {
        let patient_id = Uuid::new_v4();
        let patient = sqlx::query_as::<_, Patient>(
            r#"
            INSERT INTO patients (id, external_id, age, sex, tenant_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(external_id, tenant_id)
            DO UPDATE SET age = excluded.age, sex = excluded.sex, updated_at = CURRENT_TIMESTAMP
            RETURNING id, external_id, age, sex, tenant_id, created_at, updated_at
            "#,
        )
        .bind(patient_id)
        .bind(external_id)
        .bind(age)
        .bind(sex)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(patient)
    }

    async fn get_patient_by_external_id(
        &self,
        external_id: &str,
        tenant_id: Uuid,
    ) -> Result<Patient> {
        sqlx::query_as::<_, Patient>(
            r#"
            SELECT id, external_id, age, sex, tenant_id, created_at, updated_at
            FROM patients
            WHERE external_id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(external_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("patient", external_id))
    }

    async fn get_patient_by_id(&self, id: Uuid) -> Result<Patient> {
        sqlx::query_as::<_, Patient>(
            "SELECT id, external_id, age, sex, tenant_id, created_at, updated_at FROM patients WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PlatformError::not_found("patient", id.to_string()))
    }

    async fn batch_upsert_patients(
        &self,
        patients: &[(String, Option<i32>, Option<String>)],
        tenant_id: Uuid,
    ) -> Result<HashMap<String, Uuid>> {
        if patients.is_empty() {
            return Ok(HashMap::new());
        }

        let mut tx = self.pool.begin().await?;
        let mut result = HashMap::with_capacity(patients.len());
        for (external_id, age, sex) in patients {
            let id = Uuid::new_v4();
            let row = sqlx::query(
                r#"
                INSERT INTO patients (id, external_id, age, sex, tenant_id, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                ON CONFLICT(external_id, tenant_id)
                DO UPDATE SET age = excluded.age, sex = excluded.sex, updated_at = CURRENT_TIMESTAMP
                RETURNING id, external_id
                "#,
            )
            .bind(id)
            .bind(external_id)
            .bind(age)
            .bind(sex.as_deref())
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await?;

            result.insert(row.get("external_id"), row.get("id"));
        }
        tx.commit().await?;

        Ok(result)
    }

    async fn create_patient_note(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        category: &str,
        content: &str,
    ) -> Result<PatientNote> {
        let note = sqlx::query_as::<_, PatientNote>(
            r#"
            INSERT INTO patient_notes (id, patient_id, tenant_id, category, content, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            RETURNING id, patient_id, tenant_id, category, content, created_at, updated_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(patient_id)
        .bind(tenant_id)
        .bind(category)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;

        Ok(note)
    }

    async fn get_patient_notes(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PatientNote>> {
        let notes = sqlx::query_as::<_, PatientNote>(
            r#"
            SELECT id, patient_id, tenant_id, category, content, created_at, updated_at
            FROM patient_notes
            WHERE patient_id = ?1 AND tenant_id = ?2
            ORDER BY created_at ASC
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(notes)
    }

    async fn get_patient_notes_by_category(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        category: &str,
    ) -> Result<Vec<PatientNote>> {
        let notes = sqlx::query_as::<_, PatientNote>(
            r#"
            SELECT id, patient_id, tenant_id, category, content, created_at, updated_at
            FROM patient_notes
            WHERE patient_id = ?1 AND tenant_id = ?2 AND category = ?3
            ORDER BY created_at ASC
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .bind(category)
        .fetch_all(&self.pool)
        .await?;

        Ok(notes)
    }

    async fn upsert_patient_note(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        category: &str,
        content: &str,
    ) -> Result<PatientNote> {
        let note = sqlx::query_as::<_, PatientNote>(
            r#"
            INSERT INTO patient_notes (id, patient_id, tenant_id, category, content, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(patient_id, category)
            DO UPDATE SET content = excluded.content, updated_at = CURRENT_TIMESTAMP
            RETURNING id, patient_id, tenant_id, category, content, created_at, updated_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(patient_id)
        .bind(tenant_id)
        .bind(category)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;

        Ok(note)
    }

    async fn create_clinical_journal_entry(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        entry_timestamp: DateTime<Utc>,
        entry_sequence: i32,
        role: Option<&str>,
        content: &str,
    ) -> Result<ClinicalJournalEntry> {
        let entry = sqlx::query_as::<_, ClinicalJournalEntry>(
            r#"
            INSERT INTO clinical_journal (
                id, patient_id, tenant_id, entry_timestamp, entry_sequence, role, content, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(patient_id, entry_sequence)
            DO UPDATE SET role = excluded.role, content = excluded.content, updated_at = CURRENT_TIMESTAMP
            RETURNING id, patient_id, tenant_id, entry_timestamp, entry_sequence, role, content, created_at, updated_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(patient_id)
        .bind(tenant_id)
        .bind(entry_timestamp)
        .bind(entry_sequence)
        .bind(role)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;

        Ok(entry)
    }

    async fn get_clinical_journal_entries(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ClinicalJournalEntry>> {
        let entries = sqlx::query_as::<_, ClinicalJournalEntry>(
            r#"
            SELECT id, patient_id, tenant_id, entry_timestamp, entry_sequence, role, content, created_at, updated_at
            FROM clinical_journal
            WHERE patient_id = ?1 AND tenant_id = ?2
            ORDER BY entry_timestamp ASC, entry_sequence ASC
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    async fn get_clinical_journal_entries_in_range(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<ClinicalJournalEntry>> {
        let entries = sqlx::query_as::<_, ClinicalJournalEntry>(
            r#"
            SELECT id, patient_id, tenant_id, entry_timestamp, entry_sequence, role, content, created_at, updated_at
            FROM clinical_journal
            WHERE patient_id = ?1 AND tenant_id = ?2 AND entry_timestamp >= ?3 AND entry_timestamp <= ?4
            ORDER BY entry_timestamp ASC, entry_sequence ASC
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    async fn health_check(&self) -> Result<bool> {
        let result: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&self.pool).await?;
        Ok(result.0 == 1)
    }

    async fn get_version(&self) -> Result<String> {
        let version: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await?;
        Ok(format!("SQLite {version}"))
    }

    async fn get_table_counts(&self) -> Result<HashMap<String, i64>> {
        let tables = [
            "tenants",
            "users",
            "user_tenant_roles",
            "patients",
            "patient_notes",
            "clinical_journal",
            "judgments",
            "research_cohorts",
        ];

        let mut counts = HashMap::new();
        for table in tables {
            let query = format!("SELECT COUNT(*) FROM {table}");
            let count: i64 = sqlx::query_scalar(&query).fetch_one(&self.pool).await?;
            counts.insert(table.to_string(), count);
        }

        Ok(counts)
    }

    async fn get_patients(
        &self,
        tenant_id: Uuid,
        filters: &PatientFilterOptions,
        sorting: &PatientSortOptions,
        pagination: &PaginationOptions,
    ) -> Result<Vec<PatientSummary>> {
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT
                p.id,
                p.external_id,
                p.age,
                p.sex,
                COALESCE(p.review_status, 'pending') AS review_status,
                COALESCE(p.priority_level, 1) AS priority_level,
                CASE WHEN j.patient_id IS NOT NULL THEN 1 ELSE 0 END AS has_judgment,
                CASE WHEN af.patient_id IS NOT NULL THEN 1 ELSE 0 END AS is_flagged,
                p.created_at
            FROM patients p
            LEFT JOIN judgments j ON p.id = j.patient_id AND j.tenant_id = p.tenant_id
            LEFT JOIN admin_flags af ON p.id = af.patient_id AND af.tenant_id = p.tenant_id AND af.status = 'active'
            WHERE p.tenant_id = "#,
        );
        query.push_bind(tenant_id);

        if let Some(status) = &filters.review_status {
            query.push(" AND COALESCE(p.review_status, 'pending') = ");
            query.push_bind(status);
        }

        if let Some(search) = &filters.search_query {
            query.push(" AND LOWER(p.external_id) LIKE LOWER(");
            query.push_bind(format!("%{search}%"));
            query.push(")");
        }

        if let Some(has_judgment) = filters.has_judgment {
            if has_judgment {
                query.push(" AND j.patient_id IS NOT NULL");
            } else {
                query.push(" AND j.patient_id IS NULL");
            }
        }

        if let Some(is_flagged) = filters.is_flagged {
            if is_flagged {
                query.push(" AND af.patient_id IS NOT NULL");
            } else {
                query.push(" AND af.patient_id IS NULL");
            }
        }

        query.push(" ORDER BY p.");
        query.push(sorting.to_sql());
        query.push(" LIMIT ");
        query.push_bind(pagination.limit() as i64);
        query.push(" OFFSET ");
        query.push_bind(pagination.offset() as i64);

        Ok(query
            .build_query_as::<PatientSummary>()
            .fetch_all(&self.pool)
            .await?)
    }

    async fn get_patients_count(
        &self,
        tenant_id: Uuid,
        filters: &PatientFilterOptions,
    ) -> Result<i64> {
        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT COUNT(DISTINCT p.id)
            FROM patients p
            LEFT JOIN judgments j ON p.id = j.patient_id AND j.tenant_id = p.tenant_id
            LEFT JOIN admin_flags af ON p.id = af.patient_id AND af.tenant_id = p.tenant_id AND af.status = 'active'
            WHERE p.tenant_id = "#,
        );
        query.push_bind(tenant_id);

        if let Some(status) = &filters.review_status {
            query.push(" AND COALESCE(p.review_status, 'pending') = ");
            query.push_bind(status);
        }

        if let Some(search) = &filters.search_query {
            query.push(" AND LOWER(p.external_id) LIKE LOWER(");
            query.push_bind(format!("%{search}%"));
            query.push(")");
        }

        if let Some(has_judgment) = filters.has_judgment {
            query.push(if has_judgment {
                " AND j.patient_id IS NOT NULL"
            } else {
                " AND j.patient_id IS NULL"
            });
        }

        if let Some(is_flagged) = filters.is_flagged {
            query.push(if is_flagged {
                " AND af.patient_id IS NOT NULL"
            } else {
                " AND af.patient_id IS NULL"
            });
        }

        Ok(query
            .build_query_scalar::<i64>()
            .fetch_one(&self.pool)
            .await?)
    }

    async fn get_patients_by_external_ids(
        &self,
        external_ids: &[String],
        tenant_id: Uuid,
    ) -> Result<Vec<PatientSummary>> {
        if external_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT
                p.id,
                p.external_id,
                p.age,
                p.sex,
                COALESCE(p.review_status, 'pending') AS review_status,
                COALESCE(p.priority_level, 1) AS priority_level,
                CASE WHEN j.patient_id IS NOT NULL THEN 1 ELSE 0 END AS has_judgment,
                CASE WHEN af.patient_id IS NOT NULL THEN 1 ELSE 0 END AS is_flagged,
                p.created_at
            FROM patients p
            LEFT JOIN judgments j ON p.id = j.patient_id AND j.tenant_id = p.tenant_id
            LEFT JOIN admin_flags af ON p.id = af.patient_id AND af.tenant_id = p.tenant_id AND af.status = 'active'
            WHERE p.tenant_id = "#,
        );
        query.push_bind(tenant_id);
        query.push(" AND p.external_id IN (");
        let mut separated = query.separated(", ");
        for external_id in external_ids {
            separated.push_bind(external_id);
        }
        separated.push_unseparated(")");
        query.push(" ORDER BY p.external_id ASC");

        Ok(query
            .build_query_as::<PatientSummary>()
            .fetch_all(&self.pool)
            .await?)
    }

    async fn upsert_judgment(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        reviewer_id: Option<Uuid>,
        judgment: &str,
        notes: Option<&str>,
    ) -> Result<Judgment> {
        let active_work_session_id = if let Some(reviewer_id) = reviewer_id {
            sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT id
                FROM local_work_sessions
                WHERE tenant_id = ?1 AND operator_id = ?2 AND status = 'active'
                ORDER BY started_at DESC
                LIMIT 1
                "#,
            )
            .bind(tenant_id)
            .bind(reviewer_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let record = sqlx::query_as::<_, Judgment>(
            r#"
            INSERT INTO judgments (
                id,
                patient_id,
                tenant_id,
                reviewer_id,
                local_work_session_id,
                judgment,
                judgment_notes,
                judgment_made_at,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(patient_id)
            DO UPDATE SET
                reviewer_id = excluded.reviewer_id,
                local_work_session_id = excluded.local_work_session_id,
                judgment = excluded.judgment,
                judgment_notes = excluded.judgment_notes,
                judgment_made_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, patient_id, tenant_id, reviewer_id, judgment, judgment_notes, judgment_made_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(patient_id)
        .bind(tenant_id)
        .bind(reviewer_id)
        .bind(active_work_session_id)
        .bind(judgment)
        .bind(notes)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    async fn get_judgment_by_patient_id(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<Judgment>> {
        let judgment = sqlx::query_as::<_, Judgment>(
            r#"
            SELECT id, patient_id, tenant_id, reviewer_id, judgment, judgment_notes, judgment_made_at
            FROM judgments
            WHERE patient_id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(judgment)
    }

    async fn upsert_admin_flag(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        created_by: Uuid,
        flag_type: &str,
        reason: &str,
    ) -> Result<AdminFlag> {
        let flag = sqlx::query_as::<_, AdminFlag>(
            r#"
            INSERT INTO admin_flags (
                id, patient_id, tenant_id, created_by, flag_type, reason, status, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(patient_id, flag_type)
            DO UPDATE SET
                created_by = excluded.created_by,
                reason = excluded.reason,
                status = 'active',
                updated_at = CURRENT_TIMESTAMP
            RETURNING id, patient_id, tenant_id, created_by, flag_type, reason, status, created_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(patient_id)
        .bind(tenant_id)
        .bind(created_by)
        .bind(flag_type)
        .bind(reason)
        .fetch_one(&self.pool)
        .await?;

        Ok(flag)
    }

    async fn get_admin_flag_by_patient_id(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<AdminFlag>> {
        let flag = sqlx::query_as::<_, AdminFlag>(
            r#"
            SELECT id, patient_id, tenant_id, created_by, flag_type, reason, status, created_at
            FROM admin_flags
            WHERE patient_id = ?1 AND tenant_id = ?2
            ORDER BY CASE WHEN status = 'active' THEN 0 ELSE 1 END, created_at DESC
            LIMIT 1
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(flag)
    }

    async fn list_admin_flags(&self, tenant_id: Uuid) -> Result<Vec<AdminFlag>> {
        let flags = sqlx::query_as::<_, AdminFlag>(
            r#"
            SELECT id, patient_id, tenant_id, created_by, flag_type, reason, status, created_at
            FROM admin_flags
            WHERE tenant_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(flags)
    }

    async fn update_admin_flag_status(
        &self,
        flag_id: Uuid,
        tenant_id: Uuid,
        new_status: &str,
        resolved_by: Uuid,
        resolution_notes: &str,
    ) -> Result<AdminFlag> {
        let flag = sqlx::query_as::<_, AdminFlag>(
            r#"
            UPDATE admin_flags
            SET status = ?1,
                resolved_by = ?2,
                resolution_notes = ?3,
                resolved_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?4 AND tenant_id = ?5
            RETURNING id, patient_id, tenant_id, created_by, flag_type, reason, status, created_at
            "#,
        )
        .bind(new_status)
        .bind(resolved_by)
        .bind(resolution_notes)
        .bind(flag_id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(flag)
    }

    async fn get_active_research_session(
        &self,
        tenant_id: Uuid,
        researcher_id: Uuid,
    ) -> Result<Option<ResearchSession>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, session_name, primary_researcher_id, status, total_patients,
                   current_chunk_number, current_chunk_patients, completed_chunks, cohort_id
            FROM research_sessions
            WHERE tenant_id = ?1 AND primary_researcher_id = ?2 AND status = 'active'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(researcher_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| map_research_session(&row)).transpose()
    }

    async fn create_research_session(
        &self,
        tenant_id: Uuid,
        session_name: &str,
        primary_researcher_id: Option<Uuid>,
        current_chunk_number: i32,
        current_chunk_patients: Vec<String>,
        completed_chunks: Vec<i32>,
    ) -> Result<ResearchSession> {
        let session_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO research_sessions (
                id, tenant_id, session_name, primary_researcher_id, status, total_patients,
                current_chunk_number, current_chunk_patients, completed_chunks,
                patients_per_chunk, started_at, created_at, updated_at, last_activity_at
            )
            VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(session_name)
        .bind(primary_researcher_id)
        .bind(current_chunk_patients.len() as i32)
        .bind(current_chunk_number)
        .bind(serialize_json(&current_chunk_patients)?)
        .bind(serialize_json(&completed_chunks)?)
        .bind(current_chunk_patients.len() as i32)
        .execute(&self.pool)
        .await?;

        self.fetch_research_session_by_id(tenant_id, session_id)
            .await
    }

    async fn update_research_session_chunk(
        &self,
        session_id: Uuid,
        tenant_id: Uuid,
        new_chunk_number: i32,
        new_chunk_patients: Vec<String>,
        newly_completed_chunk: i32,
    ) -> Result<ResearchSession> {
        let existing = self
            .fetch_research_session_by_id(tenant_id, session_id)
            .await?;
        let mut completed_chunks = existing.completed_chunks;
        if !completed_chunks.contains(&newly_completed_chunk) {
            completed_chunks.push(newly_completed_chunk);
        }

        sqlx::query(
            r#"
            UPDATE research_sessions
            SET current_chunk_number = ?1,
                current_chunk_patients = ?2,
                completed_chunks = ?3,
                updated_at = CURRENT_TIMESTAMP,
                last_activity_at = CURRENT_TIMESTAMP
            WHERE id = ?4 AND tenant_id = ?5
            "#,
        )
        .bind(new_chunk_number)
        .bind(serialize_json(&new_chunk_patients)?)
        .bind(serialize_json(&completed_chunks)?)
        .bind(session_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        self.fetch_research_session_by_id(tenant_id, session_id)
            .await
    }

    async fn complete_research_session(
        &self,
        session_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<ResearchSession> {
        sqlx::query(
            r#"
            UPDATE research_sessions
            SET status = 'completed',
                completed_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP,
                last_activity_at = CURRENT_TIMESTAMP
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        self.fetch_research_session_by_id(tenant_id, session_id)
            .await
    }

    async fn pause_research_session(
        &self,
        session_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<ResearchSession> {
        sqlx::query(
            r#"
            UPDATE research_sessions
            SET status = 'paused',
                updated_at = CURRENT_TIMESTAMP,
                last_activity_at = CURRENT_TIMESTAMP
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        self.fetch_research_session_by_id(tenant_id, session_id)
            .await
    }

    async fn create_new_active_session_from_cohort(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        session_name: &str,
        cohort_id: Uuid,
    ) -> Result<ResearchSession> {
        if session_name.trim().is_empty() {
            return Err(PlatformError::invalid_input_field(
                "Session name cannot be empty",
                "session_name",
            ));
        }

        let mut tx = self.pool.begin().await?;
        let existing_session_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM research_sessions
            WHERE tenant_id = ?1 AND primary_researcher_id = ?2 AND status = 'active'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(session_id) = existing_session_id {
            sqlx::query(
                r#"
                UPDATE research_sessions
                SET status = 'paused',
                    updated_at = CURRENT_TIMESTAMP,
                    last_activity_at = CURRENT_TIMESTAMP
                WHERE id = ?1
                "#,
            )
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        }

        let batch_rows = sqlx::query(
            r#"
            SELECT id, cohort_id, tenant_id, batch_number, patient_external_ids, is_empty, created_at, updated_at
            FROM research_cohort_batches
            WHERE cohort_id = ?1 AND tenant_id = ?2
            ORDER BY batch_number ASC
            "#,
        )
        .bind(cohort_id)
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await?;

        let cohort_batches = batch_rows
            .iter()
            .map(map_research_cohort_batch)
            .collect::<Result<Vec<_>>>()?;

        let first_batch = cohort_batches.iter().find(|batch| !batch.is_empty).ok_or_else(|| {
            PlatformError::unprocessable_entity_with_reason(
                "This cohort has no pre-composed batches. Re-ingest the cohort data before starting a review session.",
                "no_cohort_batches",
            )
        })?;

        let total_patients: i32 = cohort_batches
            .iter()
            .filter(|batch| !batch.is_empty)
            .map(|batch| batch.patient_external_ids.len() as i32)
            .sum();

        let session_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO research_sessions (
                id, tenant_id, session_name, primary_researcher_id, status, total_patients,
                patients_per_chunk, current_chunk_number, current_chunk_patients, completed_chunks,
                cohort_id, started_at, created_at, updated_at, last_activity_at
            )
            VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?8, '[]', ?9, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(session_name)
        .bind(user_id)
        .bind(total_patients)
        .bind(first_batch.patient_external_ids.len() as i32)
        .bind(first_batch.batch_number)
        .bind(serialize_json(&first_batch.patient_external_ids)?)
        .bind(cohort_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        self.fetch_research_session_by_id(tenant_id, session_id)
            .await
    }

    async fn update_patient_review_status(
        &self,
        patient_id: Uuid,
        tenant_id: Uuid,
        new_status: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE patients
            SET review_status = ?1, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?2 AND tenant_id = ?3
            "#,
        )
        .bind(new_status)
        .bind(patient_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_research_cohorts_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<ResearchCohort>> {
        let rows = sqlx::query(
            r#"
            SELECT rc.id, rc.tenant_id, rc.name, rc.description, rc.external_cohort_id,
                   rc.total_patients, rc.cohort_type, rc.selection_criteria, rc.created_by,
                   rc.research_protocol, rc.study_metadata, rc.status, rc.created_at,
                   rc.updated_at, rc.archived_at, rc.version, rc.metadata
            FROM research_cohorts rc
            JOIN research_cohort_reviewers rcr ON rc.id = rcr.cohort_id
            WHERE rc.tenant_id = ?1 AND rcr.user_id = ?2 AND rc.status = 'active'
            ORDER BY rc.created_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_research_cohort).collect()
    }

    async fn get_research_cohort_with_access(
        &self,
        tenant_id: Uuid,
        cohort_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ResearchCohort>> {
        let row = sqlx::query(
            r#"
            SELECT rc.id, rc.tenant_id, rc.name, rc.description, rc.external_cohort_id,
                   rc.total_patients, rc.cohort_type, rc.selection_criteria, rc.created_by,
                   rc.research_protocol, rc.study_metadata, rc.status, rc.created_at,
                   rc.updated_at, rc.archived_at, rc.version, rc.metadata
            FROM research_cohorts rc
            JOIN research_cohort_reviewers rcr ON rc.id = rcr.cohort_id
            WHERE rc.tenant_id = ?1 AND rc.id = ?2 AND rcr.user_id = ?3
            "#,
        )
        .bind(tenant_id)
        .bind(cohort_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| map_research_cohort(&row)).transpose()
    }

    async fn get_research_cohort_patients(
        &self,
        tenant_id: Uuid,
        cohort_id: Uuid,
    ) -> Result<Vec<ResearchCohortPatient>> {
        let rows = sqlx::query(
            r#"
            SELECT rcp.cohort_id, rcp.patient_id, rcp.display_order,
                   rcp.inclusion_reason, rcp.patient_metadata, rcp.added_at, rcp.added_by
            FROM research_cohort_patients rcp
            JOIN patients p ON p.id = rcp.patient_id
            WHERE rcp.cohort_id = ?1 AND p.tenant_id = ?2
            ORDER BY rcp.display_order ASC
            "#,
        )
        .bind(cohort_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_research_cohort_patient).collect()
    }

    async fn get_research_cohort_reviewer(
        &self,
        tenant_id: Uuid,
        cohort_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ResearchCohortReviewer>> {
        let row = sqlx::query(
            r#"
            SELECT rcr.cohort_id, rcr.user_id, rcr.role, rcr.can_review,
                   rcr.can_export, rcr.can_modify_cohort, rcr.granted_at,
                   rcr.granted_by, rcr.expires_at, rcr.access_metadata
            FROM research_cohort_reviewers rcr
            JOIN research_cohorts rc ON rc.id = rcr.cohort_id
            WHERE rc.tenant_id = ?1 AND rcr.cohort_id = ?2 AND rcr.user_id = ?3
            "#,
        )
        .bind(tenant_id)
        .bind(cohort_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| map_research_cohort_reviewer(&row))
            .transpose()
    }

    async fn batch_insert_cohort_ingestion(
        &self,
        cohort_name: &str,
        tenant_slug: &str,
        patient_external_ids: &[String],
    ) -> Result<i32> {
        if patient_external_ids.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        for (index, patient_external_id) in patient_external_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO cohort_ingestion (
                    id, cohort_name, patient_external_id, tenant_slug, status, display_order, created_at
                )
                VALUES (?1, ?2, ?3, ?4, 'pending', ?5, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(cohort_name)
            .bind(patient_external_id)
            .bind(tenant_slug)
            .bind(index as i32 + 1)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(patient_external_ids.len() as i32)
    }

    async fn process_pending_cohort_ingestion(
        &self,
        cohort_name: &str,
        tenant_slug: &str,
        user_id: Uuid,
        description: Option<&str>,
    ) -> Result<(Uuid, i32, i32, i32, serde_json::Value)> {
        let tenant = self.get_tenant_by_slug(tenant_slug).await?;
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            r#"
            SELECT id, patient_external_id, display_order
            FROM cohort_ingestion
            WHERE cohort_name = ?1 AND tenant_slug = ?2 AND status = 'pending'
            ORDER BY COALESCE(display_order, 0) ASC, created_at ASC
            "#,
        )
        .bind(cohort_name)
        .bind(tenant_slug)
        .fetch_all(&mut *tx)
        .await?;

        if rows.is_empty() {
            return Err(PlatformError::unprocessable_entity_with_reason(
                "No pending cohort ingestion rows found",
                "empty_ingestion_set",
            ));
        }

        let cohort_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO research_cohorts (
                id, tenant_id, name, description, total_patients, cohort_type, created_by, status, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, 0, 'imported', ?5, 'active', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(tenant_id, name)
            DO UPDATE SET description = excluded.description, updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(cohort_id)
        .bind(tenant.id)
        .bind(cohort_name)
        .bind(description)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        let persisted_cohort_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM research_cohorts WHERE tenant_id = ?1 AND name = ?2",
        )
        .bind(tenant.id)
        .bind(cohort_name)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO research_cohort_reviewers (
                cohort_id, user_id, role, can_review, can_export, can_modify_cohort, granted_at, granted_by, access_metadata
            )
            VALUES (?1, ?2, 'reviewer', 1, 0, 0, CURRENT_TIMESTAMP, ?2, ?3)
            ON CONFLICT(cohort_id, user_id)
            DO UPDATE SET granted_at = CURRENT_TIMESTAMP, granted_by = excluded.granted_by
            "#,
        )
        .bind(persisted_cohort_id)
        .bind(user_id)
        .bind(json!({"source": "cohort_ingestion"}).to_string())
        .execute(&mut *tx)
        .await?;

        let mut processed_patients = 0;
        let mut error_count = 0;

        for row in rows {
            let ingestion_id: Uuid = row.get("id");
            let patient_external_id: String = row.get("patient_external_id");
            let display_order: i32 = row.get("display_order");
            let patient_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM patients WHERE tenant_id = ?1 AND external_id = ?2",
            )
            .bind(tenant.id)
            .bind(&patient_external_id)
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(patient_id) = patient_id {
                sqlx::query(
                    r#"
                    INSERT INTO research_cohort_patients (
                        cohort_id, patient_id, display_order, inclusion_reason, added_at, added_by
                    )
                    VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, ?5)
                    ON CONFLICT(cohort_id, patient_id)
                    DO UPDATE SET display_order = excluded.display_order
                    "#,
                )
                .bind(persisted_cohort_id)
                .bind(patient_id)
                .bind(display_order)
                .bind("Imported from cohort_ingestion")
                .bind(user_id)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    r#"
                    UPDATE cohort_ingestion
                    SET status = 'processed', error_message = NULL, processed_at = CURRENT_TIMESTAMP
                    WHERE id = ?1
                    "#,
                )
                .bind(ingestion_id)
                .execute(&mut *tx)
                .await?;

                processed_patients += 1;
            } else {
                sqlx::query(
                    r#"
                    UPDATE cohort_ingestion
                    SET status = 'error', error_message = ?2, processed_at = CURRENT_TIMESTAMP
                    WHERE id = ?1
                    "#,
                )
                .bind(ingestion_id)
                .bind(format!(
                    "Patient '{patient_external_id}' not found in tenant '{tenant_slug}'"
                ))
                .execute(&mut *tx)
                .await?;
                error_count += 1;
            }
        }

        sqlx::query(
            "UPDATE research_cohorts SET total_patients = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        )
        .bind(processed_patients)
        .bind(persisted_cohort_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let total_patients = processed_patients + error_count;
        let summary = json!({
            "cohort_name": cohort_name,
            "tenant_slug": tenant_slug,
            "processed_patients": processed_patients,
            "error_count": error_count
        });

        Ok((
            persisted_cohort_id,
            total_patients,
            processed_patients,
            error_count,
            summary,
        ))
    }

    async fn get_patient_by_id_with_tenant(
        &self,
        tenant_id: Uuid,
        patient_id: Uuid,
    ) -> Result<Option<Patient>> {
        let patient = sqlx::query_as::<_, Patient>(
            r#"
            SELECT id, external_id, age, sex, tenant_id, created_at, updated_at
            FROM patients
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(patient_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(patient)
    }

    async fn create_research_session_enhanced(
        &self,
        tenant_id: Uuid,
        session_name: &str,
        primary_researcher_id: Option<Uuid>,
        patient_external_ids: &[String],
        chunk_size: usize,
    ) -> Result<ResearchSession> {
        let session_id = Uuid::new_v4();
        let current_chunk_patients: Vec<String> = patient_external_ids
            .iter()
            .take(chunk_size)
            .cloned()
            .collect();

        sqlx::query(
            r#"
            INSERT INTO research_sessions (
                id, tenant_id, session_name, primary_researcher_id, status, total_patients,
                patients_per_chunk, current_chunk_number, current_chunk_patients, completed_chunks,
                started_at, created_at, updated_at, last_activity_at
            )
            VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, 1, ?7, '[]', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(session_name)
        .bind(primary_researcher_id)
        .bind(patient_external_ids.len() as i32)
        .bind(chunk_size as i32)
        .bind(serialize_json(&current_chunk_patients)?)
        .execute(&self.pool)
        .await?;

        self.fetch_research_session_by_id(tenant_id, session_id)
            .await
    }

    async fn batch_get_patients_by_external_ids(
        &self,
        external_ids: &[String],
        tenant_id: Uuid,
    ) -> Result<HashMap<String, Patient>> {
        if external_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT id, external_id, age, sex, tenant_id, created_at, updated_at FROM patients WHERE tenant_id = ",
        );
        query.push_bind(tenant_id);
        query.push(" AND external_id IN (");
        let mut separated = query.separated(", ");
        for external_id in external_ids {
            separated.push_bind(external_id);
        }
        separated.push_unseparated(")");

        let rows = query
            .build_query_as::<Patient>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|patient| (patient.external_id.clone(), patient))
            .collect())
    }

    async fn batch_get_judgments_by_patient_ids(
        &self,
        patient_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<HashMap<Uuid, Judgment>> {
        if patient_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT id, patient_id, tenant_id, reviewer_id, judgment, judgment_notes, judgment_made_at FROM judgments WHERE tenant_id = ",
        );
        query.push_bind(tenant_id);
        query.push(" AND patient_id IN (");
        let mut separated = query.separated(", ");
        for patient_id in patient_ids {
            separated.push_bind(patient_id);
        }
        separated.push_unseparated(")");

        let rows = query
            .build_query_as::<Judgment>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|judgment| (judgment.patient_id, judgment))
            .collect())
    }

    async fn batch_check_patients_have_judgments(
        &self,
        patient_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<HashSet<Uuid>> {
        if patient_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT DISTINCT patient_id FROM judgments WHERE tenant_id = ",
        );
        query.push_bind(tenant_id);
        query.push(" AND patient_id IN (");
        let mut separated = query.separated(", ");
        for patient_id in patient_ids {
            separated.push_bind(patient_id);
        }
        separated.push_unseparated(")");

        let rows: Vec<(Uuid,)> = query.build_query_as().fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    async fn create_cohort_batches(
        &self,
        cohort_id: Uuid,
        tenant_id: Uuid,
        patient_external_ids: &[String],
        batch_size: usize,
        include_empty_placeholder: bool,
    ) -> Result<Vec<ResearchCohortBatch>> {
        if batch_size == 0 {
            return Err(PlatformError::invalid_input_field(
                "batch_size must be greater than 0",
                "batch_size",
            ));
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM research_cohort_batches WHERE cohort_id = ?1 AND tenant_id = ?2")
            .bind(cohort_id)
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;

        let mut batches = Vec::new();
        let mut batch_number = 1i32;

        for patient_chunk in patient_external_ids.chunks(batch_size) {
            let row = sqlx::query(
                r#"
                INSERT INTO research_cohort_batches (
                    id, cohort_id, tenant_id, batch_number, patient_external_ids, is_empty, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                RETURNING id, cohort_id, tenant_id, batch_number, patient_external_ids, is_empty, created_at, updated_at
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(cohort_id)
            .bind(tenant_id)
            .bind(batch_number)
            .bind(serialize_json(&patient_chunk)?)
            .fetch_one(&mut *tx)
            .await?;
            batches.push(map_research_cohort_batch(&row)?);
            batch_number += 1;
        }

        if include_empty_placeholder {
            let row = sqlx::query(
                r#"
                INSERT INTO research_cohort_batches (
                    id, cohort_id, tenant_id, batch_number, patient_external_ids, is_empty, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, '[]', 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                RETURNING id, cohort_id, tenant_id, batch_number, patient_external_ids, is_empty, created_at, updated_at
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(cohort_id)
            .bind(tenant_id)
            .bind(batch_number)
            .fetch_one(&mut *tx)
            .await?;
            batches.push(map_research_cohort_batch(&row)?);
        }

        tx.commit().await?;
        Ok(batches)
    }

    async fn get_cohort_batches(
        &self,
        cohort_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ResearchCohortBatch>> {
        let rows = sqlx::query(
            r#"
            SELECT id, cohort_id, tenant_id, batch_number, patient_external_ids, is_empty, created_at, updated_at
            FROM research_cohort_batches
            WHERE cohort_id = ?1 AND tenant_id = ?2
            ORDER BY batch_number ASC
            "#,
        )
        .bind(cohort_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_research_cohort_batch).collect()
    }

    async fn batch_get_patient_external_ids_by_uuids(
        &self,
        patient_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<HashMap<Uuid, String>> {
        if patient_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut query =
            QueryBuilder::<Sqlite>::new("SELECT id, external_id FROM patients WHERE tenant_id = ");
        query.push_bind(tenant_id);
        query.push(" AND id IN (");
        let mut separated = query.separated(", ");
        for patient_id in patient_ids {
            separated.push_bind(patient_id);
        }
        separated.push_unseparated(")");

        let rows = query.build().fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<Uuid, _>("id"),
                    row.get::<String, _>("external_id"),
                )
            })
            .collect())
    }
}

fn normalize_sqlite_connection_string(connection_string: &str) -> Result<String> {
    if connection_string.starts_with("sqlite://") {
        return Ok(connection_string.to_string());
    }

    Err(PlatformError::config(format!(
        "Unsupported database connection string format. Expected sqlite://<path>, got: {connection_string}"
    )))
}

fn ensure_sqlite_parent_directory(connection_string: &str) -> Result<()> {
    let path = connection_string
        .strip_prefix("sqlite://")
        .expect("connection string was normalized before path preparation")
        .split('?')
        .next()
        .unwrap_or_default();
    if path.is_empty() {
        return Err(PlatformError::config(
            "SQLite connection string must include a database path",
        ));
    }
    if path == ":memory:" || path.starts_with("file:") {
        return Ok(());
    }

    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            PlatformError::config(format!(
                "Could not create SQLite database directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    Ok(())
}

fn slugify(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn serialize_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(PlatformError::from)
}

fn parse_json_or_default<T>(value: &str) -> Result<T>
where
    T: DeserializeOwned + Default,
{
    if value.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(value).map_err(PlatformError::from)
}

fn parse_optional_json(value: Option<String>) -> Result<Option<serde_json::Value>> {
    value
        .map(|json| serde_json::from_str(&json).map_err(PlatformError::from))
        .transpose()
}

fn map_research_session(row: &SqliteRow) -> Result<ResearchSession> {
    Ok(ResearchSession {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        session_name: row.try_get("session_name")?,
        primary_researcher_id: row.try_get("primary_researcher_id")?,
        status: row.try_get("status")?,
        total_patients: row.try_get("total_patients")?,
        current_chunk_number: row.try_get("current_chunk_number")?,
        current_chunk_patients: parse_json_or_default(
            &row.try_get::<String, _>("current_chunk_patients")?,
        )?,
        completed_chunks: parse_json_or_default(&row.try_get::<String, _>("completed_chunks")?)?,
        cohort_id: row.try_get("cohort_id")?,
    })
}

fn map_research_cohort(row: &SqliteRow) -> Result<ResearchCohort> {
    Ok(ResearchCohort {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        external_cohort_id: row.try_get("external_cohort_id")?,
        total_patients: row.try_get("total_patients")?,
        cohort_type: row.try_get("cohort_type")?,
        selection_criteria: parse_optional_json(row.try_get("selection_criteria")?)?,
        created_by: row.try_get("created_by")?,
        research_protocol: row.try_get("research_protocol")?,
        study_metadata: parse_optional_json(row.try_get("study_metadata")?)?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        archived_at: row.try_get("archived_at")?,
        version: row.try_get("version")?,
        metadata: parse_optional_json(row.try_get("metadata")?)?,
    })
}

fn map_research_cohort_patient(row: &SqliteRow) -> Result<ResearchCohortPatient> {
    Ok(ResearchCohortPatient {
        cohort_id: row.try_get("cohort_id")?,
        patient_id: row.try_get("patient_id")?,
        display_order: row.try_get("display_order")?,
        inclusion_reason: row.try_get("inclusion_reason")?,
        patient_metadata: parse_optional_json(row.try_get("patient_metadata")?)?,
        added_at: row.try_get("added_at")?,
        added_by: row.try_get("added_by")?,
    })
}

fn map_research_cohort_reviewer(row: &SqliteRow) -> Result<ResearchCohortReviewer> {
    Ok(ResearchCohortReviewer {
        cohort_id: row.try_get("cohort_id")?,
        user_id: row.try_get("user_id")?,
        role: row.try_get("role")?,
        can_review: row.try_get("can_review")?,
        can_export: row.try_get("can_export")?,
        can_modify_cohort: row.try_get("can_modify_cohort")?,
        granted_at: row.try_get("granted_at")?,
        granted_by: row.try_get("granted_by")?,
        expires_at: row.try_get("expires_at")?,
        access_metadata: parse_optional_json(row.try_get("access_metadata")?)?,
    })
}

fn map_research_cohort_batch(row: &SqliteRow) -> Result<ResearchCohortBatch> {
    Ok(ResearchCohortBatch {
        id: row.try_get("id")?,
        cohort_id: row.try_get("cohort_id")?,
        tenant_id: row.try_get("tenant_id")?,
        batch_number: row.try_get("batch_number")?,
        patient_external_ids: parse_json_or_default(
            &row.try_get::<String, _>("patient_external_ids")?,
        )?,
        is_empty: row.try_get("is_empty")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod sqlite_url_tests {
    use super::*;

    #[test]
    fn bare_database_paths_are_rejected() {
        let error = normalize_sqlite_connection_string("review.sqlite3")
            .expect_err("a bare database path must not be accepted");
        assert!(error.to_string().contains("sqlite://"));
    }

    #[test]
    fn parent_directories_are_created_for_file_databases() {
        let directory = tempfile::tempdir().expect("temporary parent directory");
        let database_path = directory.path().join("nested/review.sqlite3");
        let database_url = format!("sqlite://{}", database_path.display());

        ensure_sqlite_parent_directory(&database_url).expect("database parent directory");

        assert!(database_path.parent().unwrap().is_dir());
    }
}
