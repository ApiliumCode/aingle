use aingle_cli_bundle::AinAppBundle;
use structopt::StructOpt;

/// Main `ai-app` executable entrypoint.
#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    AinAppBundle::from_args().run().await
}
