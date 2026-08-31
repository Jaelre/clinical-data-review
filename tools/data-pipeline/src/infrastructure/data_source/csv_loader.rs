use super::{DataAnalysis, DataLoader, DataSource, FormatInfo};
use crate::analysis::ColumnAnalyzer;
use crate::infrastructure::csv_detection::PolarsCsvDetector;
use indexmap::IndexMap;
use platform_errors::{PlatformError, Result};

/// CSV loader backed by the Polars detector.
pub struct CsvLoader {
    #[allow(dead_code)]
    analyzer: ColumnAnalyzer,
}

impl CsvLoader {
    pub fn new() -> Self {
        Self {
            analyzer: ColumnAnalyzer::new(),
        }
    }

    /// Convert PolarsCsvDetector analysis to new format
    fn convert_analysis(
        source: &DataSource,
        polars_analysis: crate::infrastructure::csv_detection::EnhancedCsvAnalysis,
    ) -> DataAnalysis {
        let format_info = FormatInfo::Csv {
            separator: polars_analysis.detection_result.separator,
            has_header: polars_analysis.detection_result.has_header,
            quote_char: polars_analysis.detection_result.quote_char,
            encoding: "UTF-8".to_string(), // Default encoding
        };

        DataAnalysis {
            source: source.clone(),
            format_info,
            column_analysis: polars_analysis.column_analysis,
            suggested_primary_keys: polars_analysis.suggested_primary_keys,
            medical_data_score: polars_analysis.medical_data_score,
            suggestions: Vec::new(), // Will be populated by generate_suggestions
        }
    }

    /// Generate CSV-specific processing suggestions
    fn generate_suggestions(&self, analysis: &DataAnalysis) -> Vec<String> {
        let mut suggestions = Vec::new();

        if let FormatInfo::Csv { separator, .. } = &analysis.format_info {
            let sep_char = *separator as char;
            if sep_char != ',' {
                suggestions.push(format!(
                    "📄 Non-standard separator detected: '{}'",
                    sep_char
                ));
            }
        }

        if analysis.medical_data_score > 50.0 {
            suggestions.push(
                "🏥 Medical CSV data detected - consider PII purging before sharing".to_string(),
            );
        }

        if !analysis.suggested_primary_keys.is_empty() {
            suggestions.push(format!(
                "🔑 Primary key candidates: {}",
                analysis.suggested_primary_keys.join(", ")
            ));
        }

        if analysis.column_analysis.len() > 15 {
            suggestions.push(
                "📊 Wide CSV dataset - consider column filtering for processing efficiency"
                    .to_string(),
            );
        }

        // Check for potential data quality issues
        let empty_columns: Vec<&str> = analysis
            .column_analysis
            .iter()
            .filter(|col| col.uniqueness_ratio < 0.1)
            .map(|col| col.name.as_str())
            .collect();

        if !empty_columns.is_empty() && empty_columns.len() < 5 {
            suggestions.push(format!(
                "⚠️ Mostly empty columns detected: {}",
                empty_columns.join(", ")
            ));
        }

        // Check for potential Excel export artifacts
        let artifact_indicators: Vec<&str> = analysis
            .column_analysis
            .iter()
            .filter(|col| col.name.starts_with("Column_") || col.name.is_empty())
            .map(|col| col.name.as_str())
            .collect();

        if !artifact_indicators.is_empty() {
            suggestions.push(
                "📄 Possible Excel export artifacts detected - consider using original Excel file"
                    .to_string(),
            );
        }

        suggestions
    }
}

impl DataLoader for CsvLoader {
    fn load(&self, source: &DataSource) -> Result<Vec<IndexMap<String, String>>> {
        match source {
            DataSource::Csv(path) => PolarsCsvDetector::load_csv_robust(path),
            _ => Err(PlatformError::invalid_input(format!(
                "CsvLoader does not support source type: {}",
                source.format()
            ))),
        }
    }

    fn analyze(&self, source: &DataSource) -> Result<DataAnalysis> {
        match source {
            DataSource::Csv(path) => {
                let polars_analysis = PolarsCsvDetector::analyze_csv(path)?;
                let mut analysis = Self::convert_analysis(source, polars_analysis);
                analysis.suggestions = self.generate_suggestions(&analysis);
                Ok(analysis)
            }
            _ => Err(PlatformError::invalid_input(format!(
                "CsvLoader does not support source type: {}",
                source.format()
            ))),
        }
    }

    fn supports(&self, source: &DataSource) -> bool {
        matches!(source, DataSource::Csv(_))
    }

    fn name(&self) -> &'static str {
        "CsvLoader"
    }
}

impl Default for CsvLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_supports() {
        let loader = CsvLoader::new();
        let csv_source = DataSource::Csv("test.csv".into());
        let excel_source = DataSource::Excel("test.xlsx".into(), None);

        assert!(loader.supports(&csv_source));
        assert!(!loader.supports(&excel_source));
    }

    #[test]
    fn test_loader_name() {
        let loader = CsvLoader::new();
        assert_eq!(loader.name(), "CsvLoader");
    }

    #[test]
    fn test_csv_loading() {
        let csv_content = "patient_id,age,sex\n1,25,M\n2,30,F";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();

        let loader = CsvLoader::new();
        let source = DataSource::Csv(temp_file.path().to_path_buf());

        let data = loader.load(&source).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["patient_id"], "1");
        assert_eq!(data[0]["age"], "25");
        assert_eq!(data[1]["sex"], "F");
    }

    #[test]
    fn test_csv_analysis() {
        let csv_content = "patient_id,age,sex\n1,25,M\n2,30,F";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();

        let loader = CsvLoader::new();
        let source = DataSource::Csv(temp_file.path().to_path_buf());

        let analysis = loader.analyze(&source).unwrap();
        assert!(!analysis.column_analysis.is_empty());
        assert!(analysis.medical_data_score > 0.0);

        if let FormatInfo::Csv {
            separator,
            has_header,
            ..
        } = analysis.format_info
        {
            assert_eq!(separator, b',');
            assert!(has_header);
        } else {
            panic!("Expected CSV format info");
        }
    }
}
