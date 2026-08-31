use platform_errors::{PlatformError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path};

/// Explicit file and column mapping for one ETL source directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EtlMappingConfig {
    pub files: FileMapping,
    pub columns: ColumnMapping,
    pub processing: ProcessingOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileMapping {
    pub patient_demographics: String,
    pub medical_notes: BTreeMap<String, MedicalNoteMapping>,
    pub clinical_journal: Option<ClinicalJournalMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MedicalNoteMapping {
    pub filename: String,
    pub content_columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClinicalJournalMapping {
    pub filename: String,
    pub timestamp_columns: Vec<String>,
    pub content_columns: Vec<String>,
    #[serde(default)]
    pub role_columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnMapping {
    pub patient_id_patterns: Vec<String>,
    pub age_patterns: Vec<String>,
    pub sex_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessingOptions {
    pub sex_value_mapping: HashMap<String, String>,
    pub age_range: (i32, i32),
    pub default_tenant_name: String,
    #[serde(default)]
    pub enable_column_detection: bool,
}

impl EtlMappingConfig {
    /// Load and validate an explicit TOML mapping. There is deliberately no default profile.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|error| {
            PlatformError::config(format!(
                "Could not read ETL mapping `{}`: {error}",
                path.display()
            ))
        })?;
        let config: Self = toml::from_str(&content).map_err(|error| {
            PlatformError::config(format!(
                "Could not parse ETL mapping `{}`: {error}",
                path.display()
            ))
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        Self::validate_relative_filename(
            "files.patient_demographics",
            &self.files.patient_demographics,
        )?;
        Self::validate_patterns(
            "columns.patient_id_patterns",
            &self.columns.patient_id_patterns,
        )?;
        Self::validate_patterns("columns.age_patterns", &self.columns.age_patterns)?;
        Self::validate_patterns("columns.sex_patterns", &self.columns.sex_patterns)?;

        if self.files.medical_notes.is_empty() {
            return Err(PlatformError::config_key(
                "At least one medical-note mapping is required",
                "files.medical_notes",
            ));
        }
        for (category, mapping) in &self.files.medical_notes {
            if category.trim().is_empty() {
                return Err(PlatformError::config_key(
                    "Medical-note category names cannot be blank",
                    "files.medical_notes",
                ));
            }
            Self::validate_relative_filename(
                &format!("files.medical_notes.{category}.filename"),
                &mapping.filename,
            )?;
            Self::validate_patterns(
                &format!("files.medical_notes.{category}.content_columns"),
                &mapping.content_columns,
            )?;
        }

        if let Some(journal) = &self.files.clinical_journal {
            Self::validate_relative_filename("files.clinical_journal.filename", &journal.filename)?;
            Self::validate_patterns(
                "files.clinical_journal.timestamp_columns",
                &journal.timestamp_columns,
            )?;
            Self::validate_patterns(
                "files.clinical_journal.content_columns",
                &journal.content_columns,
            )?;
        }

        let (minimum_age, maximum_age) = self.processing.age_range;
        if minimum_age < 0 || minimum_age > maximum_age {
            return Err(PlatformError::config_key(
                "age_range must contain a non-negative minimum no greater than its maximum",
                "processing.age_range",
            ));
        }
        if self.processing.default_tenant_name.trim().is_empty() {
            return Err(PlatformError::config_key(
                "Default workspace name cannot be blank",
                "processing.default_tenant_name",
            ));
        }
        if self.processing.sex_value_mapping.is_empty() {
            return Err(PlatformError::config_key(
                "At least one sex-value mapping is required",
                "processing.sex_value_mapping",
            ));
        }
        if self
            .processing
            .sex_value_mapping
            .keys()
            .any(|value| value.trim().is_empty())
        {
            return Err(PlatformError::config_key(
                "Sex-value mapping keys cannot be blank",
                "processing.sex_value_mapping",
            ));
        }

        Ok(())
    }

    fn validate_patterns(key: &str, values: &[String]) -> Result<()> {
        if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
            return Err(PlatformError::config_key(
                "Mapping list must contain only non-blank values",
                key,
            ));
        }
        Ok(())
    }

    fn validate_relative_filename(key: &str, value: &str) -> Result<()> {
        let path = Path::new(value);
        let is_safe = !value.trim().is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
        if !is_safe {
            return Err(PlatformError::config_key(
                "Mapped files must use non-empty relative paths without parent traversal",
                key,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> EtlMappingConfig {
        EtlMappingConfig {
            files: FileMapping {
                patient_demographics: "demographics.xlsx".to_string(),
                medical_notes: BTreeMap::from([(
                    "notes".to_string(),
                    MedicalNoteMapping {
                        filename: "notes.xlsx".to_string(),
                        content_columns: vec!["content".to_string()],
                    },
                )]),
                clinical_journal: Some(ClinicalJournalMapping {
                    filename: "journal.xlsx".to_string(),
                    timestamp_columns: vec!["recorded_at".to_string()],
                    content_columns: vec!["entry".to_string()],
                    role_columns: vec!["author_role".to_string()],
                }),
            },
            columns: ColumnMapping {
                patient_id_patterns: vec!["patient_id".to_string()],
                age_patterns: vec!["age_years".to_string()],
                sex_patterns: vec!["sex".to_string()],
            },
            processing: ProcessingOptions {
                sex_value_mapping: HashMap::from([
                    ("female".to_string(), "F".to_string()),
                    ("male".to_string(), "M".to_string()),
                ]),
                age_range: (0, 120),
                default_tenant_name: "Example Research Workspace".to_string(),
                enable_column_detection: false,
            },
        }
    }

    #[test]
    fn valid_explicit_config_is_accepted() {
        valid_config().validate().unwrap();
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let mut config = valid_config();
        config.files.patient_demographics = "../private.xlsx".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn malformed_file_reports_context() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mapping.toml");
        std::fs::write(&path, "[files\n").unwrap();
        let error = EtlMappingConfig::from_file(&path).unwrap_err().to_string();
        assert!(error.contains("Could not parse ETL mapping"));
        assert!(error.contains("mapping.toml"));
    }
}
