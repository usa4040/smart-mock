use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "smart-mock",
    about = "Context-aware local Mock API server from OpenAPI, TypeScript, or JSON schemas",
    version
)]
pub struct Cli {
    /// Path to OpenAPI (JSON/YAML), TypeScript interface, or JSON schema definition
    pub schema: PathBuf,

    /// Port to bind the mock server
    #[arg(short = 'p', long = "port", default_value_t = 4000)]
    pub port: u16,
}
