#![forbid(missing_docs)]
//! Binary `ai-saf` command executable.

use aingle_types::prelude::{AppManifest, SafManifest};
use aingle_util::ffs;
use mr_bundle::Manifest;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;

use crate::error::AinBundleResult;

/// The file extension to use for SAF bundles
pub const SAF_BUNDLE_EXT: &str = "saf";

/// The file extension to use for hApp bundles
pub const APP_BUNDLE_EXT: &str = "happ";

/// Work with AIngle SAF bundles
#[derive(Debug, StructOpt)]
pub enum AinSafBundle {
    /// Create a new, empty AIngle SAF bundle working directory and create a new
    /// sample `saf.yaml` manifest inside.
    /// .
    Init {
        /// The path to create the working directory
        path: PathBuf,
    },

    /// Pack into the `[name].saf` bundle according to the `saf.yaml` manifest,
    /// found inside the working directory. The `[name]` is taken from the `name`
    /// property of the manifest file.
    ///
    /// e.g.:
    ///
    /// $ ai saf pack ./some/directory/foo
    ///
    /// creates a file `./some/directory/foo/[name].saf`, based on
    /// `./some/directory/foo/saf.yaml`
    Pack {
        /// The path to the working directory containing a `saf.yaml` manifest
        path: std::path::PathBuf,

        /// Specify the output path for the packed bundle file
        ///
        /// If not specified, the `[name].saf` bundle will be placed inside the
        /// provided working directory.
        #[structopt(short = "o", long)]
        output: Option<PathBuf>,
    },

    /// Unpack parts of the `.saf` bundle file into a specific directory.
    ///
    /// e.g.:
    ///
    /// $ ai saf unpack ./some/dir/my-saf.saf
    ///
    /// creates a new directory `./some/dir/my-saf`, containining a new `saf.yaml`
    /// manifest
    // #[structopt(short = "u", long)]
    Unpack {
        /// The path to the bundle to unpack
        path: std::path::PathBuf,

        /// Specify the directory for the unpacked content
        ///
        /// If not specified, the directory will be placed alongside the
        /// bundle file, with the same name as the bundle file name.
        #[structopt(short = "o", long)]
        output: Option<PathBuf>,

        /// Overwrite an existing directory, if one exists
        #[structopt(short = "f", long)]
        force: bool,
    },
}

/// Work with AIngle hApp bundles
#[derive(Debug, StructOpt)]
pub enum AinAppBundle {
    /// Create a new, empty AIngle app (hApp) working directory and create a new
    /// sample `happ.yaml` manifest inside.
    Init {
        /// The path to create the working directory
        path: PathBuf,
    },

    /// Pack into the `[name].happ` bundle according to the `happ.yaml` manifest,
    /// found inside the working directory. The `[name]` is taken from the `name`
    /// property of the manifest file.
    ///
    /// e.g.:
    ///
    /// $ ai app pack ./some/directory/foo
    ///
    /// creates a file `./some/directory/foo/[name].happ`, based on
    /// `./some/directory/foo/happ.yaml`
    Pack {
        /// The path to the working directory containing a `happ.yaml` manifest
        path: std::path::PathBuf,

        /// Specify the output path for the packed bundle file
        ///
        /// If not specified, the `[name].happ` bundle will be placed inside the
        /// provided working directory.
        #[structopt(short = "o", long)]
        output: Option<PathBuf>,
    },

    /// Unpack parts of the `.happ` bundle file into a specific directory.
    ///
    /// e.g.:
    ///
    /// $ ai app unpack ./some/dir/my-app.happ
    ///
    /// creates a new directory `./some/dir/my-app`, containining a new `happ.yaml`
    /// manifest
    // #[structopt(short = "u", long)]
    Unpack {
        /// The path to the bundle to unpack
        path: std::path::PathBuf,

        /// Specify the directory for the unpacked content
        ///
        /// If not specified, the directory will be placed alongside the
        /// bundle file, with the same name as the bundle file name.
        #[structopt(short = "o", long)]
        output: Option<PathBuf>,

        /// Overwrite an existing directory, if one exists
        #[structopt(short = "f", long)]
        force: bool,
    },
}

impl AinSafBundle {
    /// Run this command
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Init { path } => {
                crate::init::init_saf(path).await?;
            }
            Self::Pack { path, output } => {
                let name = get_saf_name(&path).await?;
                let (bundle_path, _) =
                    crate::packing::pack::<SafManifest>(&path, output, name).await?;
                println!("Wrote bundle {}", bundle_path.to_string_lossy());
            }
            Self::Unpack {
                path,
                output,
                force,
            } => {
                let dir_path =
                    crate::packing::unpack::<SafManifest>(SAF_BUNDLE_EXT, &path, output, force)
                        .await?;
                println!("Unpacked to directory {}", dir_path.to_string_lossy());
            }
        }
        Ok(())
    }
}

impl AinAppBundle {
    /// Run this command
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Init { path } => {
                crate::init::init_app(path).await?;
            }
            Self::Pack { path, output } => {
                let name = get_app_name(&path).await?;
                let (bundle_path, _) =
                    crate::packing::pack::<AppManifest>(&path, output, name).await?;
                println!("Wrote bundle {}", bundle_path.to_string_lossy());
            }
            Self::Unpack {
                path,
                output,
                force,
            } => {
                let dir_path =
                    crate::packing::unpack::<AppManifest>(APP_BUNDLE_EXT, &path, output, force)
                        .await?;
                println!("Unpacked to directory {}", dir_path.to_string_lossy());
            }
        }
        Ok(())
    }
}

async fn get_saf_name(manifest_path: &Path) -> AinBundleResult<String> {
    let manifest_path = manifest_path.to_path_buf();
    let manifest_path = manifest_path.join(SafManifest::path());
    let manifest_yaml = ffs::read_to_string(&manifest_path).await?;
    let manifest: SafManifest = serde_yml::from_str(&manifest_yaml)?;
    Ok(manifest.name())
}

async fn get_app_name(manifest_path: &Path) -> AinBundleResult<String> {
    let manifest_path = manifest_path.to_path_buf();
    let manifest_path = manifest_path.join(AppManifest::path());
    let manifest_yaml = ffs::read_to_string(&manifest_path).await?;
    let manifest: AppManifest = serde_yml::from_str(&manifest_yaml)?;
    Ok(manifest.app_name().to_string())
}
