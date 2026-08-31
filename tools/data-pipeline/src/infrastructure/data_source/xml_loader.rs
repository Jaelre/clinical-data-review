use super::{DataAnalysis, DataLoader, DataSource, FormatInfo};
use crate::analysis::{ColumnAnalyzer, ColumnStats};
use indexmap::IndexMap;
use platform_errors::{PlatformError, Result};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::Path;

pub struct XmlLoader {
    analyzer: ColumnAnalyzer,
}

impl Default for XmlLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl XmlLoader {
    pub fn new() -> Self {
        Self {
            analyzer: ColumnAnalyzer::new(),
        }
    }

    /// Auto-detect record structure in XML by analyzing repeated patterns
    pub fn detect_record_structure<P: AsRef<Path>>(path: P) -> Result<RecordStructure> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PlatformError::data_access(format!("IO error: {}", e)))?;

        let doc = Document::parse(&content)
            .map_err(|e| PlatformError::data_access(format!("Failed to parse XML: {}", e)))?;

        let root = doc.root_element();
        let mut root_child_counts = HashMap::new();
        let mut element_counts = HashMap::new();
        let mut max_count = 0;
        let mut best_element = None;

        for child in root.children().filter(|node| node.is_element()) {
            let name = child.tag_name().name().to_string();
            *root_child_counts.entry(name).or_insert(0) += 1;
        }

        for (element_name, count) in &root_child_counts {
            if *count > max_count && *count > 1 {
                max_count = *count;
                best_element = Some(element_name.clone());
            }
        }

        // Count all element types to find repeating patterns
        if best_element.is_none() {
            Self::count_elements(root, &mut element_counts);

            // Find the most frequent element (likely records)
            for (element_name, count) in &element_counts {
                if *count > max_count && *count > 1 {
                    max_count = *count;
                    best_element = Some(element_name.clone());
                }
            }
        }

        let record_element = best_element.unwrap_or_else(|| {
            // Fallback: use first child element name
            root.children()
                .find(|n| n.is_element())
                .map(|n| n.tag_name().name().to_string())
                .unwrap_or_else(|| "record".to_string())
        });

        // Find a sample record to analyze structure
        let sample_record = Self::find_element_by_name(root, &record_element).ok_or_else(|| {
            PlatformError::invalid_input(format!(
                "No elements found with name '{}'",
                record_element
            ))
        })?;

        let field_names = Self::extract_field_names(sample_record);

        Ok(RecordStructure {
            root_element: root.tag_name().name().to_string(),
            record_element,
            field_names,
            estimated_count: max_count,
            namespaces: Self::extract_namespaces(&doc),
        })
    }

    /// Recursively count all elements in the XML tree
    fn count_elements(node: Node, counts: &mut HashMap<String, usize>) {
        if node.is_element() {
            let name = node.tag_name().name().to_string();
            *counts.entry(name).or_insert(0) += 1;
        }

        for child in node.children() {
            Self::count_elements(child, counts);
        }
    }

    /// Find first element with given name
    fn find_element_by_name<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
        if node.is_element() && node.tag_name().name() == name {
            return Some(node);
        }

        for child in node.children() {
            if let Some(found) = Self::find_element_by_name(child, name) {
                return Some(found);
            }
        }

        None
    }

    /// Extract field names from a sample record element
    fn extract_field_names(record: Node) -> Vec<String> {
        let mut fields = Vec::new();

        for child in record.children() {
            if child.is_element() {
                fields.push(child.tag_name().name().to_string());
            }
        }

        // If no child elements, use attributes
        if fields.is_empty() {
            for attr in record.attributes() {
                fields.push(attr.name().to_string());
            }
        }

        fields
    }

    /// Extract namespace information
    fn extract_namespaces(_doc: &Document) -> Vec<String> {
        // For now, return empty - namespace handling can be enhanced later
        Vec::new()
    }

    /// Load XML data into universal format
    pub fn load_xml<P: AsRef<Path>>(
        path: P,
        xpath: Option<&str>,
    ) -> Result<Vec<IndexMap<String, String>>> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| PlatformError::data_access(format!("IO error: {}", e)))?;

        let doc = Document::parse(&content)
            .map_err(|e| PlatformError::data_access(format!("Failed to parse XML: {}", e)))?;

        let structure = if xpath.is_some() {
            // Use provided XPath logic (simplified for now)
            Self::detect_record_structure(&path)?
        } else {
            Self::detect_record_structure(&path)?
        };

        Self::extract_records(&doc, &structure)
    }

    /// Extract records based on detected structure
    fn extract_records(
        doc: &Document,
        structure: &RecordStructure,
    ) -> Result<Vec<IndexMap<String, String>>> {
        let mut records = Vec::new();
        let root = doc.root_element();

        // Find all record elements
        let record_nodes = Self::find_all_elements_by_name(root, &structure.record_element);

        for record_node in record_nodes {
            let mut record = IndexMap::new();

            // Extract child elements as fields
            for child in record_node.children() {
                if child.is_element() {
                    let field_name = child.tag_name().name().to_string();
                    let field_value = Self::extract_text_content(child);
                    record.insert(field_name, field_value);
                }
            }

            // Extract attributes if no child elements
            if record.is_empty() {
                for attr in record_node.attributes() {
                    record.insert(attr.name().to_string(), attr.value().to_string());
                }
            }

            // Include the record if it has any data
            if !record.is_empty() {
                records.push(record);
            }
        }

        Ok(records)
    }

    /// Find all elements with given name
    fn find_all_elements_by_name<'a>(node: Node<'a, 'a>, name: &str) -> Vec<Node<'a, 'a>> {
        let mut elements = Vec::new();

        if node.is_element() && node.tag_name().name() == name {
            elements.push(node);
        }

        for child in node.children() {
            elements.extend(Self::find_all_elements_by_name(child, name));
        }

        elements
    }

    /// Extract text content from a node, handling nested elements
    fn extract_text_content(node: Node) -> String {
        let mut content = String::new();

        for child in node.children() {
            if child.is_text() {
                content.push_str(child.text().unwrap_or(""));
            } else if child.is_element() {
                // For nested elements, concatenate with element name
                let nested_content = Self::extract_text_content(child);
                if !nested_content.trim().is_empty() {
                    if !content.is_empty() {
                        content.push_str("; ");
                    }
                    content.push_str(&format!("{}:{}", child.tag_name().name(), nested_content));
                }
            }
        }

        content.trim().to_string()
    }

    /// Calculate medical data confidence for XML content
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
            // XML-specific medical indicators
            if name_lower.contains("record")
                || name_lower.contains("entry")
                || name_lower.contains("document")
            {
                score += 10.0;
            }
        }

        score.min(100.0)
    }

    /// Generate XML-specific processing suggestions
    fn generate_suggestions(&self, analysis: &DataAnalysis) -> Vec<String> {
        let mut suggestions = Vec::new();

        if let FormatInfo::Xml {
            estimated_records, ..
        } = &analysis.format_info
        {
            if *estimated_records > 1000 {
                suggestions.push(
                    "📊 Large XML dataset - consider streaming processing for memory efficiency"
                        .to_string(),
                );
            }
        }

        if analysis.medical_data_score > 50.0 {
            suggestions.push(
                "🏥 Medical XML data detected - ensure compliance with healthcare data standards"
                    .to_string(),
            );
        }

        if !analysis.suggested_primary_keys.is_empty() {
            suggestions.push(format!(
                "🔑 Potential key fields: {}",
                analysis.suggested_primary_keys.join(", ")
            ));
        }

        // Check for nested structure complexity
        let complex_fields: Vec<&str> = analysis
            .column_analysis
            .iter()
            .filter(|col| col.name.contains(':'))
            .map(|col| col.name.as_str())
            .collect();

        if !complex_fields.is_empty() {
            suggestions.push(
                "🔗 Nested XML structure detected - data has been flattened for processing"
                    .to_string(),
            );
        }

        suggestions
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RecordStructure {
    root_element: String,
    record_element: String,
    field_names: Vec<String>,
    estimated_count: usize,
    namespaces: Vec<String>,
}

impl DataLoader for XmlLoader {
    fn load(&self, source: &DataSource) -> Result<Vec<IndexMap<String, String>>> {
        match source {
            DataSource::Xml(path, xpath_opt) => Self::load_xml(path, xpath_opt.as_deref()),
            _ => Err(PlatformError::invalid_input(format!(
                "XmlLoader does not support source type: {}",
                source.format()
            ))),
        }
    }

    fn analyze(&self, source: &DataSource) -> Result<DataAnalysis> {
        match source {
            DataSource::Xml(path, xpath_opt) => {
                let structure = Self::detect_record_structure(path)?;

                // Load sample data for analysis
                let data = Self::load_xml(path, xpath_opt.as_deref())?;

                let format_info = FormatInfo::Xml {
                    root_element: structure.root_element,
                    namespaces: structure.namespaces,
                    record_xpath: xpath_opt.clone(),
                    estimated_records: structure.estimated_count,
                };

                let column_analysis = if !data.is_empty() {
                    self.analyzer
                        .analyze_file(&data)
                        .map_err(|e| PlatformError::data_access(format!("Analysis error: {}", e)))?
                } else {
                    Vec::new()
                };

                let medical_data_score = self.calculate_medical_score(&column_analysis);

                // Simple primary key detection for XML
                let suggested_primary_keys: Vec<String> = column_analysis
                    .iter()
                    .filter(|col| {
                        let name_lower = col.name.to_lowercase();
                        (name_lower.contains("id") || name_lower.contains("key"))
                            && col.uniqueness_ratio > 0.8
                    })
                    .map(|col| col.name.clone())
                    .take(3)
                    .collect();

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
                "XmlLoader does not support source type: {}",
                source.format()
            ))),
        }
    }

    fn supports(&self, source: &DataSource) -> bool {
        matches!(source, DataSource::Xml(_, _))
    }

    fn name(&self) -> &'static str {
        "XmlLoader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_supports() {
        let loader = XmlLoader::new();
        let xml_source = DataSource::Xml("test.xml".into(), None);
        let csv_source = DataSource::Csv("test.csv".into());

        assert!(loader.supports(&xml_source));
        assert!(!loader.supports(&csv_source));
    }

    #[test]
    fn test_loader_name() {
        let loader = XmlLoader::new();
        assert_eq!(loader.name(), "XmlLoader");
    }

    #[test]
    fn test_extract_text_content() {
        let xml = r#"<root><test>simple text</test></root>"#;
        let doc = Document::parse(xml).unwrap();
        let test_node = doc.root_element().first_element_child().unwrap();

        let content = XmlLoader::extract_text_content(test_node);
        assert_eq!(content, "simple text");
    }

    #[test]
    fn test_simple_xml_structure() {
        let xml = r#"
        <patients>
            <patient id="1">
                <name>John Doe</name>
                <age>30</age>
            </patient>
            <patient id="2">
                <name>Jane Smith</name>
                <age>25</age>
            </patient>
        </patients>
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(xml.as_bytes()).unwrap();

        let structure = XmlLoader::detect_record_structure(temp_file.path()).unwrap();
        assert_eq!(structure.record_element, "patient");
        assert_eq!(structure.estimated_count, 2);
    }
}
