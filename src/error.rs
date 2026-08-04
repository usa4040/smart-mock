use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to read schema file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse schema: {0}")]
    Parse(String),

    #[error("unsupported schema format: {0}")]
    UnsupportedFormat(String),

    #[error("schema validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
