pub mod etl;
pub mod purging;

#[derive(Debug, Clone)]
pub struct ProcessingContext {
    pub output_dir: std::path::PathBuf,
}

impl Default for ProcessingContext {
    fn default() -> Self {
        Self {
            output_dir: "./output".into(),
        }
    }
}

pub trait DataProcessor {
    type Input;
    type Output;
    type Error;

    fn process(
        &self,
        input: Self::Input,
        context: &ProcessingContext,
    ) -> Result<Self::Output, Self::Error>;
}
