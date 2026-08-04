use clap::Parser;
use smart_mock::cli::Cli;
use smart_mock::schema;
use smart_mock::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let routes = schema::load_routes(&cli.schema)?;
    if routes.is_empty() {
        anyhow::bail!("no routes found in schema: {}", cli.schema.display());
    }
    server::serve(routes, cli.port).await
}
