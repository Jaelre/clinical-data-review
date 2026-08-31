use clinical_data_pipeline::cli::{Cli, Commands};
use clinical_data_pipeline::domains::ProcessingContext;
use clinical_data_pipeline::operations;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse_args();

    let context = ProcessingContext {
        output_dir: cli.get_output_dir(),
    };

    match cli.command {
        Commands::Purge {
            files,
            protected_columns,
            redaction_text,
            name_dictionary,
        } => {
            std::fs::create_dir_all(&context.output_dir)?;
            operations::run_purging(
                files,
                Some(protected_columns),
                Some(redaction_text),
                name_dictionary,
                &context,
            )?;
        }
        Commands::Analyze { files, detailed } => {
            operations::run_analysis(files, detailed, &context)?;
        }
        Commands::Etl {
            input,
            mapping,
            database_url,
            purge_pii,
            name_dictionary,
        } => {
            operations::run_etl(input, mapping, database_url, purge_pii, name_dictionary).await?;
        }
        Commands::Cohort {
            file_path,
            database_url,
            cohort_name,
            tenant_slug,
            operator_handle,
            description,
            session_name,
            batch_size,
            include_empty_placeholder,
            dry_run,
            verbose,
        } => {
            operations::run_cohort(
                file_path,
                database_url,
                cohort_name,
                tenant_slug,
                operator_handle,
                description,
                session_name,
                batch_size,
                include_empty_placeholder,
                dry_run,
                verbose,
            )
            .await?;
        }
    }

    Ok(())
}
