// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

use std::path::{Path, PathBuf};

#[cfg(feature = "packing")]
use crate::error::{UnpackingError, UnpackingResult};

/// Removes a subpath suffix from a path
#[cfg(feature = "packing")]
pub fn prune_path<P: AsRef<Path>>(mut path: PathBuf, subpath: P) -> UnpackingResult<PathBuf> {
    if path.ends_with(&subpath) {
        for _ in subpath.as_ref().components() {
            let _ = path.pop();
        }
        Ok(path)
    } else {
        Err(UnpackingError::ManifestPathSuffixMismatch(
            path,
            subpath.as_ref().to_owned(),
        ))
    }
}
