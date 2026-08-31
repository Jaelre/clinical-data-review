use anyhow::{Context, Result};
use platform_db::{DatabaseConnection, DatabaseConnectionType};
use std::collections::HashSet;
use std::fs;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CohortIngestionConfig {
    pub file_path: String,
    pub database_url: String,
    pub cohort_name: String,
    pub tenant_slug: String,
    pub operator_handle: String, // Local operator handle, not a login credential
    pub description: Option<String>,
    /// If Some, create a research session after cohort batches are materialized
    pub session_name: Option<String>,
    pub batch_size: usize,
    pub include_empty_placeholder: bool,
    pub dry_run: bool,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct CohortIngestionResult {
    pub cohort_id: Option<Uuid>,
    pub total_patients: i32,
    pub processed_patients: i32,
    pub error_count: i32,
    pub ingestion_summary: serde_json::Value,
    /// Set when session_name was provided and a session was created
    pub session_id: Option<Uuid>,
    pub batch_count: Option<usize>,
}

pub struct CohortIngestionProcessor {
    config: CohortIngestionConfig,
}

impl CohortIngestionProcessor {
    pub fn new(config: CohortIngestionConfig) -> Self {
        Self { config }
    }

    /// Execute the complete cohort ingestion workflow
    pub async fn process(&self) -> Result<CohortIngestionResult> {
        if let Some(session_name) = self.config.session_name.as_deref() {
            if session_name.trim().is_empty() {
                return Err(anyhow::anyhow!("Session name cannot be empty"));
            }
        }

        if self.config.batch_size == 0 {
            return Err(anyhow::anyhow!("Batch size must be greater than 0"));
        }

        if self.config.verbose {
            println!("🚀 Starting cohort ingestion workflow");
            println!("   📁 File: {}", self.config.file_path);
            println!("   🏷️  Cohort: {}", self.config.cohort_name);
            println!("   🏠 Tenant: {}", self.config.tenant_slug);
            println!("   🧑‍💻 Local operator: {}", self.config.operator_handle);
            if self.config.dry_run {
                println!("   🧪 Mode: DRY RUN (no data will be inserted)");
            }
        }

        // Step 1: Read and parse patient IDs from file
        let patient_ids = self.read_patient_ids()?;

        if patient_ids.is_empty() {
            return Err(anyhow::anyhow!("No patient IDs found in file"));
        }

        if self.config.verbose {
            println!("   📊 Found {} patient IDs", patient_ids.len());
        }

        if self.config.dry_run {
            println!(
                "✅ DRY RUN: Validation successful. Found {} patient IDs",
                patient_ids.len()
            );
            println!(
                "   Sample IDs: {:?}",
                patient_ids.iter().take(5).collect::<Vec<_>>()
            );
            let full_batches = patient_ids.len().div_ceil(self.config.batch_size);
            let batch_count = Some(if self.config.include_empty_placeholder {
                full_batches + 1
            } else {
                full_batches
            });
            return Ok(CohortIngestionResult {
                cohort_id: None,
                total_patients: patient_ids.len() as i32,
                processed_patients: 0,
                error_count: 0,
                ingestion_summary: serde_json::json!({
                    "dry_run": true,
                    "total_ids": patient_ids.len(),
                    "sample_ids": patient_ids.iter().take(5).collect::<Vec<_>>()
                }),
                session_id: None,
                batch_count,
            });
        }

        // Step 2: Connect to database
        let db = DatabaseConnectionType::new(&self.config.database_url)
            .await
            .context("Failed to connect to database")?;

        if self.config.verbose {
            println!("   🔌 Connected to database");
        }

        self.validate_patient_ids(&db, &patient_ids).await?;

        // Step 3: Insert patient IDs into cohort_ingestion table
        self.insert_into_ingestion_table(&db, &patient_ids).await?;

        if self.config.verbose {
            println!(
                "   📥 Inserted {} records into cohort_ingestion table",
                patient_ids.len()
            );
        }

        // Step 4: Execute the atomic processing function
        let mut result = self.process_cohort_ingestion(&db).await?;

        if self.config.verbose {
            println!("   ⚡ Processed cohort ingestion atomically");
            println!("   🎯 Cohort ID: {}", result.cohort_id.unwrap_or_default());
            println!(
                "   ✅ Successfully processed: {}",
                result.processed_patients
            );
            if result.error_count > 0 {
                println!("   ⚠️  Errors encountered: {}", result.error_count);
            }
        }

        if let Some(cohort_id) = result.cohort_id {
            let batch_count = self.create_cohort_batches(&db, cohort_id).await?;
            result.batch_count = Some(batch_count);

            if let Some(session_name) = self.config.session_name.as_deref() {
                let session_id = self
                    .create_session_from_cohort_batches(&db, session_name, cohort_id)
                    .await?;
                result.session_id = Some(session_id);
            }
        }

        Ok(result)
    }

    async fn validate_patient_ids(
        &self,
        db: &DatabaseConnectionType,
        patient_ids: &[String],
    ) -> Result<()> {
        let tenant = db
            .get_tenant_by_slug(&self.config.tenant_slug)
            .await
            .with_context(|| format!("Workspace '{}' not found", self.config.tenant_slug))?;
        let known_ids: HashSet<String> = db
            .get_patients_by_external_ids(patient_ids, tenant.id)
            .await
            .context("Failed to validate cohort patient identifiers")?
            .into_iter()
            .map(|patient| patient.external_id)
            .collect();
        let missing: Vec<&str> = patient_ids
            .iter()
            .filter(|patient_id| !known_ids.contains(*patient_id))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            return Err(anyhow::anyhow!(
                "Unknown patient identifiers for workspace '{}': {}",
                self.config.tenant_slug,
                missing.join(", ")
            ));
        }
        Ok(())
    }

    /// Read patient IDs from file with smart parsing
    fn read_patient_ids(&self) -> Result<Vec<String>> {
        let content = fs::read_to_string(&self.config.file_path)
            .with_context(|| format!("Failed to read file: {}", self.config.file_path))?;

        if self.config.verbose {
            println!("   📖 Read {} characters from file", content.len());
        }

        // Smart parsing: support newlines, semicolons, and commas
        let mut patient_ids = Vec::new();

        // Split by multiple delimiters and clean up
        for line in content.lines() {
            // Split each line by semicolons and commas
            for segment in line.split(&[';', ',']) {
                let trimmed = segment.trim();
                if !trimmed.is_empty() {
                    patient_ids.push(trimmed.to_string());
                }
            }
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        patient_ids.retain(|id| seen.insert(id.clone()));

        if self.config.verbose && patient_ids.len() != content.lines().count() {
            println!(
                "   🧹 Cleaned {} duplicates, final count: {}",
                content.lines().count() - patient_ids.len(),
                patient_ids.len()
            );
        }

        Ok(patient_ids)
    }

    /// Insert patient IDs into the cohort_ingestion staging table using platform-db bulk insert
    async fn insert_into_ingestion_table(
        &self,
        db: &DatabaseConnectionType,
        patient_ids: &[String],
    ) -> Result<()> {
        if self.config.verbose {
            println!(
                "   📥 Starting bulk insert of {} patient IDs",
                patient_ids.len()
            );
        }

        // Use the platform-db bulk insert method
        let inserted_count = db
            .batch_insert_cohort_ingestion(
                &self.config.cohort_name,
                &self.config.tenant_slug,
                patient_ids,
            )
            .await
            .context("Failed to bulk insert patient IDs into cohort_ingestion table")?;

        if self.config.verbose {
            println!(
                "   ✅ Successfully inserted {} records into cohort_ingestion table",
                inserted_count
            );
        }

        Ok(())
    }

    /// Execute the atomic cohort processing function using platform-db methods
    async fn process_cohort_ingestion(
        &self,
        db: &DatabaseConnectionType,
    ) -> Result<CohortIngestionResult> {
        if self.config.verbose {
            println!("   ⚡ Looking up local operator and processing cohort ingestion");
        }

        let operator_record_key = self.config.local_operator_record_key();

        // Look up the local operator record using the shared database contract
        let operator = db
            .get_user_by_email(&operator_record_key)
            .await
            .with_context(|| {
                format!(
                    "Local operator '{}' not found. Seed a local operator profile in the SQLite workspace before running cohort import.",
                    self.config.operator_handle
                )
            })?;

        if self.config.verbose {
            println!(
                "   👤 Found operator ID: {} for handle: {}",
                operator.id, self.config.operator_handle
            );
        }

        // Call the shared cohort ingestion function using the local operator identity
        let (cohort_id, total_patients, processed_patients, error_count, processing_summary) = db
            .process_pending_cohort_ingestion(
                &self.config.cohort_name,
                &self.config.tenant_slug,
                operator.id,
                self.config.description.as_deref(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to process cohort ingestion for cohort: {} in tenant: {}",
                    self.config.cohort_name, self.config.tenant_slug
                )
            })?;

        if self.config.verbose {
            println!("   🎯 Created cohort ID: {}", cohort_id);
            println!(
                "   📊 Processed {} of {} patients",
                processed_patients, total_patients
            );
            if error_count > 0 {
                println!(
                    "   ⚠️  Encountered {} errors during processing",
                    error_count
                );
            }
        }

        Ok(CohortIngestionResult {
            cohort_id: Some(cohort_id),
            total_patients,
            processed_patients,
            error_count,
            ingestion_summary: processing_summary,
            session_id: None,  // populated later if session_name is set
            batch_count: None, // populated later if session_name is set
        })
    }

    /// Populate ETL-authored cohort batches from the just-created cohort.
    async fn create_cohort_batches(
        &self,
        db: &DatabaseConnectionType,
        cohort_id: Uuid,
    ) -> Result<usize> {
        if self.config.verbose {
            println!("   📦 Creating cohort batches...");
        }

        // Resolve tenant_id
        let tenant = db
            .get_tenant_by_slug(&self.config.tenant_slug)
            .await
            .with_context(|| format!("Tenant '{}' not found", self.config.tenant_slug))?;

        // Load ordered patients from the cohort (ordered by display_order)
        let cohort_patients = db
            .get_research_cohort_patients(tenant.id, cohort_id)
            .await
            .context("Failed to fetch cohort patients")?;

        if cohort_patients.is_empty() {
            return Err(anyhow::anyhow!(
                "Cohort has no patients — cannot create cohort batches"
            ));
        }

        // Resolve patient UUIDs → external_ids preserving display_order
        let patient_uuids: Vec<Uuid> = cohort_patients.iter().map(|cp| cp.patient_id).collect();
        let uuid_to_external = db
            .batch_get_patient_external_ids_by_uuids(&patient_uuids, tenant.id)
            .await
            .context("Failed to resolve patient external IDs")?;

        let ordered_external_ids: Vec<String> = cohort_patients
            .iter()
            .filter_map(|cp| uuid_to_external.get(&cp.patient_id).cloned())
            .collect();

        if ordered_external_ids.is_empty() {
            return Err(anyhow::anyhow!(
                "Could not resolve any patient external IDs from cohort"
            ));
        }

        let batch_count = ordered_external_ids.len().div_ceil(self.config.batch_size)
            + usize::from(self.config.include_empty_placeholder);

        db.create_cohort_batches(
            cohort_id,
            tenant.id,
            &ordered_external_ids,
            self.config.batch_size,
            self.config.include_empty_placeholder,
        )
        .await
        .context("Failed to create cohort batches")?;

        if self.config.verbose {
            println!(
                "   ✅ Created {} cohort batch records (batch 1 has {} patients)",
                batch_count,
                ordered_external_ids.len().min(self.config.batch_size)
            );
        }

        Ok(batch_count)
    }

    /// Create a research session that consumes existing cohort batches.
    async fn create_session_from_cohort_batches(
        &self,
        db: &DatabaseConnectionType,
        session_name: &str,
        cohort_id: Uuid,
    ) -> Result<Uuid> {
        if self.config.verbose {
            println!("   📋 Creating research session '{}'...", session_name);
        }

        let tenant = db
            .get_tenant_by_slug(&self.config.tenant_slug)
            .await
            .with_context(|| format!("Tenant '{}' not found", self.config.tenant_slug))?;

        let operator_record_key = self.config.local_operator_record_key();
        let operator = db
            .get_user_by_email(&operator_record_key)
            .await
            .with_context(|| {
                format!(
                    "Local operator '{}' not found. Seed a local operator profile in the SQLite workspace before creating a session.",
                    self.config.operator_handle
                )
            })?;

        let session = db
            .create_new_active_session_from_cohort(tenant.id, operator.id, session_name, cohort_id)
            .await
            .context("Failed to create research session")?;

        if self.config.verbose {
            println!("   ✅ Session created: {}", session.id);
        }

        Ok(session.id)
    }
}

// Configuration builder for easier setup
impl CohortIngestionConfig {
    pub fn new(
        file_path: String,
        database_url: String,
        cohort_name: String,
        tenant_slug: String,
        operator_handle: String, // Local operator handle
    ) -> Self {
        Self {
            file_path,
            database_url,
            cohort_name,
            tenant_slug,
            operator_handle,
            description: None,
            session_name: None,
            batch_size: 12,
            include_empty_placeholder: false,
            dry_run: false,
            verbose: false,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    pub fn with_session_name(mut self, session_name: Option<String>) -> Self {
        self.session_name = session_name;
        self
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn with_include_empty_placeholder(mut self, include: bool) -> Self {
        self.include_empty_placeholder = include;
        self
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    fn local_operator_record_key(&self) -> String {
        let slug = self
            .operator_handle
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<&str>>()
            .join("-");

        if slug.is_empty() {
            "example-operator@example.invalid".to_string()
        } else {
            format!("{}@example.invalid", slug)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_db::{DatabaseConnection, DatabaseConnectionType};
    use tempfile::TempDir;

    fn test_database_url(test_name: &str) -> (TempDir, String) {
        let temp_dir = tempfile::tempdir().expect("failed to create temporary directory");
        let database_path = temp_dir.path().join(format!("{test_name}.sqlite"));
        (temp_dir, format!("sqlite://{}", database_path.display()))
    }

    async fn setup_database(test_name: &str) -> anyhow::Result<(TempDir, String)> {
        let (temp_dir, database_url) = test_database_url(test_name);
        let db = DatabaseConnectionType::new(&database_url).await?;
        db.run_migrations().await?;

        let tenant = db
            .create_tenant("Example Research Workspace", "example-research-workspace")
            .await?;
        db.create_local_operator(
            tenant.id,
            "Example Reviewer",
            Some("example-reviewer"),
            Some("example-reviewer@example.invalid"),
            "reviewer",
        )
        .await?;
        db.create_patient("SYNTH-001", Some(42), Some("F"), tenant.id)
            .await?;

        Ok((temp_dir, database_url))
    }

    fn write_cohort_file(contents: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("failed to create cohort temp file");
        std::fs::write(file.path(), contents).expect("failed to write cohort fixture file");
        file
    }

    #[test]
    fn test_local_operator_record_key_normalizes_handle() {
        let config = CohortIngestionConfig::new(
            "cohort.txt".to_string(),
            "sqlite://./clinical.db".to_string(),
            "synthetic-review".to_string(),
            "example-research-workspace".to_string(),
            " Dr. Jane   Doe ".to_string(),
        );

        assert_eq!(
            config.local_operator_record_key(),
            "dr-jane-doe@example.invalid"
        );

        let blank_handle = CohortIngestionConfig::new(
            "cohort.txt".to_string(),
            "sqlite://./clinical.db".to_string(),
            "synthetic-review".to_string(),
            "example-research-workspace".to_string(),
            "   ".to_string(),
        );

        assert_eq!(
            blank_handle.local_operator_record_key(),
            "example-operator@example.invalid"
        );
    }

    #[tokio::test]
    async fn test_cohort_ingestion_rejects_unknown_patients() {
        let (_temp_dir, database_url) = setup_database("cohort-success")
            .await
            .expect("failed to initialize sqlite database");
        let cohort_file = write_cohort_file("SYNTH-001\nSYNTH-001\nMISSING-1\n");

        let processor = CohortIngestionProcessor::new(CohortIngestionConfig::new(
            cohort_file.path().display().to_string(),
            database_url.clone(),
            "synthetic-review".to_string(),
            "example-research-workspace".to_string(),
            "example-reviewer".to_string(),
        ));

        let error = processor
            .process()
            .await
            .expect_err("unknown patients must reject the complete cohort import");
        assert!(error.to_string().contains("MISSING-1"));

        let db = DatabaseConnectionType::new(&database_url)
            .await
            .expect("failed to reopen sqlite database");
        let tenant = db
            .get_tenant_by_slug("example-research-workspace")
            .await
            .expect("failed to resolve tenant");
        let operator = db
            .get_user_by_email("example-reviewer@example.invalid")
            .await
            .expect("failed to resolve local reviewer");
        let cohorts = db
            .get_research_cohorts_for_user(tenant.id, operator.id)
            .await
            .expect("failed to list cohorts for seeded local reviewer");

        assert!(cohorts.is_empty(), "rejected imports must create no cohort");
    }

    #[tokio::test]
    async fn test_cohort_ingestion_fails_when_local_operator_is_missing() {
        let (_temp_dir, database_url) = setup_database("cohort-missing-operator")
            .await
            .expect("failed to initialize sqlite database");
        let cohort_file = write_cohort_file("SYNTH-001\n");

        let processor = CohortIngestionProcessor::new(CohortIngestionConfig::new(
            cohort_file.path().display().to_string(),
            database_url,
            "synthetic-review".to_string(),
            "example-research-workspace".to_string(),
            "unknown local reviewer".to_string(),
        ));

        let error = processor
            .process()
            .await
            .expect_err("missing local operators must fail cohort attribution");

        let message = format!("{error:#}");
        assert!(message.contains("Local operator 'unknown local reviewer' not found"));
    }
}
