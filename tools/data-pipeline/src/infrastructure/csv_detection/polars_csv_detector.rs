use crate::analysis::{ColumnAnalyzer, ColumnStats};
use indexmap::IndexMap;
use platform_errors::{PlatformError, Result};
use polars::prelude::*;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CsvDetectionResult {
    pub separator: u8,
    pub has_header: bool,
    pub quote_char: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct EnhancedCsvAnalysis {
    pub detection_result: CsvDetectionResult,
    pub column_analysis: Vec<ColumnStats>,
    pub suggested_primary_keys: Vec<String>,
    pub medical_data_score: f64,
}

pub struct PolarsCsvDetector;

impl PolarsCsvDetector {
    /// Load CSV file using Polars with automatic format detection
    pub fn load_csv_robust<P: AsRef<Path>>(path: P) -> Result<Vec<IndexMap<String, String>>> {
        let df = Self::load_dataframe(&path)?;
        Self::dataframe_to_indexmap(&df)
    }

    /// Load CSV file as Polars DataFrame with smart detection
    pub fn load_dataframe<P: AsRef<Path>>(path: P) -> Result<DataFrame> {
        // Try different separators until one works
        let separators = [b',', b';', b'\t', b'|'];
        let mut last_error = None;

        for &separator in &separators {
            match Self::try_load_with_separator(&path, separator) {
                Ok(df) => {
                    if df.height() > 0 && df.width() > 0 {
                        return Ok(df);
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            PlatformError::invalid_input("Could not parse CSV with any separator".to_string())
        }))
    }

    fn try_load_with_separator<P: AsRef<Path>>(path: P, separator: u8) -> Result<DataFrame> {
        let parse_options =
            polars::io::csv::read::CsvParseOptions::default().with_separator(separator);

        let options = CsvReadOptions::default()
            .with_has_header(true)
            .with_ignore_errors(false)
            .with_parse_options(parse_options);

        let df = options
            .try_into_reader_with_file_path(Some(path.as_ref().to_path_buf()))
            .map_err(|e| PlatformError::invalid_input(format!("Polars error: {}", e)))?
            .finish()
            .map_err(|e| PlatformError::invalid_input(format!("Polars error: {}", e)))?;

        Ok(df)
    }

    /// Convert a Polars DataFrame to row-oriented records.
    pub fn dataframe_to_indexmap(df: &DataFrame) -> Result<Vec<IndexMap<String, String>>> {
        let mut records = Vec::new();
        let height = df.height();
        let column_names: Vec<String> = df
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        for row_idx in 0..height {
            let mut record = IndexMap::new();

            for col_name in &column_names {
                if let Ok(column) = df.column(col_name) {
                    let value = column
                        .get(row_idx)
                        .map(|v| {
                            let s = v.to_string();
                            // Clean up quotes and null representations
                            if s == "null" || s == "NULL" {
                                String::new()
                            } else {
                                s.trim_matches('"').to_string()
                            }
                        })
                        .unwrap_or_else(|_| String::new());
                    record.insert(col_name.clone(), value);
                }
            }

            records.push(record);
        }

        Ok(records)
    }

    /// Detect CSV format and provide analysis
    pub fn detect_format<P: AsRef<Path>>(path: P) -> Result<CsvDetectionResult> {
        // Try to detect separator by loading with different separators
        let separators = [b',', b';', b'\t', b'|'];

        for &separator in &separators {
            if let Ok(df) = Self::try_load_with_separator(&path, separator) {
                if df.height() > 0 && df.width() > 1 {
                    return Ok(CsvDetectionResult {
                        separator,
                        has_header: true, // Assume header for medical data
                        quote_char: Some(b'"'),
                    });
                }
            }
        }

        Err(PlatformError::invalid_input(
            "Could not detect a valid delimited CSV format",
        ))
    }

    /// Enhanced CSV analysis with medical data scoring
    pub fn analyze_csv<P: AsRef<Path>>(path: P) -> Result<EnhancedCsvAnalysis> {
        let detection_result = Self::detect_format(&path)?;
        let df = Self::load_dataframe(&path)?;
        let data = Self::dataframe_to_indexmap(&df)?;

        if data.is_empty() {
            return Ok(EnhancedCsvAnalysis {
                detection_result,
                column_analysis: Vec::new(),
                suggested_primary_keys: Vec::new(),
                medical_data_score: 0.0,
            });
        }

        let analyzer = ColumnAnalyzer::new();
        let column_analysis = analyzer
            .analyze_file(&data)
            .map_err(|e| PlatformError::data_access(format!("Analysis error: {}", e)))?;

        // Score medical data likelihood
        let medical_data_score = Self::calculate_medical_score(&column_analysis);

        // Suggest primary keys based on analysis
        let suggested_primary_keys = Self::suggest_primary_keys(&column_analysis);

        Ok(EnhancedCsvAnalysis {
            detection_result,
            column_analysis,
            suggested_primary_keys,
            medical_data_score,
        })
    }

    fn calculate_medical_score(columns: &[ColumnStats]) -> f64 {
        let mut score = 0.0_f64;

        for col in columns {
            let name_lower = col.name.to_lowercase();

            // Medical indicators
            if name_lower.contains("patient") || name_lower.contains("id") {
                score += 20.0;
            }
            if name_lower.contains("age") {
                score += 15.0;
            }
            if name_lower.contains("sex") || name_lower.contains("gender") {
                score += 15.0;
            }
            if name_lower.contains("date") || name_lower.contains("time") {
                score += 10.0;
            }
            if name_lower.contains("diagnosis") || name_lower.contains("icd") {
                score += 25.0;
            }
        }

        score.min(100.0)
    }

    fn suggest_primary_keys(columns: &[ColumnStats]) -> Vec<String> {
        let mut candidates = Vec::new();

        for col in columns {
            let name_lower = col.name.to_lowercase();
            let mut score = 0;

            // ID-like patterns
            if name_lower.contains("id") && !name_lower.contains("void") {
                score += 100;
            }
            if name_lower.contains("key") {
                score += 90;
            }
            if name_lower.contains("protocol") {
                score += 85;
            }

            if score > 50 {
                candidates.push((col.name.clone(), score));
            }
        }

        // Sort by score and return top candidates
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates
            .into_iter()
            .map(|(name, _)| name)
            .take(3)
            .collect()
    }

    /// Get processing suggestions based on analysis
    pub fn get_processing_suggestions(analysis: &EnhancedCsvAnalysis) -> Vec<String> {
        let mut suggestions = Vec::new();

        if analysis.medical_data_score > 50.0 {
            suggestions.push("🏥 Medical data detected - consider PII purging".to_string());
        }

        if !analysis.suggested_primary_keys.is_empty() {
            suggestions.push(format!(
                "🔑 Primary key candidates: {}",
                analysis.suggested_primary_keys.join(", ")
            ));
        }

        if analysis.column_analysis.len() > 10 {
            suggestions.push("📊 Large dataset - consider filtering before processing".to_string());
        }

        suggestions
    }

    /// Test if Polars is working correctly
    pub fn test_availability() -> bool {
        match df! [
            "test" => [1, 2, 3],
            "name" => ["a", "b", "c"],
        ] {
            Ok(_) => {
                println!("✅ Polars CSV processing is available and working");
                true
            }
            Err(e) => {
                println!("❌ Polars error: {}", e);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_polars_availability() {
        assert!(PolarsCsvDetector::test_availability());
    }

    #[test]
    fn test_csv_loading() {
        let csv_content = "patient_id,age,sex,diagnosis\n1,25,M,A00.1\n2,30,F,B01.2";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();

        let data = PolarsCsvDetector::load_csv_robust(temp_file.path()).unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["patient_id"], "1");
        assert_eq!(data[0]["age"], "25");
    }

    #[test]
    fn test_medical_scoring() {
        let csv_content = "patient_id,age,sex,diagnosis\n1,25,M,A00.1\n2,30,F,B01.2";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();

        let analysis = PolarsCsvDetector::analyze_csv(temp_file.path()).unwrap();
        assert!(analysis.medical_data_score > 50.0);
        assert!(!analysis.suggested_primary_keys.is_empty());
    }

    #[test]
    fn test_separator_detection() {
        let csv_content = "id;name;value\n1;Alice;100\n2;Bob;200";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();

        let result = PolarsCsvDetector::detect_format(temp_file.path()).unwrap();
        assert_eq!(result.separator, b';');
    }
}
