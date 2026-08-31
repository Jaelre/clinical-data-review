use crate::domains::{
    etl::{
        CohortIngestionConfig, CohortIngestionProcessor, EtlConfig, EtlMappingConfig, EtlProcessor,
    },
    purging::{PersonalInfoPurger, PurgingConfig, PurgingInput},
    DataProcessor, ProcessingContext,
};
use crate::infrastructure::data_source::{DataSource, UniversalDataLoader};
use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;

pub fn run_purging(
    files: Vec<PathBuf>,
    protected_columns: Option<Vec<String>>,
    redaction_text: Option<String>,
    name_dictionary: Option<PathBuf>,
    context: &ProcessingContext,
) -> Result<()> {
    println!("🔒 Running PII purging operation...");

    let purger = PersonalInfoPurger::new(name_dictionary.as_deref())?;

    // Use smart detection if no protected columns specified
    let use_smart_detection = protected_columns.is_none();

    let redaction = redaction_text.unwrap_or_else(|| "[REDACTED]".to_string());
    let loader = UniversalDataLoader::with_defaults();

    for file in files {
        let display_name = file
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Input path `{}` has no file name", file.display()))?;
        println!("   Processing: {}", display_name.to_string_lossy());

        let source = DataSource::from_path(&file)
            .map_err(|e| anyhow::anyhow!("Failed to detect file format: {}", e))?;
        let data = loader.load_robust(&source)?;

        let config = if use_smart_detection {
            let mut smart_config = PurgingConfig::with_smart_protected_columns(&data);
            smart_config.redaction_text = redaction.clone();
            println!(
                "   🧠 Smart detection found {} protected columns",
                smart_config.protected_columns.len()
            );
            smart_config
        } else {
            let protected_cols: HashSet<String> = protected_columns
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Protected columns configuration is missing"))?
                .iter()
                .cloned()
                .collect();
            PurgingConfig {
                protected_columns: protected_cols,
                redaction_text: redaction.clone(),
            }
        };

        let input = PurgingInput { data, config };

        let output = purger.process(input, context)?;
        let filename = file
            .file_stem()
            .ok_or_else(|| anyhow::anyhow!("Input path `{}` has no file stem", file.display()))?;
        let output_path = context
            .output_dir
            .join(format!("{}.purged.csv", filename.to_string_lossy()));
        write_csv_data(&output.purged_data, &output_path)?;

        println!(
            "   ✅ Made {} redactions (saved to {})",
            output.stats.total_redactions,
            output_path
                .file_name()
                .expect("constructed output path always has a filename")
                .to_string_lossy()
        );
    }

    Ok(())
}

pub fn run_analysis(
    files: Vec<PathBuf>,
    detailed: bool,
    _context: &ProcessingContext,
) -> Result<()> {
    println!("📊 Analyzing data files...");
    let loader = UniversalDataLoader::with_defaults();

    for file in files {
        let display_name = file
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Input path `{}` has no file name", file.display()))?;
        println!("\n📄 Analyzing: {}", display_name.to_string_lossy());

        // Basic file info
        if let Ok(metadata) = std::fs::metadata(&file) {
            let size_kb = metadata.len() / 1024;
            println!("   Size: {} KB", size_kb);
        }

        // Enhanced analysis with intelligent pattern detection
        let source = DataSource::from_path(&file)
            .map_err(|e| anyhow::anyhow!("Failed to detect file format: {}", e))?;
        let analysis = loader.analyze_comprehensive(&source).map_err(|error| {
            anyhow::anyhow!("Analysis failed for `{}`: {error}", file.display())
        })?;
        {
            println!("   ✅ Analysis completed successfully");
            println!("   Format: {}", source.format());

            // Format-specific information
            match &analysis.format_info {
                crate::infrastructure::data_source::FormatInfo::Csv {
                    separator,
                    has_header,
                    ..
                } => {
                    println!("   Separator: '{}'", *separator as char);
                    println!("   Has header: {}", has_header);
                }
                crate::infrastructure::data_source::FormatInfo::Excel {
                    sheet_names,
                    active_sheet,
                    row_count,
                    column_count,
                    ..
                } => {
                    println!("   Active sheet: {}", active_sheet);
                    println!("   Total sheets: {}", sheet_names.len());
                    println!("   Rows: {}, Columns: {}", row_count, column_count);
                }
                crate::infrastructure::data_source::FormatInfo::Xml {
                    root_element,
                    estimated_records,
                    ..
                } => {
                    println!("   Root element: {}", root_element);
                    println!("   Estimated records: {}", estimated_records);
                }
            }

            println!(
                "   📊 Medical data confidence: {:.1}%",
                analysis.medical_data_score
            );

            if detailed {
                println!("   Columns: {}", analysis.column_analysis.len());

                // Show column analysis
                println!("\n   📋 Column Analysis:");
                for col in &analysis.column_analysis {
                    let purpose_display = match col.purpose {
                        crate::analysis::ColumnPurpose::PrimaryKey => "🔑 Primary Key",
                        crate::analysis::ColumnPurpose::Age => "👥 Age",
                        crate::analysis::ColumnPurpose::Gender => "⚧️ Gender",
                        crate::analysis::ColumnPurpose::PersonalName => "🛡️ Personal Name (PII)",
                        crate::analysis::ColumnPurpose::MedicalCode => "🏥 Medical Code",
                        crate::analysis::ColumnPurpose::DateTime => "📅 Date/Time",
                        crate::analysis::ColumnPurpose::Measurement => "📏 Measurement",
                        crate::analysis::ColumnPurpose::Category => "📊 Category",
                        _ => "❓ Unknown",
                    };

                    println!(
                        "      {} - {} (uniqueness: {:.1}%, confidence: {:.1}%)",
                        col.name,
                        purpose_display,
                        col.uniqueness_ratio * 100.0,
                        col.confidence_score
                    );

                    // Show sample values
                    if !col.sample_values.is_empty() {
                        let samples: Vec<_> = col
                            .sample_values
                            .iter()
                            .take(3)
                            .map(|s| s.as_str())
                            .collect();
                        println!("        Samples: {}", samples.join(", "));
                    }
                }

                // Show processing suggestions
                if !analysis.suggestions.is_empty() {
                    println!("\n   💡 Processing Suggestions:");
                    for suggestion in &analysis.suggestions {
                        println!("      {}", suggestion);
                    }
                }
            }
        }
    }

    Ok(())
}

// Helper functions
fn write_csv_data(data: &[indexmap::IndexMap<String, String>], path: &PathBuf) -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create(path)?;

    if let Some(first_row) = data.first() {
        // Write header with proper escaping
        let headers: Vec<String> = first_row
            .keys()
            .map(|header| {
                // Clean and escape column names
                let clean_header = header.trim_matches(['[', ']']);
                if clean_header.contains(',')
                    || clean_header.contains('"')
                    || clean_header.contains('\n')
                {
                    format!("\"{}\"", clean_header.replace('"', "\"\""))
                } else {
                    clean_header.to_string()
                }
            })
            .collect();
        writeln!(file, "{}", headers.join(","))?;

        // Write data rows with proper escaping
        for row in data {
            let values: Vec<String> = row
                .values()
                .map(|value| {
                    // Escape values that contain commas, quotes, or newlines
                    if value.contains(',')
                        || value.contains('"')
                        || value.contains('\n')
                        || value.contains('\r')
                    {
                        format!("\"{}\"", value.replace('"', "\"\""))
                    } else {
                        value.to_string()
                    }
                })
                .collect();
            writeln!(file, "{}", values.join(","))?;
        }
    }

    Ok(())
}

/// Run cohort ingestion workflow: text file → cohort_ingestion → research_cohorts
/// Cohort batches are always pre-computed during ingestion; a research session is optional.
#[allow(clippy::too_many_arguments)]
pub async fn run_cohort(
    file_path: PathBuf,
    database_url: String,
    cohort_name: String,
    tenant_slug: String,
    operator_handle: String,
    description: Option<String>,
    session_name: Option<String>,
    batch_size: usize,
    include_empty_placeholder: bool,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    println!("🚀 Starting Cohort Ingestion Workflow");
    println!("   📂 Input file: {}", file_path.display());
    println!("   🏷️  Cohort name: {}", cohort_name);
    println!("   🏠 Tenant: {}", tenant_slug);
    println!("   🧑‍💻 Local operator: {}", operator_handle);
    println!("   📦 Batch size: {}", batch_size);
    if let Some(ref sn) = session_name {
        println!("   📋 Session: {}", sn);
    }

    // Create configuration
    let mut config = CohortIngestionConfig::new(
        file_path.to_string_lossy().to_string(),
        database_url,
        cohort_name.clone(),
        tenant_slug.clone(),
        operator_handle.clone(),
    )
    .with_session_name(session_name.clone())
    .with_batch_size(batch_size)
    .with_include_empty_placeholder(include_empty_placeholder)
    .with_dry_run(dry_run)
    .with_verbose(verbose);

    if let Some(desc) = description {
        config = config.with_description(desc);
    }

    // Create and run processor
    let processor = CohortIngestionProcessor::new(config);
    let result = processor.process().await?;

    // Display results
    if dry_run {
        println!("✅ DRY RUN COMPLETE");
        println!("   📊 Total patient IDs found: {}", result.total_patients);
        if let Some(batches) = result.batch_count {
            println!("   📦 Cohort batches that would be created: {}", batches);
        }
        println!("   💡 Run without --dry-run to perform actual ingestion");
    } else {
        println!("✅ COHORT INGESTION COMPLETE");
        if let Some(cohort_id) = result.cohort_id {
            println!("   🎯 Created cohort ID: {}", cohort_id);
        }
        println!("   📊 Total patients: {}", result.total_patients);
        println!(
            "   ✅ Successfully processed: {}",
            result.processed_patients
        );
        if let Some(batches) = result.batch_count {
            println!("   📦 Cohort batches created: {}", batches);
        }
        if result.error_count > 0 {
            println!("   ⚠️  Errors encountered: {}", result.error_count);
        }

        if let Some(session_id) = result.session_id {
            println!("   📋 Session ID: {}", session_id);
        }

        println!(
            "\n🎉 Cohort '{}' is now available in Clinical Data Review!",
            cohort_name
        );
        if session_name.is_some() {
            println!(
                "   Research session is ready for review using the ETL-authored cohort batches"
            );
        }
        println!(
            "   Database table: research_cohorts (tenant: {})",
            tenant_slug
        );
    }

    Ok(())
}

/// Run the explicit-mapping ETL operation.
pub async fn run_etl(
    input: PathBuf,
    mapping: PathBuf,
    database_url: String,
    purge_pii: bool,
    name_dictionary: Option<PathBuf>,
) -> Result<()> {
    if !input.is_dir() {
        return Err(anyhow::anyhow!(
            "ETL input must be an existing directory: {}",
            input.display()
        ));
    }
    if name_dictionary.is_some() && !purge_pii {
        return Err(anyhow::anyhow!("--name-dictionary requires --purge-pii"));
    }
    if !database_url.starts_with("sqlite://") {
        return Err(anyhow::anyhow!(
            "Unsupported database URL scheme; only sqlite:// URLs are accepted"
        ));
    }

    let mapping_config = EtlMappingConfig::from_file(&mapping)
        .map_err(|error| anyhow::anyhow!("ETL mapping validation failed: {error}"))?;

    println!("🚀 Starting local workbook ETL into SQLite");
    println!("   Mapping: {}", mapping.display());
    if purge_pii {
        println!("   🛡️  PII PURGING ENABLED");
    } else {
        eprintln!("\n⚠️  WARNING: PII PURGING IS DISABLED");
        eprintln!("⚠️  Free-text content will be stored exactly as supplied.\n");
    }

    let config = EtlConfig {
        input_directory: input.to_string_lossy().to_string(),
        database_url,
        purge_pii,
        name_dictionary,
        mapping_config,
    };

    let stats = EtlProcessor::process(config)
        .await
        .map_err(|error| anyhow::anyhow!("ETL failed: {error}"))?;
    println!("✅ ETL process completed successfully");
    println!("   Patients: {}", stats.patients_processed);
    println!("   Processing time: {:.2}s", stats.processing_time_seconds);

    Ok(())
}
