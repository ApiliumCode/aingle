// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Keystore backed by lair_keystore_client.

use crate::*;
use lair_keystore_api::*;

/// Spawn a new keystore backed by lair_keystore_client.
pub async fn spawn_lair_keystore(
    lair_dir: Option<&std::path::Path>,
) -> KeystoreApiResult<KeystoreSender> {
    let mut config = Config::builder();
    if let Some(lair_dir) = lair_dir {
        config = config.set_root_path(lair_dir);
    }
    let config = config.build();

    // TODO - actual passphrase handling
    let passphrase = sodoken::BufRead::from(Vec::from("[blank-passphrase]"));

    let api = lair_keystore_client::assert_running_lair_and_connect(config, passphrase).await?;

    Ok(api)
}
