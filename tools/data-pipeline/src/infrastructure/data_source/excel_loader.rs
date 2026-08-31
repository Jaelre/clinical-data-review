use super::{DataAnalysis, DataLoader, DataSource, FormatInfo};
use crate::analysis::{ColumnAnalyzer, ColumnStats};
use calamine::{open_workbook, DataType, Range, Reader, Xlsx};
use indexmap::IndexMap;
use platform_errors::{PlatformError, Result};
use std::path::Path;

pub struct ExcelLoader {
    analyzer: ColumnAnalyzer,
}

impl Default for ExcelLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ExcelLoader {
    pub fn new() -> Self {
        Self {
            analyzer: ColumnAnalyzer::new(),
        }
    }

    /// Get all sheet names from Excel file
    pub fn get_sheet_names<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
        let workbook: Xlsx<_> = open_workbook(path)
            .map_err(|e| PlatformError::data_access(format!("Failed to open Excel file: {}", e)))?;

        Ok(workbook.sheet_names().to_vec())
    }

    /// Auto-detect the primary data sheet (largest non-empty sheet)
    pub fn detect_primary_sheet<P: AsRef<Path>>(path: P) -> Result<String> {
        let sheet_names = Self::get_sheet_names(&path)?;

        if sheet_names.is_empty() {
            return Err(PlatformError::invalid_input(
                "Excel file contains no sheets".to_string(),
            ));
        }

        let mut workbook: Xlsx<_> = open_workbook(path)
            .map_err(|e| PlatformError::data_access(format!("Failed to open Excel file: {}", e)))?;

        let mut best_sheet = sheet_names[0].clone();
        let mut max_rows = 0;

        for sheet_name in &sheet_names {
            if let Some(Ok(range)) = workbook.worksheet_range(sheet_name) {
                let row_count = range.get_size().0;
                if row_count > max_rows {
                    max_rows = row_count;
                    best_sheet = sheet_name.clone();
                }
            }
        }

        Ok(best_sheet)
    }

    /// Load specific sheet from Excel file
    pub fn load_sheet<P: AsRef<Path>>(
        path: P,
        sheet_name: &str,
    ) -> Result<Vec<IndexMap<String, String>>> {
        let mut workbook: Xlsx<_> = open_workbook(path)
            .map_err(|e| PlatformError::data_access(format!("Failed to open Excel file: {}", e)))?;

        let range = workbook
            .worksheet_range(sheet_name)
            .ok_or_else(|| {
                PlatformError::invalid_input(format!("Sheet '{}' not found", sheet_name))
            })?
            .map_err(|e| {
                PlatformError::data_access(format!("Failed to read sheet '{}': {}", sheet_name, e))
            })?;

        Self::range_to_records(&range)
    }

    /// Convert calamine Range to our universal format
    fn range_to_records(range: &Range<DataType>) -> Result<Vec<IndexMap<String, String>>> {
        let mut records = Vec::new();
        let (rows, cols) = range.get_size();

        if rows < 2 {
            return Ok(records); // No data or header only
        }

        // Extract headers from first row
        let mut headers = Vec::new();
        for col in 0..cols {
            if let Some(cell) = range.get((0, col)) {
                let header = Self::cell_to_string(cell);
                headers.push(if header.is_empty() {
                    format!("Column_{}", col + 1) // Auto-name empty headers
                } else {
                    header
                });
            } else {
                headers.push(format!("Column_{}", col + 1));
            }
        }

        // Convert data rows
        for row in 1..rows {
            let mut record = IndexMap::new();

            for (col, header) in headers.iter().enumerate() {
                let value = if let Some(cell) = range.get((row, col)) {
                    Self::cell_to_string(cell)
                } else {
                    String::new()
                };
                record.insert(header.clone(), value);
            }

            // Skip completely empty rows
            if record.values().any(|v| !v.trim().is_empty()) {
                records.push(record);
            }
        }

        Ok(records)
    }

    /// Convert Excel cell to string with type preservation awareness
    fn cell_to_string(cell: &DataType) -> String {
        match cell {
            DataType::Empty => String::new(),
            DataType::String(s) => s.clone(),
            DataType::Float(f) => {
                // Preserve integer appearance for whole numbers
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    (*f as i64).to_string()
                } else {
                    f.to_string()
                }
            }
            DataType::Int(i) => i.to_string(),
            DataType::Bool(b) => b.to_string(),
            DataType::DateTime(dt) => {
                // Format as ISO date string
                format!("{:.0}", dt)
            }
            DataType::Error(e) => format!("#ERROR: {:?}", e),
            DataType::DateTimeIso(dt) => dt.clone(),
            DataType::DurationIso(d) => d.clone(),
            DataType::Duration(d) => format!("{}s", d),
        }
    }

    /// Calculate medical data confidence for Excel content
    fn calculate_medical_score(&self, columns: &[ColumnStats]) -> f64 {
        let mut score = 0.0_f64;

        for col in columns {
            let name_lower = col.name.to_lowercase();

            // Medical indicators
            if name_lower.contains("patient") || name_lower.contains("id") {
                score += 20.0;
            }
            if name_lower.contains("age") || name_lower.contains("years") {
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
            if name_lower.contains("medication") || name_lower.contains("drug") {
                score += 20.0;
            }
        }

        score.min(100.0)
    }

    /// Suggest primary keys based on Excel data analysis
    fn suggest_primary_keys(&self, columns: &[ColumnStats]) -> Vec<String> {
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
            if name_lower.contains("code") {
                score += 70;
            }

            // Uniqueness bonus
            if col.uniqueness_ratio > 0.9 {
                score += 50;
            } else if col.uniqueness_ratio > 0.7 {
                score += 30;
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

    /// Generate processing suggestions for Excel data
    fn generate_suggestions(&self, analysis: &DataAnalysis) -> Vec<String> {
        let mut suggestions = Vec::new();

        if let FormatInfo::Excel { sheet_names, .. } = &analysis.format_info {
            if sheet_names.len() > 1 {
                suggestions.push(format!(
                    "📊 Multiple sheets detected ({}). Consider processing each separately.",
                    sheet_names.len()
                ));
            }
        }

        if analysis.medical_data_score > 50.0 {
            suggestions
                .push("🏥 Medical data detected - consider PII purging before sharing".to_string());
        }

        if !analysis.suggested_primary_keys.is_empty() {
            suggestions.push(format!(
                "🔑 Primary key candidates: {}",
                analysis.suggested_primary_keys.join(", ")
            ));
        }

        if analysis.column_analysis.len() > 20 {
            suggestions.push(
                "📊 Large dataset - consider filtering by date ranges or patient subsets"
                    .to_string(),
            );
        }

        // Type preservation suggestions
        let date_columns: Vec<&str> = analysis
            .column_analysis
            .iter()
            .filter(|col| {
                col.name.to_lowercase().contains("date") || col.name.to_lowercase().contains("data")
            })
            .map(|col| col.name.as_str())
            .collect();

        if !date_columns.is_empty() {
            suggestions.push(format!(
                "📅 Date columns preserved from Excel: {}",
                date_columns.join(", ")
            ));
        }

        suggestions
    }
}

impl DataLoader for ExcelLoader {
    fn load(&self, source: &DataSource) -> Result<Vec<IndexMap<String, String>>> {
        match source {
            DataSource::Excel(path, sheet_opt) => {
                let sheet_name = if let Some(sheet) = sheet_opt {
                    sheet.clone()
                } else {
                    Self::detect_primary_sheet(path)?
                };

                Self::load_sheet(path, &sheet_name)
            }
            _ => Err(PlatformError::invalid_input(format!(
                "ExcelLoader does not support source type: {}",
                source.format()
            ))),
        }
    }

    fn analyze(&self, source: &DataSource) -> Result<DataAnalysis> {
        match source {
            DataSource::Excel(path, sheet_opt) => {
                let sheet_names = Self::get_sheet_names(path)?;
                let active_sheet = if let Some(sheet) = sheet_opt {
                    sheet.clone()
                } else {
                    Self::detect_primary_sheet(path)?
                };

                // Load sample data for analysis
                let data = Self::load_sheet(path, &active_sheet)?;

                let format_info = FormatInfo::Excel {
                    sheet_names,
                    active_sheet,
                    has_header: !data.is_empty(),
                    row_count: data.len(),
                    column_count: if data.is_empty() { 0 } else { data[0].len() },
                };

                let column_analysis = if !data.is_empty() {
                    self.analyzer
                        .analyze_file(&data)
                        .map_err(|e| PlatformError::data_access(format!("Analysis error: {}", e)))?
                } else {
                    Vec::new()
                };

                let medical_data_score = self.calculate_medical_score(&column_analysis);
                let suggested_primary_keys = self.suggest_primary_keys(&column_analysis);

                let mut analysis = DataAnalysis {
                    source: source.clone(),
                    format_info,
                    column_analysis,
                    suggested_primary_keys,
                    medical_data_score,
                    suggestions: Vec::new(),
                };

                analysis.suggestions = self.generate_suggestions(&analysis);

                Ok(analysis)
            }
            _ => Err(PlatformError::invalid_input(format!(
                "ExcelLoader does not support source type: {}",
                source.format()
            ))),
        }
    }

    fn supports(&self, source: &DataSource) -> bool {
        matches!(source, DataSource::Excel(_, _))
    }

    fn name(&self) -> &'static str {
        "ExcelLoader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_to_string() {
        assert_eq!(
            ExcelLoader::cell_to_string(&DataType::String("test".to_string())),
            "test"
        );
        assert_eq!(ExcelLoader::cell_to_string(&DataType::Float(42.0)), "42");
        assert_eq!(ExcelLoader::cell_to_string(&DataType::Float(42.5)), "42.5");
        assert_eq!(ExcelLoader::cell_to_string(&DataType::Int(100)), "100");
        assert_eq!(ExcelLoader::cell_to_string(&DataType::Bool(true)), "true");
        assert_eq!(ExcelLoader::cell_to_string(&DataType::Empty), "");
    }

    #[test]
    fn test_supports() {
        let loader = ExcelLoader::new();
        let excel_source = DataSource::Excel("test.xlsx".into(), None);
        let csv_source = DataSource::Csv("test.csv".into());

        assert!(loader.supports(&excel_source));
        assert!(!loader.supports(&csv_source));
    }

    #[test]
    fn test_loader_name() {
        let loader = ExcelLoader::new();
        assert_eq!(loader.name(), "ExcelLoader");
    }
}
