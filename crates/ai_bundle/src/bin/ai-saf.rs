use aingle_cli_bundle::AinSafBundle;
use structopt::StructOpt;

/// Main `ai-saf` executable entrypoint.
#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    AinSafBundle::from_args().run().await
}
