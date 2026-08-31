use super::DataSource;
use crate::analysis::ColumnStats;
use indexmap::IndexMap;
use platform_errors::{PlatformError, Result};

type LoadedRows = Vec<IndexMap<String, String>>;
type LoadedSource = (DataSource, LoadedRows);

/// Universal data analysis result combining format detection with content analysis
#[derive(Debug, Clone)]
pub struct DataAnalysis {
    /// Source file information
    pub source: DataSource,
    /// Format-specific detection results
    pub format_info: FormatInfo,
    /// Intelligent column analysis
    pub column_analysis: Vec<ColumnStats>,
    /// Suggested primary key candidates
    pub suggested_primary_keys: Vec<String>,
    /// Medical data confidence score (0-100)
    pub medical_data_score: f64,
    /// Processing suggestions
    pub suggestions: Vec<String>,
}

/// Format-specific information
#[derive(Debug, Clone)]
pub enum FormatInfo {
    Csv {
        separator: u8,
        has_header: bool,
        quote_char: Option<u8>,
        encoding: String,
    },
    Excel {
        sheet_names: Vec<String>,
        active_sheet: String,
        has_header: bool,
        row_count: usize,
        column_count: usize,
    },
    Xml {
        root_element: String,
        namespaces: Vec<String>,
        record_xpath: Option<String>,
        estimated_records: usize,
    },
}

/// Universal data loader trait for all supported formats
pub trait DataLoader {
    /// Load data from source into universal format
    fn load(&self, source: &DataSource) -> Result<Vec<IndexMap<String, String>>>;

    /// Analyze source without loading full data (for large files)
    fn analyze(&self, source: &DataSource) -> Result<DataAnalysis>;

    /// Check if this loader supports the given source type
    fn supports(&self, source: &DataSource) -> bool;

    /// Get loader name for error reporting
    fn name(&self) -> &'static str;
}

/// Universal data loader that delegates to format-specific loaders
pub struct UniversalDataLoader {
    csv_loader: Box<dyn DataLoader>,
    excel_loader: Box<dyn DataLoader>,
    xml_loader: Box<dyn DataLoader>,
}

impl UniversalDataLoader {
    pub fn new(
        csv_loader: Box<dyn DataLoader>,
        excel_loader: Box<dyn DataLoader>,
        xml_loader: Box<dyn DataLoader>,
    ) -> Self {
        Self {
            csv_loader,
            excel_loader,
            xml_loader,
        }
    }

    /// Create with default loaders
    pub fn with_defaults() -> Self {
        use super::{CsvLoader, ExcelLoader, XmlLoader};

        Self::new(
            Box::new(CsvLoader::new()),
            Box::new(ExcelLoader::new()),
            Box::new(XmlLoader::new()),
        )
    }

    /// Get the appropriate loader for a data source
    fn get_loader(&self, source: &DataSource) -> Result<&dyn DataLoader> {
        match source {
            DataSource::Csv(_) => Ok(self.csv_loader.as_ref()),
            DataSource::Excel(_, _) => Ok(self.excel_loader.as_ref()),
            DataSource::Xml(_, _) => Ok(self.xml_loader.as_ref()),
        }
    }

    /// Load data using the loader selected from the explicit source format.
    pub fn load_robust(&self, source: &DataSource) -> Result<Vec<IndexMap<String, String>>> {
        let loader = self.get_loader(source)?;

        if !loader.supports(source) {
            return Err(PlatformError::invalid_input(format!(
                "Loader {} does not support source format {}",
                loader.name(),
                source.format()
            )));
        }

        loader.load(source)
    }

    /// Analyze with format-specific intelligence
    pub fn analyze_comprehensive(&self, source: &DataSource) -> Result<DataAnalysis> {
        let loader = self.get_loader(source)?;

        if !loader.supports(source) {
            return Err(PlatformError::invalid_input(format!(
                "Loader {} does not support source format {}",
                loader.name(),
                source.format()
            )));
        }

        loader.analyze(source)
    }

    /// Discover and load multiple sources from directory
    pub fn discover_and_load(&self, directory: &std::path::Path) -> Result<Vec<LoadedSource>> {
        let mut results = Vec::new();

        if !directory.exists() || !directory.is_dir() {
            return Err(PlatformError::data_access(format!(
                "Directory not found: {}",
                directory.display()
            )));
        }

        for entry in std::fs::read_dir(directory)
            .map_err(|e| PlatformError::data_access(format!("IO error: {}", e)))?
        {
            let entry =
                entry.map_err(|e| PlatformError::data_access(format!("IO error: {}", e)))?;
            let path = entry.path();

            if path.is_file() {
                if let Ok(source) = DataSource::from_path(&path) {
                    match self.load_robust(&source) {
                        Ok(data) => {
                            if !data.is_empty() {
                                results.push((source, data));
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to load {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

impl DataLoader for UniversalDataLoader {
    fn load(&self, source: &DataSource) -> Result<Vec<IndexMap<String, String>>> {
        self.load_robust(source)
    }

    fn analyze(&self, source: &DataSource) -> Result<DataAnalysis> {
        self.analyze_comprehensive(source)
    }

    fn supports(&self, source: &DataSource) -> bool {
        self.get_loader(source)
            .map(|l| l.supports(source))
            .unwrap_or(false)
    }

    fn name(&self) -> &'static str {
        "UniversalDataLoader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLoader {
        name: &'static str,
        supports_csv: bool,
    }

    impl DataLoader for MockLoader {
        fn load(&self, _source: &DataSource) -> Result<Vec<IndexMap<String, String>>> {
            Ok(vec![IndexMap::new()])
        }

        fn analyze(&self, source: &DataSource) -> Result<DataAnalysis> {
            Ok(DataAnalysis {
                source: source.clone(),
                format_info: FormatInfo::Csv {
                    separator: b',',
                    has_header: true,
                    quote_char: Some(b'"'),
                    encoding: "UTF-8".to_string(),
                },
                column_analysis: Vec::new(),
                suggested_primary_keys: Vec::new(),
                medical_data_score: 0.0,
                suggestions: Vec::new(),
            })
        }

        fn supports(&self, source: &DataSource) -> bool {
            matches!(source, DataSource::Csv(_)) && self.supports_csv
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[test]
    fn test_universal_loader_creation() {
        let loader = UniversalDataLoader::new(
            Box::new(MockLoader {
                name: "CSV",
                supports_csv: true,
            }),
            Box::new(MockLoader {
                name: "Excel",
                supports_csv: false,
            }),
            Box::new(MockLoader {
                name: "XML",
                supports_csv: false,
            }),
        );

        let csv_source = DataSource::Csv("test.csv".into());
        assert!(loader.supports(&csv_source));
    }
}
