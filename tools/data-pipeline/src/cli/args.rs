use clap::{Parser, Subcommand};
use std::path::PathBuf;

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid integer value: {value}"))?;
    if parsed == 0 {
        return Err("value must be greater than 0".to_string());
    }
    Ok(parsed)
}

#[derive(Parser)]
#[command(name = "clinical-data-pipeline")]
#[command(about = "Local clinical-data import, validation, and PII-redaction tools")]
#[command(version = "0.1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, help = "Output directory for processed files")]
    pub output_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Remove personal information from delimited files")]
    Purge {
        #[arg(help = "Input files to purge", required = true)]
        files: Vec<PathBuf>,

        #[arg(long, help = "Protected columns that must not be purged")]
        protected_columns: Vec<String>,

        #[arg(long, default_value = "[REDACTED]", help = "Replacement text")]
        redaction_text: String,

        #[arg(long, help = "Approved local newline-delimited name dictionary")]
        name_dictionary: Option<PathBuf>,
    },

    #[command(about = "Detect and analyze delimited file formats")]
    Analyze {
        #[arg(help = "Files to analyze", required = true)]
        files: Vec<PathBuf>,

        #[arg(long, help = "Show detailed schema information")]
        detailed: bool,
    },

    #[command(about = "Import explicitly mapped workbooks into a local SQLite database")]
    Etl {
        #[arg(help = "Directory containing the mapped workbooks")]
        input: PathBuf,

        #[arg(long, help = "Required TOML file and column mapping")]
        mapping: PathBuf,

        #[arg(
            long,
            env = "DATABASE_URL",
            help = "SQLite connection string (sqlite://...)"
        )]
        database_url: String,

        #[arg(long, help = "Enable PII purging before database insertion")]
        purge_pii: bool,

        #[arg(
            long,
            requires = "purge_pii",
            help = "Approved local newline-delimited name dictionary"
        )]
        name_dictionary: Option<PathBuf>,
    },

    #[command(about = "Import synthetic or approved patient IDs into a research cohort")]
    Cohort {
        #[arg(help = "Text file containing patient IDs separated by newlines or semicolons")]
        file_path: PathBuf,

        #[arg(long, help = "SQLite connection string (sqlite://...)")]
        database_url: String,

        #[arg(long, help = "Name for the cohort")]
        cohort_name: String,

        #[arg(long, help = "Workspace slug")]
        tenant_slug: String,

        #[arg(
            long,
            default_value = "example-reviewer",
            help = "Local operator handle used to attribute the cohort"
        )]
        operator_handle: String,

        #[arg(long, help = "Optional cohort description")]
        description: Option<String>,

        #[arg(long, help = "Optionally create a review session")]
        session_name: Option<String>,

        #[arg(
            long,
            default_value_t = 12,
            value_parser = parse_positive_usize,
            help = "Patients per precomputed cohort batch"
        )]
        batch_size: usize,

        #[arg(long, help = "Append one empty placeholder batch")]
        include_empty_placeholder: bool,

        #[arg(long, help = "Validate without inserting")]
        dry_run: bool,

        #[arg(long, help = "Show detailed progress information")]
        verbose: bool,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }

    pub fn get_output_dir(&self) -> PathBuf {
        self.output_dir.clone().unwrap_or_else(|| "./output".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohort_arguments_are_parsed() {
        let cli = Cli::try_parse_from([
            "clinical-data-pipeline",
            "cohort",
            "cohort.txt",
            "--database-url",
            "sqlite://./review.db",
            "--cohort-name",
            "Synthetic Review",
            "--tenant-slug",
            "example-research-workspace",
        ])
        .unwrap();

        match cli.command {
            Commands::Cohort {
                cohort_name,
                tenant_slug,
                operator_handle,
                ..
            } => {
                assert_eq!(cohort_name, "Synthetic Review");
                assert_eq!(tenant_slug, "example-research-workspace");
                assert_eq!(operator_handle, "example-reviewer");
            }
            _ => panic!("expected cohort command"),
        }
    }

    #[test]
    fn dictionary_requires_etl_purging() {
        let cli = Cli::try_parse_from([
            "clinical-data-pipeline",
            "etl",
            "fixtures/synthetic",
            "--mapping",
            "fixtures/synthetic/mapping.toml",
            "--database-url",
            "sqlite://./review.db",
            "--name-dictionary",
            "fixtures/synthetic/names.txt",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn etl_contract_accepts_explicit_mapping() {
        let cli = Cli::try_parse_from([
            "clinical-data-pipeline",
            "etl",
            "fixtures/synthetic",
            "--mapping",
            "fixtures/synthetic/mapping.toml",
            "--database-url",
            "sqlite://./review.db",
            "--purge-pii",
        ])
        .unwrap();
        assert!(matches!(cli.command, Commands::Etl { .. }));
    }

    #[test]
    fn etl_contract_rejects_removed_flags() {
        for removed_flag in ["--migrate", "--tenant-name", "--dry-run", "--verbose"] {
            let cli = Cli::try_parse_from([
                "clinical-data-pipeline",
                "etl",
                "fixtures/synthetic",
                "--mapping",
                "fixtures/synthetic/mapping.toml",
                "--database-url",
                "sqlite://./review.db",
                removed_flag,
            ]);
            assert!(
                cli.is_err(),
                "removed ETL flag was accepted: {removed_flag}"
            );
        }
    }

    #[test]
    fn cohort_batch_size_must_be_positive() {
        let cli = Cli::try_parse_from([
            "clinical-data-pipeline",
            "cohort",
            "cohort.txt",
            "--database-url",
            "sqlite://./review.db",
            "--cohort-name",
            "Synthetic Review",
            "--tenant-slug",
            "example-research-workspace",
            "--batch-size",
            "0",
        ]);
        assert!(cli.is_err());
    }
}
