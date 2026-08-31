use super::etl_config::EtlMappingConfig;
use crate::domains::purging::{PersonalInfoPurger, TextRedactionStats};
use crate::infrastructure::data_source::{DataLoader, DataSource, ExcelLoader};
use indexmap::IndexMap;
use platform_db::{DatabaseConnection, DatabaseConnectionType};
use platform_errors::{PlatformError, Result};
use platform_models::Tenant;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use uuid::Uuid;
// Note: sqlx imports not needed since we use database methods instead of direct pool access

/// Aggregates patient data from Excel files into categorized notes
pub struct PatientAggregator<'a> {
    db: &'a DatabaseConnectionType,
    tenant: &'a Tenant,
    excel_loader: ExcelLoader,
    pub patient_uuid_map: HashMap<String, Uuid>, // external_id -> patient UUID
    config: EtlMappingConfig,
    notes_created: usize,
    free_text_purger: Option<PersonalInfoPurger>,
    free_text_redactions: usize,
    redaction_categories: BTreeSet<String>,
}

/// Represents aggregated content for a specific category
#[derive(Debug, Clone)]
pub struct AggregatedContent {
    pub category: String,
    pub content: String,
    pub source_file: String,
    pub record_count: usize,
}

/// Configuration for Excel file processing
#[derive(Debug, Clone)]
pub struct ExcelFileConfig {
    pub filename: String,
    pub category: String,
    pub content_columns: Vec<String>,
}

impl<'a> PatientAggregator<'a> {
    pub fn new(
        db: &'a DatabaseConnectionType,
        tenant: &'a Tenant,
        config: EtlMappingConfig,
    ) -> Self {
        Self {
            db,
            tenant,
            excel_loader: ExcelLoader::new(),
            patient_uuid_map: HashMap::new(),
            config,
            notes_created: 0,
            free_text_purger: None,
            free_text_redactions: 0,
            redaction_categories: BTreeSet::new(),
        }
    }

    pub fn enable_free_text_pii_purging(&mut self, name_dictionary: Option<&Path>) -> Result<()> {
        self.free_text_purger = Some(PersonalInfoPurger::new(name_dictionary)?);
        Ok(())
    }

    /// Process all Excel files using the 3-Step Batch Discovery Pattern
    /// Step A: Comprehensive patient discovery from ALL files
    /// Step B: High-performance batch UPSERT with server-generated UUIDs
    /// Step C: Detail enrichment using guaranteed patient UUID mapping
    pub async fn process_excel_files(&mut self, input_directory: &str) -> Result<()> {
        println!("📊 Starting patient data processing...");

        // Step A: Discover all unique patient IDs and their demographics from ALL files
        println!("🔍 Discovering patients across all files...");
        let patient_demographics = self
            .discover_all_patients_and_demographics(input_directory)
            .await?;
        println!("   Found {} unique patients", patient_demographics.len());

        // Step B: Batch create/update all patients with server-generated UUIDs
        println!("⚡ Creating patient records...");
        self.batch_upsert_patients(patient_demographics).await?;

        // Step C: Process and aggregate notes using guaranteed patient UUIDs
        println!("📝 Processing medical notes...");
        self.enrich_patient_details(input_directory).await?;

        println!(
            "✅ Patient processing complete: {} patients, {} notes",
            self.patient_uuid_map.len(),
            self.notes_created
        );

        Ok(())
    }

    /// Step A: Discover all unique patient IDs and their demographics from ALL Excel files
    /// Returns HashMap<external_id, (age, sex)> with comprehensive patient discovery
    async fn discover_all_patients_and_demographics(
        &mut self,
        input_directory: &str,
    ) -> Result<HashMap<String, (Option<i32>, Option<String>)>> {
        let mut patient_demographics = HashMap::new();

        let demographics = self.load_demographics_subset(input_directory).await?;
        patient_demographics.extend(demographics);

        // Then, discover all patient IDs from ALL other files
        let file_configs = self.get_all_file_configurations();

        for config in &file_configs {
            let patient_ids = self
                .discover_patients_from_file(input_directory, config)
                .await?;
            for patient_id in patient_ids {
                patient_demographics
                    .entry(patient_id)
                    .or_insert((None, None));
            }
        }
        Ok(patient_demographics)
    }

    /// Load demographics data from the demographics subset file
    async fn load_demographics_subset(
        &mut self,
        input_directory: &str,
    ) -> Result<HashMap<String, (Option<i32>, Option<String>)>> {
        let file_path = format!(
            "{}/{}",
            input_directory, self.config.files.patient_demographics
        );
        let source = DataSource::Excel(file_path.clone().into(), None);

        // Load patient demographics data
        let records = self.excel_loader.load(&source)?;

        if records.is_empty() {
            return Err(PlatformError::invalid_input(format!(
                "No demographic records found in {}",
                file_path
            )));
        }

        // Detect columns for patient ID, age, and sex
        let patient_id_column = self.detect_patient_id_column(&records[0])?;
        let age_column = self.detect_age_column(&records[0]);
        let sex_column = self.detect_sex_column(&records[0]);

        // println!("   Demographics columns: ID={}, Age={:?}, Sex={:?}",
        //          patient_id_column, age_column, sex_column);

        let mut demographics = HashMap::new();

        for record in records {
            if let Some(patient_id) = record.get(&patient_id_column) {
                if !patient_id.trim().is_empty() {
                    let age = age_column
                        .as_ref()
                        .and_then(|col| record.get(col))
                        .and_then(|age_str| self.parse_age(age_str));

                    let sex = sex_column
                        .as_ref()
                        .and_then(|col| record.get(col))
                        .and_then(|sex_str| self.normalize_sex(sex_str));

                    demographics.insert(patient_id.trim().to_string(), (age, sex));
                }
            }
        }

        Ok(demographics)
    }

    /// Discover patient IDs from a specific file
    async fn discover_patients_from_file(
        &mut self,
        input_directory: &str,
        config: &ExcelFileConfig,
    ) -> Result<HashSet<String>> {
        let file_path = format!("{}/{}", input_directory, config.filename);
        let source = DataSource::Excel(file_path.clone().into(), None);

        let records = self.excel_loader.load(&source)?;
        let mut patient_ids = HashSet::new();

        if records.is_empty() {
            return Ok(patient_ids);
        }

        // Detect patient ID column for this file
        let patient_id_column = self.detect_patient_id_column(&records[0])?;

        for record in records {
            if let Some(patient_id) = record.get(&patient_id_column) {
                if !patient_id.trim().is_empty() {
                    patient_ids.insert(patient_id.trim().to_string());
                }
            }
        }

        Ok(patient_ids)
    }

    /// Get all file configurations for comprehensive discovery
    fn get_all_file_configurations(&self) -> Vec<ExcelFileConfig> {
        let mut configurations = self.get_excel_file_configurations();
        if let Some(journal) = &self.config.files.clinical_journal {
            configurations.push(ExcelFileConfig {
                filename: journal.filename.clone(),
                category: "clinical_journal".to_string(),
                content_columns: journal.content_columns.clone(),
            });
        }
        configurations
    }

    /// Step B: High-performance batch UPSERT with server-generated UUIDs using platform-db
    async fn batch_upsert_patients(
        &mut self,
        patient_demographics: HashMap<String, (Option<i32>, Option<String>)>,
    ) -> Result<()> {
        if patient_demographics.is_empty() {
            println!("   No patients to create");
            return Ok(());
        }

        // Convert to the format expected by platform-db: (external_id, age, sex)
        let patients: Vec<(String, Option<i32>, Option<String>)> = patient_demographics
            .into_iter()
            .map(|(external_id, (age, sex))| (external_id, age, sex))
            .collect();

        println!("   Batch upserting {} patients...", patients.len());

        // Use the high-performance batch_upsert_patients from platform-db
        self.patient_uuid_map = self
            .db
            .batch_upsert_patients(&patients, self.tenant.id)
            .await?;

        println!(
            "   ✅ Successfully created/updated {} patients",
            self.patient_uuid_map.len()
        );
        Ok(())
    }

    /// Step C: Process and aggregate notes using guaranteed patient UUIDs
    async fn enrich_patient_details(&mut self, input_directory: &str) -> Result<()> {
        let file_configs = self.get_excel_file_configurations();

        for config in file_configs {
            self.process_categorized_notes_file(input_directory, &config)
                .await?;
        }

        Ok(())
    }

    fn detect_patient_id_column(&self, record: &IndexMap<String, String>) -> Result<String> {
        // Use the configuration patterns for patient ID detection
        let id_patterns = &self.config.columns.patient_id_patterns;

        for pattern in id_patterns {
            if record.contains_key(pattern) {
                return Ok(pattern.to_string());
            }
        }

        if self.config.processing.enable_column_detection {
            for key in record.keys() {
                if id_patterns
                    .iter()
                    .any(|pattern| key.eq_ignore_ascii_case(pattern))
                {
                    return Ok(key.clone());
                }
            }
        }

        Err(PlatformError::invalid_input(
            "Could not detect patient ID column in Excel file",
        ))
    }

    fn detect_age_column(&self, record: &IndexMap<String, String>) -> Option<String> {
        // Try exact matches first
        for pattern in &self.config.columns.age_patterns {
            if record.contains_key(pattern) {
                return Some(pattern.clone());
            }
        }

        // Try partial matches if exact matches fail
        if self.config.processing.enable_column_detection {
            for key in record.keys() {
                let key_lower = key.to_lowercase();
                for pattern in &self.config.columns.age_patterns {
                    let pattern_lower = pattern.to_lowercase();
                    if key_lower.contains(&pattern_lower) {
                        return Some(key.clone());
                    }
                }
            }
        }

        None
    }

    fn detect_sex_column(&self, record: &IndexMap<String, String>) -> Option<String> {
        for pattern in &self.config.columns.sex_patterns {
            if record.contains_key(pattern) {
                return Some(pattern.clone());
            }
        }

        if self.config.processing.enable_column_detection {
            for key in record.keys() {
                if self
                    .config
                    .columns
                    .sex_patterns
                    .iter()
                    .any(|pattern| key.eq_ignore_ascii_case(pattern))
                {
                    return Some(key.clone());
                }
            }
        }

        None
    }

    fn parse_age(&self, age_str: &str) -> Option<i32> {
        let (minimum, maximum) = self.config.processing.age_range;
        age_str
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|&age| (minimum..=maximum).contains(&age))
    }

    fn normalize_sex(&self, sex_str: &str) -> Option<String> {
        let input = sex_str.trim();
        self.config
            .processing
            .sex_value_mapping
            .iter()
            .find(|(source, _)| source.eq_ignore_ascii_case(input))
            .map(|(_, normalized)| normalized.clone())
            .filter(|normalized| !normalized.trim().is_empty())
    }

    fn get_excel_file_configurations(&self) -> Vec<ExcelFileConfig> {
        let mut configs = Vec::new();

        // Build configurations from the mapping config
        for (category, mapping) in &self.config.files.medical_notes {
            let config = ExcelFileConfig {
                filename: mapping.filename.clone(),
                category: category.clone(),
                content_columns: mapping.content_columns.clone(),
            };
            configs.push(config);
        }

        configs
    }

    async fn process_categorized_notes_file(
        &mut self,
        input_directory: &str,
        config: &ExcelFileConfig,
    ) -> Result<()> {
        let file_path = format!("{}/{}", input_directory, config.filename);
        let source = DataSource::Excel(file_path.clone().into(), None);

        println!(
            "📝 Processing {} notes from: {}",
            config.category, file_path
        );

        // Load raw data
        let records = self.excel_loader.load(&source)?;

        if records.is_empty() {
            // println!("   ⚠️  No records found in {}", config.filename);
            return Ok(());
        }

        // Detect the actual patient ID column in this file
        let patient_id_column = self.detect_patient_id_column(&records[0])?;
        println!("   Patient ID column detected: {}", patient_id_column);

        // Debug: Show all columns in this file
        let all_columns: Vec<String> = records[0].keys().cloned().collect();
        println!("   All columns in file: {:?}", all_columns);

        // Process each record and create aggregated notes
        let content_columns = config.content_columns.clone();
        println!("   Looking for content columns: {:?}", content_columns);
        if !content_columns
            .iter()
            .any(|column| records[0].contains_key(column))
        {
            return Err(PlatformError::invalid_input(format!(
                "None of the mapped content columns {:?} exist in {}",
                content_columns, config.filename
            )));
        }

        let mut notes_created = 0;
        let mut content_aggregation: HashMap<String, Vec<String>> = HashMap::new();

        for record in records {
            if let Some(patient_external_id) = record.get(&patient_id_column) {
                if !patient_external_id.trim().is_empty() {
                    // Get patient UUID from our mapping
                    if let Some(_patient_uuid) =
                        self.patient_uuid_map.get(patient_external_id.trim())
                    {
                        // Collect content from all configured content columns
                        let mut record_content = Vec::new();

                        for content_col in &content_columns {
                            if let Some(content) = record.get(content_col) {
                                if !content.trim().is_empty() {
                                    record_content.push(
                                        self.sanitize_free_text_value(content, Some(&record)),
                                    );
                                }
                            }
                        }

                        // If we found content, add it to aggregation
                        if !record_content.is_empty() {
                            let combined_content = record_content.join(" | ");
                            content_aggregation
                                .entry(patient_external_id.trim().to_string())
                                .or_default()
                                .push(combined_content);
                        }
                    }
                }
            }
        }

        // Create aggregated notes for each patient
        for (patient_external_id, content_list) in content_aggregation {
            if let Some(&patient_uuid) = self.patient_uuid_map.get(&patient_external_id) {
                let aggregated_content =
                    self.sanitize_free_text_value(&content_list.join("\n\n"), None);

                // Create or update patient note using platform-db UPSERT API
                self.db
                    .upsert_patient_note(
                        patient_uuid,
                        self.tenant.id,
                        &config.category,
                        &aggregated_content,
                    )
                    .await?;
                notes_created += 1;
                self.notes_created += 1;
            }
        }

        println!(
            "   ✅ Created {} aggregated notes for category: {}",
            notes_created, config.category
        );
        if self.free_text_redactions > 0 {
            println!(
                "   🔒 Free-text redactions so far: {} ({})",
                self.free_text_redactions,
                self.redaction_categories
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(())
    }

    fn sanitize_free_text_value(
        &mut self,
        value: &str,
        record: Option<&IndexMap<String, String>>,
    ) -> String {
        let Some(purger) = self.free_text_purger.as_ref() else {
            return value.trim().to_string();
        };

        let result = match record {
            Some(record) => purger.sanitize_free_text_with_record(value, record),
            None => purger.sanitize_free_text(value, &[]),
        };
        self.record_redaction_stats(&result.stats);
        result.sanitized_text.trim().to_string()
    }

    fn record_redaction_stats(&mut self, stats: &TextRedactionStats) {
        if stats.total_redactions == 0 {
            return;
        }

        self.free_text_redactions += stats.total_redactions;
        self.redaction_categories.extend(
            stats
                .categories_hit
                .iter()
                .map(|category| category.label().to_string()),
        );
    }

    /// Get processing statistics for ETL reporting
    pub fn get_processing_stats(&self) -> ProcessingStats {
        ProcessingStats {
            patients_processed: self.patient_uuid_map.len(),
            notes_created: self.notes_created,
            redactions_applied: self.free_text_redactions,
            processing_time_seconds: 0.0, // Will be tracked when timing is added
        }
    }
}

/// Processing statistics for ETL reporting
#[derive(Debug, Default)]
pub struct ProcessingStats {
    pub patients_processed: usize,
    pub notes_created: usize,
    pub redactions_applied: usize,
    pub processing_time_seconds: f64,
}
