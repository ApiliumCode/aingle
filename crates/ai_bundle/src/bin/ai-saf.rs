// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use aingle_cli_bundle::AinSafBundle;
use structopt::StructOpt;

/// Main `ai-saf` executable entrypoint.
#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    AinSafBundle::from_args().run().await
}
