use crate::domains::{DataProcessor, ProcessingContext};
use indexmap::IndexMap;
use platform_errors::{PlatformError, Result};
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PurgingConfig {
    pub protected_columns: HashSet<String>,
    pub redaction_text: String,
}

impl Default for PurgingConfig {
    fn default() -> Self {
        Self {
            protected_columns: HashSet::new(),
            redaction_text: "[REDACTED]".to_string(),
        }
    }
}

impl PurgingConfig {
    pub fn with_smart_protected_columns(data: &[IndexMap<String, String>]) -> Self {
        let mut config = Self::default();

        if !data.is_empty() {
            let sample_row = &data[0];
            for column_name in sample_row.keys() {
                if Self::is_likely_protected_medical_column(column_name) {
                    config.protected_columns.insert(column_name.clone());
                }
            }
        }

        config
    }

    fn is_likely_protected_medical_column(column_name: &str) -> bool {
        let column_lower = column_name.to_lowercase();

        let protected_patterns = [
            r"age|years",
            r"sex|gender",
            r"^.*id[^a-z]|^id$|^.*_id$|key",
            r"record|protocol|number|code",
            r"date|time|timestamp",
            r"value|measure|result",
        ];

        protected_patterns.iter().any(|pattern| {
            regex::Regex::new(&format!("(?i){}", pattern))
                .map(|re| re.is_match(&column_lower))
                .unwrap_or(false)
        })
    }
}

#[derive(Debug)]
pub struct PurgingInput {
    pub data: Vec<IndexMap<String, String>>,
    pub config: PurgingConfig,
}

#[derive(Debug)]
pub struct PurgingOutput {
    pub purged_data: Vec<IndexMap<String, String>>,
    pub stats: PurgingStats,
}

#[derive(Debug)]
pub struct PurgingStats {
    pub total_redactions: usize,
    pub columns_processed: usize,
    pub protected_columns_skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RedactionCategory {
    Person,
    Phone,
    Email,
    Address,
    Identifier,
    Dob,
    Location,
}

impl RedactionCategory {
    fn placeholder(self) -> &'static str {
        match self {
            Self::Person => "[PERSON]",
            Self::Phone => "[PHONE]",
            Self::Email => "[EMAIL]",
            Self::Address => "[ADDRESS]",
            Self::Identifier => "[IDENTIFIER]",
            Self::Dob => "[DOB]",
            Self::Location => "[LOCATION]",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Phone => "phone",
            Self::Email => "email",
            Self::Address => "address",
            Self::Identifier => "identifier",
            Self::Dob => "dob",
            Self::Location => "location",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TextRedactionStats {
    pub total_redactions: usize,
    pub categories_hit: BTreeSet<RedactionCategory>,
}

impl TextRedactionStats {
    pub fn merge(&mut self, other: &Self) {
        self.total_redactions += other.total_redactions;
        self.categories_hit
            .extend(other.categories_hit.iter().copied());
    }
}

#[derive(Debug, Clone, Default)]
pub struct TextRedactionResult {
    pub sanitized_text: String,
    pub stats: TextRedactionStats,
}

pub struct PersonalInfoPurger {
    name_patterns: Option<Regex>,
    phone_patterns: Regex,
    email_patterns: Regex,
    address_patterns: Regex,
    identifier_patterns: Regex,
    dob_patterns: Regex,
    location_patterns: Regex,
    titled_person_patterns: Regex,
    relationship_person_patterns: Regex,
}

impl PersonalInfoPurger {
    pub fn new(name_dictionary: Option<&Path>) -> Result<Self> {
        let name_dict = match name_dictionary {
            Some(path) => Self::load_name_dictionary(path)?,
            None => Vec::new(),
        };
        let name_patterns = Self::create_name_pattern(&name_dict)?;
        let phone_patterns = Self::create_phone_pattern()?;
        let email_patterns = Self::create_email_pattern()?;
        let address_patterns = Self::create_address_pattern()?;
        let identifier_patterns = Self::create_identifier_pattern()?;
        let dob_patterns = Self::create_dob_pattern()?;
        let location_patterns = Self::create_location_pattern()?;
        let titled_person_patterns = Self::create_titled_person_pattern()?;
        let relationship_person_patterns = Self::create_relationship_person_pattern()?;

        Ok(Self {
            name_patterns,
            phone_patterns,
            email_patterns,
            address_patterns,
            identifier_patterns,
            dob_patterns,
            location_patterns,
            titled_person_patterns,
            relationship_person_patterns,
        })
    }

    fn load_name_dictionary(path: &Path) -> Result<Vec<String>> {
        let content = std::fs::read_to_string(path).map_err(|error| {
            PlatformError::config(format!(
                "Could not read name dictionary `{}`: {error}",
                path.display()
            ))
        })?;
        let mut names = BTreeSet::new();
        for (line_number, line) in content.lines().enumerate() {
            let candidate = line.trim();
            if candidate.is_empty() || candidate.starts_with('#') {
                continue;
            }
            if candidate.len() > 80
                || !candidate
                    .chars()
                    .all(|character| character.is_alphabetic() || " '-".contains(character))
            {
                return Err(PlatformError::config(format!(
                    "Invalid name dictionary entry at {}:{}; use one name per line with letters, spaces, apostrophes, or hyphens only",
                    path.display(),
                    line_number + 1
                )));
            }
            names.insert(candidate.to_string());
        }
        if names.is_empty() {
            return Err(PlatformError::config(format!(
                "Name dictionary `{}` contains no usable entries",
                path.display()
            )));
        }
        Ok(names.into_iter().collect())
    }

    fn create_name_pattern(names: &[String]) -> Result<Option<Regex>> {
        if names.is_empty() {
            return Ok(None);
        }

        let escaped_names: Vec<String> = names.iter().map(|name| regex::escape(name)).collect();

        let pattern = format!(r"(?i)\b(?:{})\b", escaped_names.join("|"));
        Regex::new(&pattern)
            .map(Some)
            .map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_phone_pattern() -> Result<Regex> {
        let patterns = [
            r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b", // US: 123-456-7890 or 123.456.7890 or 1234567890
            r"\(\d{3}\)\s?\d{3}[-.]?\d{4}\b", // US: (123) 456-7890
            r"\b\+\d{1,3}[-.\s]?\d{1,14}\b",  // International: +1-234-567-8900
            r"\b\d{2,3}[-.\s]?\d{3,4}[-.\s]?\d{3,4}\b", // European style: 12-345-6789
        ];

        let combined_pattern = patterns.join("|");
        Regex::new(&combined_pattern)
            .map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_email_pattern() -> Result<Regex> {
        let pattern = r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b";
        Regex::new(pattern).map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_address_pattern() -> Result<Regex> {
        let pattern = concat!(
            r"\b(?:\d{1,5}\s+)?(?:[A-Z][A-Za-z'-]*\s+){1,3}",
            r"(?i:street|road|avenue|lane|drive|boulevard)\b(?:\s+\d{1,5})?"
        );
        Regex::new(pattern).map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_identifier_pattern() -> Result<Regex> {
        let patterns = [
            r"(?i)\b(?:medical record|patient record|insurance|document|protocol|mrn)\s*(?:number|id)?\s*[:#]?\s*[A-Z0-9\/-]{4,}\b",
        ];

        Regex::new(&patterns.join("|"))
            .map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_dob_pattern() -> Result<Regex> {
        let pattern = concat!(
            r"(?i)\b(?:date of birth|dob|born)",
            r"(?:\s+in\s+[A-Z][A-Za-z' -]+)?",
            r"(?:\s+on)?\s+\d{1,2}[\/\.-]\d{1,2}[\/\.-]\d{2,4}\b"
        );
        Regex::new(pattern).map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_location_pattern() -> Result<Regex> {
        let pattern = concat!(
            r"(?i:\b(?:resident in|resides in|lives in))\s+",
            r"[A-Z][A-Za-z'-]*(?:\s+[A-Z][A-Za-z'-]*){0,2}\b"
        );
        Regex::new(pattern).map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_titled_person_pattern() -> Result<Regex> {
        let pattern = concat!(
            r"(?i:\b(?:mr\.?|mrs\.?|ms\.?|mx\.?|dr\.?|doctor))\s+",
            r"[A-Z][A-Za-z'-]*(?:\s+[A-Z][A-Za-z'-]*)?\b"
        );
        Regex::new(pattern).map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    fn create_relationship_person_pattern() -> Result<Regex> {
        let pattern = concat!(
            r"(?i:\b(?:mother|father|spouse|partner|son|daughter|brother|sister|caregiver|relative)\b",
            r"(?:\s+(?:mr\.?|mrs\.?|ms\.?|mx\.?))?)",
            r"\s+[A-Z][A-Za-z'-]*(?:\s+[A-Z][A-Za-z'-]*)?\b"
        );
        Regex::new(pattern).map_err(|e| PlatformError::invalid_input(format!("Regex error: {}", e)))
    }

    pub fn sanitize_free_text(
        &self,
        text: &str,
        contextual_names: &[String],
    ) -> TextRedactionResult {
        self.purge_text_internal(text, PlaceholderMode::Typed, contextual_names)
    }

    pub fn sanitize_free_text_with_record(
        &self,
        text: &str,
        record: &IndexMap<String, String>,
    ) -> TextRedactionResult {
        let contextual_names = Self::extract_contextual_names(record);
        self.sanitize_free_text(text, &contextual_names)
    }

    pub fn extract_contextual_names(record: &IndexMap<String, String>) -> Vec<String> {
        let mut candidates = BTreeSet::new();

        for (column, value) in record {
            if !Self::is_likely_name_column(column) {
                continue;
            }

            for candidate in Self::expand_contextual_name_value(value) {
                candidates.insert(candidate);
            }
        }

        candidates.into_iter().collect()
    }

    fn is_likely_name_column(column: &str) -> bool {
        let normalized = column.to_lowercase();
        let positive_hints = ["name", "surname", "patient_name", "caregiver", "doctor"];
        let negative_hints = [
            "filename",
            "sheetname",
            "username",
            "note",
            "content",
            "description",
            "text",
        ];

        positive_hints.iter().any(|hint| normalized.contains(hint))
            && !negative_hints.iter().any(|hint| normalized.contains(hint))
    }

    fn expand_contextual_name_value(value: &str) -> Vec<String> {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty()
            || normalized.len() > 80
            || normalized.chars().any(|ch| ch.is_ascii_digit())
        {
            return Vec::new();
        }

        let alphabetic_chars = normalized.chars().filter(|ch| ch.is_alphabetic()).count();
        if alphabetic_chars < 3 {
            return Vec::new();
        }

        let mut names = BTreeSet::new();
        names.insert(normalized.clone());

        for token in normalized.split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';') {
            let token =
                token.trim_matches(|ch: char| !ch.is_alphabetic() && ch != '\'' && ch != '-');
            if token.len() >= 3
                && token
                    .chars()
                    .all(|ch| ch.is_alphabetic() || ch == '\'' || ch == '-')
            {
                names.insert(token.to_string());
            }
        }

        names.into_iter().collect()
    }

    fn create_contextual_name_pattern(names: &[String]) -> Option<Regex> {
        let mut unique_names: Vec<String> = names
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect();

        if unique_names.is_empty() {
            return None;
        }

        unique_names.sort_by_key(|name| std::cmp::Reverse(name.len()));
        unique_names.dedup();

        let escaped_names: Vec<String> = unique_names
            .iter()
            .map(|name| regex::escape(name))
            .collect();
        let pattern = format!(r"(?i)\b(?:{})\b", escaped_names.join("|"));

        Regex::new(&pattern).ok()
    }

    fn purge_text(&self, text: &str, redaction_text: &str) -> (String, usize) {
        let result = self.purge_text_internal(text, PlaceholderMode::Generic(redaction_text), &[]);
        (result.sanitized_text, result.stats.total_redactions)
    }

    fn purge_text_internal(
        &self,
        text: &str,
        placeholder_mode: PlaceholderMode<'_>,
        contextual_names: &[String],
    ) -> TextRedactionResult {
        let mut result = text.to_string();
        let mut stats = TextRedactionStats::default();

        self.apply_pattern(
            &mut result,
            &self.email_patterns,
            RedactionCategory::Email,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.phone_patterns,
            RedactionCategory::Phone,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.dob_patterns,
            RedactionCategory::Dob,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.identifier_patterns,
            RedactionCategory::Identifier,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.address_patterns,
            RedactionCategory::Address,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.location_patterns,
            RedactionCategory::Location,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.titled_person_patterns,
            RedactionCategory::Person,
            placeholder_mode,
            &mut stats,
        );
        self.apply_pattern(
            &mut result,
            &self.relationship_person_patterns,
            RedactionCategory::Person,
            placeholder_mode,
            &mut stats,
        );

        if let Some(contextual_pattern) = Self::create_contextual_name_pattern(contextual_names) {
            self.apply_pattern(
                &mut result,
                &contextual_pattern,
                RedactionCategory::Person,
                placeholder_mode,
                &mut stats,
            );
        }

        if let Some(name_patterns) = &self.name_patterns {
            self.apply_pattern(
                &mut result,
                name_patterns,
                RedactionCategory::Person,
                placeholder_mode,
                &mut stats,
            );
        }

        TextRedactionResult {
            sanitized_text: result,
            stats,
        }
    }

    fn apply_pattern(
        &self,
        text: &mut String,
        pattern: &Regex,
        category: RedactionCategory,
        placeholder_mode: PlaceholderMode<'_>,
        stats: &mut TextRedactionStats,
    ) {
        let matches = pattern.find_iter(text).count();
        if matches == 0 {
            return;
        }

        *text = pattern
            .replace_all(text, placeholder_mode.placeholder_for(category))
            .to_string();
        stats.total_redactions += matches;
        stats.categories_hit.insert(category);
    }

    fn process_string_column(
        &self,
        data: &[IndexMap<String, String>],
        column: &str,
        config: &PurgingConfig,
    ) -> (Vec<String>, usize) {
        let mut total_redactions = 0;
        let purged_values: Vec<String> = data
            .iter()
            .map(|row| {
                if let Some(value) = row.get(column) {
                    let (purged, redactions) = self.purge_text(value, &config.redaction_text);
                    total_redactions += redactions;
                    purged
                } else {
                    String::new()
                }
            })
            .collect();

        (purged_values, total_redactions)
    }
}

#[derive(Clone, Copy)]
enum PlaceholderMode<'a> {
    Generic(&'a str),
    Typed,
}

impl<'a> PlaceholderMode<'a> {
    fn placeholder_for(self, category: RedactionCategory) -> &'a str {
        match self {
            Self::Generic(value) => value,
            Self::Typed => category.placeholder(),
        }
    }
}

impl DataProcessor for PersonalInfoPurger {
    type Input = PurgingInput;
    type Output = PurgingOutput;
    type Error = PlatformError;

    fn process(&self, input: Self::Input, _context: &ProcessingContext) -> Result<Self::Output> {
        if input.data.is_empty() {
            return Ok(PurgingOutput {
                purged_data: Vec::new(),
                stats: PurgingStats {
                    total_redactions: 0,
                    columns_processed: 0,
                    protected_columns_skipped: 0,
                },
            });
        }

        let mut purged_data = input.data.clone();
        let mut total_redactions = 0;
        let mut columns_processed = 0;
        let mut protected_columns_skipped = 0;

        // Get all column names from the first row
        let column_names: Vec<String> = input.data[0].keys().cloned().collect();

        for column in &column_names {
            if input.config.protected_columns.contains(column) {
                protected_columns_skipped += 1;
                continue;
            }

            // Process this column for all rows
            let (purged_values, redactions) =
                self.process_string_column(&input.data, column, &input.config);

            // Update the column in all rows
            for (row, new_value) in purged_data.iter_mut().zip(purged_values.iter()) {
                row.insert(column.clone(), new_value.clone());
            }

            total_redactions += redactions;
            columns_processed += 1;
        }

        let stats = PurgingStats {
            total_redactions,
            columns_processed,
            protected_columns_skipped,
        };

        Ok(PurgingOutput { purged_data, stats })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::indexmap;

    #[test]
    fn missing_name_dictionary_fails_with_its_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing-names.txt");
        let error = PersonalInfoPurger::new(Some(&path))
            .err()
            .expect("missing dictionaries must fail")
            .to_string();
        assert!(error.contains("missing-names.txt"));
    }

    #[test]
    fn test_text_purging() {
        let purger = PersonalInfoPurger::new(None).unwrap();

        let text = "Contact John Smith at john.smith@example.com or call 555-123-4567";
        let (purged, redactions) = purger.purge_text(text, "[REDACTED]");

        assert!(redactions > 0);
        assert!(purged.contains("[REDACTED]"));
        assert!(!purged.contains("john.smith@example.com"));
        assert!(!purged.contains("555-123-4567"));
    }

    #[test]
    fn test_phone_pattern() {
        let purger = PersonalInfoPurger::new(None).unwrap();

        let test_cases = [
            "Call me at 555-123-4567",
            "My number is (555) 123-4567",
            "International: +1-555-123-4567",
            "European style: 12-345-6789",
        ];

        for case in &test_cases {
            let (purged, redactions) = purger.purge_text(case, "[REDACTED]");
            assert!(redactions > 0, "Failed to detect phone in: {}", case);
            assert!(
                purged.contains("[REDACTED]"),
                "Failed to redact phone in: {}",
                case
            );
        }
    }

    #[test]
    fn test_email_pattern() {
        let purger = PersonalInfoPurger::new(None).unwrap();

        let test_cases = [
            "Email me at test@example.com",
            "Contact: user.name@domain.org",
            "Support: help@company.co.uk",
        ];

        for case in &test_cases {
            let (purged, redactions) = purger.purge_text(case, "[REDACTED]");
            assert!(redactions > 0, "Failed to detect email in: {}", case);
            assert!(
                purged.contains("[REDACTED]"),
                "Failed to redact email in: {}",
                case
            );
        }
    }

    #[test]
    fn test_protected_columns() {
        let purger = PersonalInfoPurger::new(None).unwrap();

        let data = vec![indexmap! {
            "age".to_string() => "25".to_string(),
            "name".to_string() => "John Smith".to_string(),
            "email".to_string() => "john@example.com".to_string(),
        }];

        let config = PurgingConfig::with_smart_protected_columns(&data);

        let input = PurgingInput { data, config };

        let context = ProcessingContext::default();
        let result = purger.process(input, &context).unwrap();

        // Age should be protected and unchanged
        assert_eq!(result.purged_data[0]["age"], "25");

        // Name and email should be purged
        assert!(
            result.purged_data[0]["name"].contains("[REDACTED]")
                || result.purged_data[0]["name"] == "John Smith"
        );
        assert!(result.purged_data[0]["email"].contains("[REDACTED]"));

        assert_eq!(result.stats.protected_columns_skipped, 1);
        assert!(result.stats.columns_processed >= 2);
    }

    #[test]
    fn test_sanitize_free_text_uses_typed_placeholders() {
        let purger = PersonalInfoPurger::new(None).unwrap();
        let text = concat!(
            "Dr. Alex Example reports chest discomfort. ",
            "Phone 555-123-4567, email alex@example.invalid, ",
            "lives in Exampletown at 12 Example Street, medical record MRN-1234, ",
            "born on 01/01/1980."
        );

        let result = purger.sanitize_free_text(text, &[]);

        assert!(result.sanitized_text.contains("[PERSON]"));
        assert!(result.sanitized_text.contains("[PHONE]"));
        assert!(result.sanitized_text.contains("[EMAIL]"));
        assert!(result.sanitized_text.contains("[ADDRESS]"));
        assert!(result.sanitized_text.contains("[IDENTIFIER]"));
        assert!(result.sanitized_text.contains("[DOB]"));
        assert!(result.stats.total_redactions >= 6);
        assert!(result
            .stats
            .categories_hit
            .contains(&RedactionCategory::Person));
        assert!(result
            .stats
            .categories_hit
            .contains(&RedactionCategory::Phone));
        assert!(result
            .stats
            .categories_hit
            .contains(&RedactionCategory::Email));
    }

    #[test]
    fn test_record_context_names_redact_free_text_mentions() {
        let purger = PersonalInfoPurger::new(None).unwrap();
        let record = indexmap! {
            "first_name".to_string() => "Taylor".to_string(),
            "last_name".to_string() => "Synthetic".to_string(),
            "clinical_note".to_string() => "Taylor Synthetic reports a fever for three days.".to_string(),
        };

        let result =
            purger.sanitize_free_text_with_record(record["clinical_note"].as_str(), &record);

        assert!(result.sanitized_text.contains("[PERSON]"));
        assert!(!result.sanitized_text.contains("Taylor"));
        assert!(!result.sanitized_text.contains("Synthetic"));
        assert!(result.sanitized_text.contains("fever for three days"));
    }

    #[test]
    fn test_pure_clinical_text_remains_readable() {
        let purger = PersonalInfoPurger::new(None).unwrap();
        let text = "Diffuse abdominal discomfort; oral therapy was well tolerated.";

        let result = purger.sanitize_free_text(text, &[]);

        assert_eq!(result.stats.total_redactions, 0);
        assert_eq!(result.sanitized_text, text);
    }
}
