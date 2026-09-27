use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "smart-mock",
    about = "Stateful OpenAPI mock server. POST sticks, and spec violations fail.",
    version
)]
pub struct Cli {
    /// Path to an OpenAPI v3 document (JSON or YAML)
    pub schema: PathBuf,

    /// Port to bind the mock server
    #[arg(short = 'p', long = "port", default_value_t = 4000)]
    pub port: u16,
}
