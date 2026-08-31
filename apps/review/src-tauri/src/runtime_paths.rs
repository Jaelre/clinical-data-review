use platform_errors::{PlatformError, Result};
use std::env;
use std::path::{Path, PathBuf};

pub const DEFAULT_DB_FILENAME: &str = "clinical-data-review.sqlite3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDatabase {
    pub database_url: String,
    pub path: Option<PathBuf>,
    pub source: &'static str,
}

pub fn resolve_database() -> Result<ResolvedDatabase> {
    if let Ok(database_url) = env::var("DATABASE_URL") {
        let trimmed = database_url.trim();
        if trimmed.is_empty() {
            return Err(PlatformError::config_key(
                "DATABASE_URL is set but empty",
                "DATABASE_URL",
            ));
        }
        if !trimmed.starts_with("sqlite://") {
            return Err(PlatformError::config_key(
                "DATABASE_URL must use the sqlite:// scheme in the local-only build",
                "DATABASE_URL",
            ));
        }
        return Ok(ResolvedDatabase {
            path: sqlite_path_from_url(trimmed),
            database_url: trimmed.to_string(),
            source: "DATABASE_URL",
        });
    }

    if let Ok(database_path) = env::var("REVIEW_APP_DB_PATH") {
        let database_path = normalize_path(&database_path)?;
        return Ok(ResolvedDatabase {
            database_url: sqlite_url_from_path(&database_path),
            path: Some(database_path),
            source: "REVIEW_APP_DB_PATH",
        });
    }

    let data_root = dirs::data_local_dir().ok_or_else(|| {
        PlatformError::config(
            "Unable to resolve the operating system's local application-data directory",
        )
    })?;
    let application_dir = data_root.join("clinical-data-review");
    std::fs::create_dir_all(&application_dir).map_err(|error| {
        PlatformError::config(format!(
            "Unable to create local application-data directory `{}`: {error}",
            application_dir.display()
        ))
    })?;
    let database_path = application_dir.join(DEFAULT_DB_FILENAME);
    Ok(ResolvedDatabase {
        database_url: sqlite_url_from_path(&database_path),
        path: Some(database_path),
        source: "operating-system data directory",
    })
}

fn sqlite_url_from_path(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

fn sqlite_path_from_url(database_url: &str) -> Option<PathBuf> {
    database_url
        .strip_prefix("sqlite://")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn normalize_path(raw_path: &str) -> Result<PathBuf> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(PlatformError::config_key(
            "REVIEW_APP_DB_PATH is set but empty",
            "REVIEW_APP_DB_PATH",
        ));
    }

    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        return Ok(path);
    }
    env::current_dir()
        .map(|current_dir| current_dir.join(path))
        .map_err(|error| {
            PlatformError::config(format!(
                "Unable to resolve REVIEW_APP_DB_PATH relative to the current directory: {error}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_path_is_extracted_only_from_supported_urls() {
        assert_eq!(
            sqlite_path_from_url("sqlite:///tmp/review.db"),
            Some(PathBuf::from("/tmp/review.db"))
        );
        assert_eq!(sqlite_path_from_url("https://example.invalid/test"), None);
    }

    #[test]
    fn relative_explicit_paths_are_resolved() {
        let resolved = normalize_path("data/review.db").unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("data/review.db"));
    }
}
