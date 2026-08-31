use anyhow::Result;
use indexmap::IndexMap;
use regex::Regex;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    Integer,
    Float,
    String,
    Date,
    Boolean,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ColumnPurpose {
    PrimaryKey,   // High uniqueness, structured format
    ForeignKey,   // Structured, references another table
    PersonalName, // Names, potentially PII
    Age,          // Numeric age data
    Gender,       // Gender/sex information
    DateTime,     // Date/time stamps
    MedicalCode,  // Medical coding systems
    FreeText,     // Unstructured text
    Measurement,  // Numeric measurements
    Category,     // Categorical data
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ColumnStats {
    pub name: String,
    pub data_type: DataType,
    pub purpose: ColumnPurpose,
    pub total_rows: usize,
    pub non_empty_rows: usize,
    pub unique_values: usize,
    pub uniqueness_ratio: f64,
    pub sample_values: Vec<String>,
    pub confidence_score: f64,
}

#[derive(Debug, Clone)]
pub struct IdCandidate {
    pub column_name: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub data_type: DataType,
    pub format_pattern: Option<String>,
}

pub struct ColumnAnalyzer {
    id_patterns: Vec<Regex>,
    name_patterns: Vec<Regex>,
    age_patterns: Vec<Regex>,
    gender_patterns: Vec<Regex>,
    date_patterns: Vec<Regex>,
}

impl Default for ColumnAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ColumnAnalyzer {
    pub fn new() -> Self {
        Self {
            // ID-like patterns (language agnostic)
            id_patterns: vec![
                Regex::new(r"(?i)^[a-z_]*id[a-z_]*$").unwrap(),
                Regex::new(r"(?i)^[a-z_]*key[a-z_]*$").unwrap(),
                Regex::new(r"(?i)^[a-z_]*code[a-z_]*$").unwrap(),
                Regex::new(r"(?i)^[a-z_]*num[a-z_]*$").unwrap(),
                Regex::new(r"(?i)^[a-z_]*ref[a-z_]*$").unwrap(),
                Regex::new(r"(?i).*identifier.*").unwrap(),
                Regex::new(r"(?i).*patient.*").unwrap(),
            ],
            // Name patterns
            name_patterns: vec![
                Regex::new(r"(?i).*name.*").unwrap(),
                Regex::new(r"(?i).*firstname.*").unwrap(),
                Regex::new(r"(?i).*lastname.*").unwrap(),
                Regex::new(r"(?i).*surname.*").unwrap(),
            ],
            // Age patterns
            age_patterns: vec![
                Regex::new(r"(?i)^age$").unwrap(),
                Regex::new(r"(?i).*years.*").unwrap(),
                Regex::new(r"(?i).*birth.*").unwrap(),
            ],
            // Gender patterns
            gender_patterns: vec![
                Regex::new(r"(?i)^sex$").unwrap(),
                Regex::new(r"(?i)^gender$").unwrap(),
                Regex::new(r"(?i).*male.*").unwrap(),
                Regex::new(r"(?i).*female.*").unwrap(),
            ],
            // Date patterns
            date_patterns: vec![
                Regex::new(r"(?i).*date.*").unwrap(),
                Regex::new(r"(?i).*time.*").unwrap(),
                Regex::new(r"(?i).*when.*").unwrap(),
                Regex::new(r"(?i).*created.*").unwrap(),
                Regex::new(r"(?i).*modified.*").unwrap(),
            ],
        }
    }

    pub fn analyze_file(&self, data: &[IndexMap<String, String>]) -> Result<Vec<ColumnStats>> {
        if data.is_empty() {
            return Ok(vec![]);
        }

        let mut stats = Vec::new();
        let header = &data[0];

        for column_name in header.keys() {
            let column_stats = self.analyze_column(column_name, data)?;
            stats.push(column_stats);
        }

        Ok(stats)
    }

    pub fn analyze_column(
        &self,
        column_name: &str,
        data: &[IndexMap<String, String>],
    ) -> Result<ColumnStats> {
        let values: Vec<String> = data
            .iter()
            .filter_map(|row| row.get(column_name))
            .filter(|val| !val.trim().is_empty())
            .cloned()
            .collect();

        let total_rows = data.len();
        let non_empty_rows = values.len();
        let unique_values: HashSet<_> = values.iter().collect();
        let unique_count = unique_values.len();
        let uniqueness_ratio = if non_empty_rows > 0 {
            unique_count as f64 / non_empty_rows as f64
        } else {
            0.0
        };

        let data_type = self.infer_data_type(&values);
        let purpose = self.infer_column_purpose(column_name, &values, &data_type, uniqueness_ratio);
        let confidence_score =
            self.calculate_confidence_score(&purpose, uniqueness_ratio, non_empty_rows);

        // Sample values for display (first few unique values)
        let sample_values: Vec<String> = unique_values.into_iter().take(5).cloned().collect();

        Ok(ColumnStats {
            name: column_name.to_string(),
            data_type,
            purpose,
            total_rows,
            non_empty_rows,
            unique_values: unique_count,
            uniqueness_ratio,
            sample_values,
            confidence_score,
        })
    }

    fn infer_data_type(&self, values: &[String]) -> DataType {
        if values.is_empty() {
            return DataType::String;
        }

        let mut type_counts = HashMap::new();

        for value in values.iter().take(100) {
            // Sample first 100 values
            let trimmed = value.trim();

            if self.is_integer(trimmed) {
                *type_counts.entry(DataType::Integer).or_insert(0) += 1;
            } else if self.is_float(trimmed) {
                *type_counts.entry(DataType::Float).or_insert(0) += 1;
            } else if self.is_date(trimmed) {
                *type_counts.entry(DataType::Date).or_insert(0) += 1;
            } else if self.is_boolean(trimmed) {
                *type_counts.entry(DataType::Boolean).or_insert(0) += 1;
            } else {
                *type_counts.entry(DataType::String).or_insert(0) += 1;
            }
        }

        // If more than 80% of values are the same type, use that type
        let sample_size = values.len().min(100);
        let threshold = (sample_size as f64 * 0.8) as i32;

        for (data_type, count) in type_counts {
            if count >= threshold {
                return data_type;
            }
        }

        DataType::Mixed
    }

    fn infer_column_purpose(
        &self,
        column_name: &str,
        values: &[String],
        data_type: &DataType,
        uniqueness_ratio: f64,
    ) -> ColumnPurpose {
        // ID detection based on name patterns and uniqueness
        if self.matches_patterns(column_name, &self.id_patterns) {
            if uniqueness_ratio > 0.8 {
                return ColumnPurpose::PrimaryKey;
            } else if uniqueness_ratio > 0.5 {
                return ColumnPurpose::ForeignKey;
            }
        }

        // Name detection
        if self.matches_patterns(column_name, &self.name_patterns) {
            return ColumnPurpose::PersonalName;
        }

        // Age detection
        if self.matches_patterns(column_name, &self.age_patterns)
            && matches!(data_type, DataType::Integer | DataType::Float)
        {
            return ColumnPurpose::Age;
        }

        // Gender detection
        if self.matches_patterns(column_name, &self.gender_patterns) {
            return ColumnPurpose::Gender;
        }

        // Date detection
        if self.matches_patterns(column_name, &self.date_patterns) || *data_type == DataType::Date {
            return ColumnPurpose::DateTime;
        }

        // Content-based detection
        if !values.is_empty() {
            // Check for medical codes (alphanumeric patterns)
            if self.looks_like_medical_codes(values) {
                return ColumnPurpose::MedicalCode;
            }

            // Check for measurements (numeric with possible units)
            if self.looks_like_measurements(values) {
                return ColumnPurpose::Measurement;
            }

            // Check for long text (likely free text)
            if self.looks_like_free_text(values) {
                return ColumnPurpose::FreeText;
            }

            // Check for categorical data (low cardinality strings)
            if uniqueness_ratio < 0.1 && values.len() > 10 {
                return ColumnPurpose::Category;
            }
        }

        ColumnPurpose::Unknown
    }

    pub fn find_id_candidates(&self, stats: &[ColumnStats]) -> Vec<IdCandidate> {
        let mut candidates = Vec::new();

        for stat in stats {
            let mut score = 0.0;
            let mut reasons = Vec::new();

            // Uniqueness score (most important factor)
            if stat.uniqueness_ratio > 0.9 {
                score += 50.0;
                reasons.push("Very high uniqueness (>90%)".to_string());
            } else if stat.uniqueness_ratio > 0.7 {
                score += 30.0;
                reasons.push("High uniqueness (>70%)".to_string());
            } else if stat.uniqueness_ratio > 0.5 {
                score += 15.0;
                reasons.push("Moderate uniqueness (>50%)".to_string());
            }

            // Name pattern matching
            if self.matches_patterns(&stat.name, &self.id_patterns) {
                score += 25.0;
                reasons.push("ID-like column name".to_string());
            }

            // Data type appropriateness
            match stat.data_type {
                DataType::Integer => {
                    score += 15.0;
                    reasons.push("Integer data type".to_string());
                }
                DataType::String => {
                    if self.looks_like_structured_ids(&stat.sample_values) {
                        score += 10.0;
                        reasons.push("Structured string format".to_string());
                    }
                }
                _ => {}
            }

            // Purpose detection
            if stat.purpose == ColumnPurpose::PrimaryKey {
                score += 20.0;
                reasons.push("Detected as primary key".to_string());
            } else if stat.purpose == ColumnPurpose::ForeignKey {
                score += 15.0;
                reasons.push("Detected as foreign key".to_string());
            }

            // Coverage bonus
            let coverage = stat.non_empty_rows as f64 / stat.total_rows as f64;
            if coverage > 0.95 {
                score += 10.0;
                reasons.push("High data coverage".to_string());
            }

            // Only include candidates with reasonable scores
            if score > 10.0 {
                let format_pattern = self.detect_format_pattern(&stat.sample_values);

                candidates.push(IdCandidate {
                    column_name: stat.name.clone(),
                    score,
                    reasons,
                    data_type: stat.data_type.clone(),
                    format_pattern,
                });
            }
        }

        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        candidates
    }

    // Helper methods
    fn matches_patterns(&self, text: &str, patterns: &[Regex]) -> bool {
        patterns.iter().any(|pattern| pattern.is_match(text))
    }

    fn is_integer(&self, s: &str) -> bool {
        s.parse::<i64>().is_ok()
    }

    fn is_float(&self, s: &str) -> bool {
        s.parse::<f64>().is_ok()
    }

    fn is_date(&self, s: &str) -> bool {
        // Basic date pattern detection
        let date_patterns = [
            r"\d{1,2}/\d{1,2}/\d{4}",
            r"\d{4}-\d{1,2}-\d{1,2}",
            r"\d{1,2}-\d{1,2}-\d{4}",
            r"\d{1,2}\.\d{1,2}\.\d{4}",
        ];

        date_patterns
            .iter()
            .any(|pattern| Regex::new(pattern).unwrap().is_match(s))
    }

    fn is_boolean(&self, s: &str) -> bool {
        matches!(
            s.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "si" | "y" | "n" | "1" | "0"
        )
    }

    fn looks_like_medical_codes(&self, values: &[String]) -> bool {
        if values.is_empty() {
            return false;
        }

        // Check if values look like medical codes (alphanumeric, structured)
        let sample_size = values.len().min(20);
        let structured_count = values
            .iter()
            .take(sample_size)
            .filter(|v| {
                let v = v.trim();
                // Medical codes often have specific patterns
                v.len() >= 3
                    && v.len() <= 20
                    && (v.chars().any(|c| c.is_ascii_digit())
                        && v.chars().any(|c| c.is_ascii_alphabetic()))
            })
            .count();

        structured_count as f64 / sample_size as f64 > 0.6
    }

    fn looks_like_measurements(&self, values: &[String]) -> bool {
        if values.is_empty() {
            return false;
        }

        let sample_size = values.len().min(20);
        let numeric_count = values
            .iter()
            .take(sample_size)
            .filter(|v| {
                let v = v.trim();
                // Check for numbers possibly with units
                self.is_float(v) || Regex::new(r"^\d+\.?\d*\s*[a-zA-Z]*$").unwrap().is_match(v)
            })
            .count();

        numeric_count as f64 / sample_size as f64 > 0.7
    }

    fn looks_like_free_text(&self, values: &[String]) -> bool {
        if values.is_empty() {
            return false;
        }

        let avg_length: f64 =
            values.iter().map(|v| v.len()).sum::<usize>() as f64 / values.len() as f64;

        // Free text tends to be longer and contain spaces
        avg_length > 50.0 && values.iter().any(|v| v.contains(' '))
    }

    fn looks_like_structured_ids(&self, values: &[String]) -> bool {
        if values.is_empty() {
            return false;
        }

        // Check if string IDs have consistent patterns
        let first_len = values[0].len();
        let consistent_length = values.iter().take(10).all(|v| v.len() == first_len);

        consistent_length && first_len >= 3
    }

    fn detect_format_pattern(&self, values: &[String]) -> Option<String> {
        if values.is_empty() {
            return None;
        }

        let first = &values[0];

        // Try to detect common ID patterns
        if Regex::new(r"^\d+$").unwrap().is_match(first) {
            Some("Numeric ID".to_string())
        } else if Regex::new(r"^[A-Z]+\d+$").unwrap().is_match(first) {
            Some("Alpha-numeric (letters+digits)".to_string())
        } else if Regex::new(r"^\d{4}\d+$").unwrap().is_match(first) {
            Some("Year-prefixed ID".to_string())
        } else if first.contains('-') || first.contains('_') {
            Some("Delimited ID".to_string())
        } else {
            None
        }
    }

    fn calculate_confidence_score(
        &self,
        purpose: &ColumnPurpose,
        uniqueness_ratio: f64,
        sample_size: usize,
    ) -> f64 {
        let mut confidence: f64 = 50.0; // Base confidence

        // Adjust based on purpose certainty
        match purpose {
            ColumnPurpose::PrimaryKey | ColumnPurpose::ForeignKey => confidence += 30.0,
            ColumnPurpose::PersonalName | ColumnPurpose::Age | ColumnPurpose::Gender => {
                confidence += 20.0
            }
            ColumnPurpose::DateTime | ColumnPurpose::MedicalCode => confidence += 15.0,
            ColumnPurpose::Unknown => confidence -= 20.0,
            _ => {}
        }

        // Adjust based on data quality
        if uniqueness_ratio > 0.8 {
            confidence += 15.0;
        } else if uniqueness_ratio < 0.1 {
            confidence -= 10.0;
        }

        if sample_size > 100 {
            confidence += 10.0;
        } else if sample_size < 10 {
            confidence -= 15.0;
        }

        confidence.clamp(0.0, 100.0)
    }
}
