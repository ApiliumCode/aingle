// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Re-export types from the current version.
//! Simply adjust this import when using a new version.

pub use super::app_manifest_v1::{
    AppManifestV1 as AppManifestCurrent, AppManifestV1Builder as AppManifestCurrentBuilder, *,
};
