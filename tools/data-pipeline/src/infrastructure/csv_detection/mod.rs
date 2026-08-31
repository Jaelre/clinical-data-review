pub mod polars_csv_detector;

// Re-export the public detector types from their implementation modules.
pub use polars_csv_detector::PolarsCsvDetector as CsvDetector;
pub use polars_csv_detector::*;
