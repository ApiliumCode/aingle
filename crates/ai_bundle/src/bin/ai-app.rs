// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use aingle_cli_bundle::AinAppBundle;
use structopt::StructOpt;

/// Main `ai-app` executable entrypoint.
#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    AinAppBundle::from_args().run().await
}
