use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum DataSource {
    /// CSV file with separator detection
    Csv(PathBuf),
    /// Excel file with optional sheet name (None = auto-detect primary sheet)
    Excel(PathBuf, Option<String>),
    /// XML file with optional XPath for data extraction
    Xml(PathBuf, Option<String>),
}

impl DataSource {
    /// Auto-detect data source type from file extension
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, DataSourceError> {
        let path = path.as_ref().to_path_buf();

        match path.extension().and_then(|s| s.to_str()) {
            Some("csv") => Ok(DataSource::Csv(path)),
            Some("xlsx") | Some("xls") => Ok(DataSource::Excel(path, None)),
            Some("xml") => Ok(DataSource::Xml(path, None)),
            Some(ext) => Err(DataSourceError::UnsupportedFormat {
                extension: ext.to_string(),
                path: path.to_string_lossy().to_string(),
            }),
            None => Err(DataSourceError::NoExtension {
                path: path.to_string_lossy().to_string(),
            }),
        }
    }

    /// Get the file path regardless of source type
    pub fn path(&self) -> &PathBuf {
        match self {
            DataSource::Csv(path) => path,
            DataSource::Excel(path, _) => path,
            DataSource::Xml(path, _) => path,
        }
    }

    /// Get the format type as string
    pub fn format(&self) -> &'static str {
        match self {
            DataSource::Csv(_) => "CSV",
            DataSource::Excel(_, _) => "Excel",
            DataSource::Xml(_, _) => "XML",
        }
    }

    /// Check if file exists
    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    /// Get file size in bytes
    pub fn size(&self) -> Result<u64, std::io::Error> {
        std::fs::metadata(self.path()).map(|m| m.len())
    }

    /// Set sheet name for Excel sources
    pub fn with_sheet(self, sheet_name: String) -> Self {
        if let DataSource::Excel(path, _) = self {
            DataSource::Excel(path, Some(sheet_name))
        } else {
            self
        }
    }

    /// Set XPath for XML sources
    pub fn with_xpath(self, xpath: String) -> Self {
        if let DataSource::Xml(path, _) = self {
            DataSource::Xml(path, Some(xpath))
        } else {
            self
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DataSourceError {
    #[error("Unsupported file format '{extension}' for file: {path}")]
    UnsupportedFormat { extension: String, path: String },

    #[error("File has no extension: {path}")]
    NoExtension { path: String },

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_csv_detection() {
        let source = DataSource::from_path("test.csv").unwrap();
        assert_eq!(source.format(), "CSV");
        assert_eq!(source.path(), &PathBuf::from("test.csv"));
    }

    #[test]
    fn test_excel_detection() {
        let source = DataSource::from_path("test.xlsx").unwrap();
        assert_eq!(source.format(), "Excel");
        assert!(matches!(source, DataSource::Excel(_, None)));
    }

    #[test]
    fn test_xml_detection() {
        let source = DataSource::from_path("test.xml").unwrap();
        assert_eq!(source.format(), "XML");
        assert!(matches!(source, DataSource::Xml(_, None)));
    }

    #[test]
    fn test_with_sheet() {
        let source = DataSource::from_path("test.xlsx")
            .unwrap()
            .with_sheet("Sheet1".to_string());
        assert!(matches!(source, DataSource::Excel(_, Some(sheet)) if sheet == "Sheet1"));
    }

    #[test]
    fn test_unsupported_format() {
        let result = DataSource::from_path("test.pdf");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DataSourceError::UnsupportedFormat { .. }
        ));
    }
}
