use super::etl_config::ClinicalJournalMapping;
use crate::domains::purging::{PersonalInfoPurger, TextRedactionStats};
use crate::infrastructure::data_source::{DataLoader, DataSource, ExcelLoader};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use indexmap::IndexMap;
use platform_db::{DatabaseConnection, DatabaseConnectionType};
use platform_errors::{PlatformError, Result};
use platform_models::Tenant;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use uuid::Uuid;

/// Processes clinical journal data with chronological sorting and sequencing
pub struct ClinicalJournalProcessor<'a> {
    db: &'a DatabaseConnectionType,
    tenant: &'a Tenant,
    excel_loader: ExcelLoader,
    patient_uuid_map: &'a HashMap<String, Uuid>, // external_id -> patient UUID
    mapping: ClinicalJournalMapping,
    patient_id_patterns: Vec<String>,
    free_text_purger: Option<PersonalInfoPurger>,
}

/// Represents a clinical journal entry before database insertion
#[derive(Debug, Clone)]
pub struct JournalEntryData {
    pub patient_external_id: String,
    pub timestamp: DateTime<Utc>,
    pub role: Option<String>,
    pub content: String,
    pub raw_timestamp: String, // For debugging parsing issues
}

#[derive(Debug, Default, Clone)]
pub struct ClinicalJournalProcessingStats {
    pub entries_created: usize,
    pub redactions_applied: usize,
    pub redaction_categories: BTreeSet<String>,
}

impl<'a> ClinicalJournalProcessor<'a> {
    pub fn new(
        db: &'a DatabaseConnectionType,
        tenant: &'a Tenant,
        patient_uuid_map: &'a HashMap<String, Uuid>,
        mapping: ClinicalJournalMapping,
        patient_id_patterns: Vec<String>,
    ) -> Self {
        Self {
            db,
            tenant,
            excel_loader: ExcelLoader::new(),
            patient_uuid_map,
            mapping,
            patient_id_patterns,
            free_text_purger: None,
        }
    }

    pub fn enable_free_text_pii_purging(&mut self, name_dictionary: Option<&Path>) -> Result<()> {
        self.free_text_purger = Some(PersonalInfoPurger::new(name_dictionary)?);
        Ok(())
    }

    /// Process clinical journal Excel file with chronological sorting
    pub async fn process_clinical_journal_file(
        &self,
        input_directory: &str,
    ) -> Result<ClinicalJournalProcessingStats> {
        let file_path = format!("{}/{}", input_directory, self.mapping.filename);
        let source = DataSource::Excel(file_path.clone().into(), None);

        println!("📋 Processing clinical journal...");

        // Load raw journal data
        let records = self.excel_loader.load(&source)?;

        if records.is_empty() {
            // println!("   ⚠️  No clinical journal entries found");
            return Ok(ClinicalJournalProcessingStats::default());
        }

        // Parse and validate journal entries
        let parsed_journal = self.parse_journal_entries(&records)?;
        let journal_entries = parsed_journal.entries;

        if journal_entries.is_empty() {
            // println!("   ⚠️  No valid clinical journal entries found after parsing");
            return Ok(ClinicalJournalProcessingStats {
                redactions_applied: parsed_journal.redactions_applied,
                redaction_categories: parsed_journal.redaction_categories,
                ..ClinicalJournalProcessingStats::default()
            });
        }

        // Group by patient and sort chronologically
        let sorted_entries_by_patient = self.group_and_sort_entries(journal_entries)?;

        // Insert entries with sequential numbering per patient
        let mut total_entries_created = 0;
        for (patient_external_id, sorted_entries) in sorted_entries_by_patient {
            if let Some(&patient_uuid) = self.patient_uuid_map.get(&patient_external_id) {
                let entries_created = self
                    .create_journal_entries_for_patient(patient_uuid, sorted_entries)
                    .await?;
                total_entries_created += entries_created;
            } else {
                return Err(PlatformError::not_found("patient", patient_external_id));
            }
        }

        println!(
            "✅ Clinical journal complete: {} entries",
            total_entries_created
        );

        Ok(ClinicalJournalProcessingStats {
            entries_created: total_entries_created,
            redactions_applied: parsed_journal.redactions_applied,
            redaction_categories: parsed_journal.redaction_categories,
        })
    }

    /// Parse raw Excel records into structured journal entries
    fn parse_journal_entries(
        &self,
        records: &[IndexMap<String, String>],
    ) -> Result<ParsedJournalEntries> {
        let mut journal_entries = Vec::new();

        // Detect column names
        let patient_id_column = self.detect_patient_id_column(&records[0])?;
        let timestamp_column = self.detect_timestamp_column(&records[0])?;
        let role_column = self.detect_role_column(&records[0]);
        let content_column = self.detect_content_column(&records[0])?;

        println!("   Column mapping:");
        println!("     Patient ID: {}", patient_id_column);
        println!("     Timestamp: {}", timestamp_column);
        if let Some(ref role_col) = role_column {
            println!("     Role: {}", role_col);
        }
        println!("     Content: {}", content_column);

        let mut parsed_entries = 0;
        let mut redaction_stats = TextRedactionStats::default();

        for (row_index, record) in records.iter().enumerate() {
            // Extract patient ID
            let patient_id = match record.get(&patient_id_column) {
                Some(id) if !id.trim().is_empty() => id.trim().to_string(),
                _ => {
                    return Err(PlatformError::invalid_input(format!(
                        "Clinical journal row {} has no patient identifier",
                        row_index + 2
                    )));
                }
            };

            // Extract and parse timestamp
            let raw_timestamp = match record.get(&timestamp_column) {
                Some(ts) if !ts.trim().is_empty() => ts.trim().to_string(),
                _ => {
                    return Err(PlatformError::invalid_input(format!(
                        "Clinical journal row {} has no timestamp",
                        row_index + 2
                    )));
                }
            };

            let timestamp = self.parse_timestamp(&raw_timestamp).map_err(|error| {
                PlatformError::invalid_input(format!(
                    "Clinical journal row {} has invalid timestamp `{}`: {error}",
                    row_index + 2,
                    raw_timestamp
                ))
            })?;

            // Extract role (optional)
            let role = role_column
                .as_ref()
                .and_then(|col| record.get(col))
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty());

            // Extract content
            let content = match record.get(&content_column) {
                Some(content) if !content.trim().is_empty() => content.trim().to_string(),
                _ => {
                    return Err(PlatformError::invalid_input(format!(
                        "Clinical journal row {} has no content",
                        row_index + 2
                    )));
                }
            };
            let content = self.sanitize_free_text_value(&content, record, &mut redaction_stats);

            journal_entries.push(JournalEntryData {
                patient_external_id: patient_id,
                timestamp,
                role,
                content,
                raw_timestamp,
            });

            parsed_entries += 1;
        }

        println!("   ✅ Successfully parsed {} entries", parsed_entries);
        Ok(ParsedJournalEntries {
            entries: journal_entries,
            redactions_applied: redaction_stats.total_redactions,
            redaction_categories: redaction_stats
                .categories_hit
                .iter()
                .map(|category| category.label().to_string())
                .collect(),
        })
    }

    /// Group entries by patient and sort chronologically within each group
    fn group_and_sort_entries(
        &self,
        journal_entries: Vec<JournalEntryData>,
    ) -> Result<HashMap<String, Vec<JournalEntryData>>> {
        let mut entries_by_patient: HashMap<String, Vec<JournalEntryData>> = HashMap::new();

        // Group by patient
        for entry in journal_entries {
            entries_by_patient
                .entry(entry.patient_external_id.clone())
                .or_default()
                .push(entry);
        }

        // Sort each patient's entries chronologically
        for (_patient_id, entries) in entries_by_patient.iter_mut() {
            entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

            // println!("   📅 Patient {}: {} entries sorted chronologically",
            //     patient_id, entries.len());

            // Log timestamp range for verification
            // if let (Some(first), Some(last)) = (entries.first(), entries.last()) {
            //     println!("     Range: {} to {}",
            //         first.timestamp.format("%Y-%m-%d %H:%M"),
            //         last.timestamp.format("%Y-%m-%d %H:%M"));
            // }
        }

        Ok(entries_by_patient)
    }

    /// Create database entries for a single patient with sequential numbering
    async fn create_journal_entries_for_patient(
        &self,
        patient_uuid: Uuid,
        sorted_entries: Vec<JournalEntryData>,
    ) -> Result<usize> {
        let mut entries_created = 0;

        for (sequence, entry_data) in sorted_entries.iter().enumerate() {
            // Use platform-db API to create clinical journal entry
            self.db
                .create_clinical_journal_entry(
                    patient_uuid,
                    self.tenant.id,
                    entry_data.timestamp,
                    (sequence + 1) as i32, // 1-based sequencing
                    entry_data.role.as_deref(),
                    &entry_data.content,
                )
                .await?;
            entries_created += 1;
        }

        Ok(entries_created)
    }

    /// Parse timestamp with multiple format support
    fn parse_timestamp(&self, timestamp_str: &str) -> Result<DateTime<Utc>> {
        Self::parse_timestamp_value(timestamp_str)
    }

    fn parse_timestamp_value(timestamp_str: &str) -> Result<DateTime<Utc>> {
        let timestamp_str = timestamp_str.trim();

        // Try multiple date formats commonly found in Excel files
        let formats = vec![
            "%Y-%m-%d %H:%M:%S", // 2024-01-15 14:30:00
            "%Y-%m-%d %H:%M",    // 2024-01-15 14:30
            "%d/%m/%Y %H:%M:%S", // 15/01/2024 14:30:00
            "%d/%m/%Y %H:%M",    // 15/01/2024 14:30
            "%d-%m-%Y %H:%M:%S", // 15-01-2024 14:30:00
            "%d-%m-%Y %H:%M",    // 15-01-2024 14:30
            "%Y-%m-%d",          // 2024-01-15 (assume 00:00:00)
            "%d/%m/%Y",          // 15/01/2024 (assume 00:00:00)
            "%d-%m-%Y",          // 15-01-2024 (assume 00:00:00)
        ];

        // Try parsing with each format
        for format in &formats {
            if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, format) {
                return Ok(Utc.from_utc_datetime(&naive_dt));
            }
        }

        // Try parsing as Unix timestamp (if it's a number)
        if let Ok(timestamp_num) = timestamp_str.parse::<i64>() {
            if let Some(dt) = Utc.timestamp_opt(timestamp_num, 0).single() {
                return Ok(dt);
            }
        }

        Err(PlatformError::invalid_input(format!(
            "Unsupported timestamp `{timestamp_str}`"
        )))
    }

    /// Detect a configured patient ID column in journal records.
    fn detect_patient_id_column(&self, record: &IndexMap<String, String>) -> Result<String> {
        for pattern in &self.patient_id_patterns {
            if record.contains_key(pattern) {
                return Ok(pattern.clone());
            }
        }

        Err(PlatformError::invalid_input(
            "Could not detect patient ID column in clinical journal file",
        ))
    }

    /// Detect timestamp column in journal records
    fn detect_timestamp_column(&self, record: &IndexMap<String, String>) -> Result<String> {
        for pattern in &self.mapping.timestamp_columns {
            if record.contains_key(pattern) {
                return Ok(pattern.clone());
            }
        }

        Err(PlatformError::invalid_input(
            "Could not detect timestamp column in clinical journal file",
        ))
    }

    /// Detect role column (optional)
    fn detect_role_column(&self, record: &IndexMap<String, String>) -> Option<String> {
        for pattern in &self.mapping.role_columns {
            if record.contains_key(pattern) {
                return Some(pattern.clone());
            }
        }

        None
    }

    /// Detect content column
    fn detect_content_column(&self, record: &IndexMap<String, String>) -> Result<String> {
        for pattern in &self.mapping.content_columns {
            if record.contains_key(pattern) {
                return Ok(pattern.clone());
            }
        }

        Err(PlatformError::invalid_input(
            "Could not detect content column in clinical journal file",
        ))
    }

    /// Validate journal entries before insertion
    pub fn validate_journal_entries(&self, entries: &[JournalEntryData]) -> Result<()> {
        let mut validation_errors = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            if entry.patient_external_id.trim().is_empty() {
                validation_errors.push(format!("Entry {}: Empty patient ID", index));
            }

            if entry.content.trim().is_empty() {
                validation_errors.push(format!("Entry {}: Empty content", index));
            }

            // Validate timestamp is reasonable (not too far in future/past)
            let now = Utc::now();
            let hundred_years_ago = now - chrono::Duration::days(365 * 100);
            let one_year_future = now + chrono::Duration::days(365);

            if entry.timestamp < hundred_years_ago || entry.timestamp > one_year_future {
                validation_errors.push(format!(
                    "Entry {}: Timestamp {} seems unreasonable (parsed from '{}')",
                    index, entry.timestamp, entry.raw_timestamp
                ));
            }
        }

        if !validation_errors.is_empty() {
            return Err(PlatformError::invalid_input(format!(
                "Journal validation errors: {}",
                validation_errors.join(", ")
            )));
        }

        Ok(())
    }

    fn sanitize_free_text_value(
        &self,
        value: &str,
        record: &IndexMap<String, String>,
        aggregate_stats: &mut TextRedactionStats,
    ) -> String {
        let Some(purger) = self.free_text_purger.as_ref() else {
            return value.trim().to_string();
        };

        let result = purger.sanitize_free_text_with_record(value, record);
        aggregate_stats.merge(&result.stats);
        result.sanitized_text.trim().to_string()
    }
}

#[derive(Debug, Default)]
struct ParsedJournalEntries {
    entries: Vec<JournalEntryData>,
    redactions_applied: usize,
    redaction_categories: BTreeSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_timestamp_parsing() {
        // Test various timestamp formats
        let test_cases = vec![
            ("2024-01-15 14:30:00", true),
            ("15/01/2024 14:30", true),
            ("invalid_timestamp", false),
        ];

        for (input, should_succeed) in test_cases {
            let result = ClinicalJournalProcessor::parse_timestamp_value(input);
            if should_succeed {
                assert!(result.is_ok(), "Failed to parse valid timestamp: {}", input);
            } else {
                assert!(result.is_err(), "Invalid timestamp should fail: {}", input);
            }
        }
    }

    #[test]
    fn test_chronological_sorting() {
        let entries = vec![
            JournalEntryData {
                patient_external_id: "123".to_string(),
                timestamp: Utc
                    .with_ymd_and_hms(2024, 1, 15, 10, 0, 0)
                    .single()
                    .unwrap(),
                role: None,
                content: "Entry 2".to_string(),
                raw_timestamp: "2024-01-15 10:00:00".to_string(),
            },
            JournalEntryData {
                patient_external_id: "123".to_string(),
                timestamp: Utc
                    .with_ymd_and_hms(2024, 1, 14, 10, 0, 0)
                    .single()
                    .unwrap(),
                role: None,
                content: "Entry 1".to_string(),
                raw_timestamp: "2024-01-14 10:00:00".to_string(),
            },
        ];

        let mut sorted_entries = entries;
        sorted_entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        assert_eq!(sorted_entries[0].content, "Entry 1");
        assert_eq!(sorted_entries[1].content, "Entry 2");
    }
}
