#![warn(missing_docs)]

//! A library and CLI to help create, run and interact with aingle conductor setups.
//! **Warning this is still WIP and subject to change**
//! There's probably a few bugs. If you find one please open an [issue](https://github.com/AIngleLab/aingle/issues)
//! or make a PR.
//!
//! ## CLI
//! The `ai` CLI makes it easy to run a saf that you are working on
//! or someone has sent you.
//! It has been designed to use sensible defaults but still give you
//! the configurability when that's required.
//! Setups are stored in tmp directories by default and the paths are
//! persisted in a `.ai` file which is created wherever you are using
//! the CLI.
//! ### Install
//! #### Requirements
//! - [Rust](https://rustup.rs/)
//! - [AIngle](https://github.com/AIngleLab/aingle) binary on the path
//! #### Building
//! From github:
//! ```shell
//! cargo install aingle_cli --git https://github.com/AIngleLab/aingle
//! ```
//! From the aingle repo:
//! ```shell
//! cargo install --path crates/ai
//! ```
//! ### Common usage
//! The best place to start is:
//! ```shell
//! ai -h
//! ```
//! This will be more up to date then this readme.
//! #### Run
//! This command can be used to generate and run conductor setups.
//! ```shell
//! ai run -h
//! # or shorter
//! ai r -h
//! ```
//!  In a folder with where your `my-saf.saf` is you can generate and run
//!  a new setup with:
//! ```shell
//! ai r
//! ```
//! If you have already created a setup previously then it will be reused
//! (usually cleared on reboots).
//! #### Generate
//! Generates new conductor setups and installs apps / safs.
//! ```shell
//! ai generate
//! # or shorter
//! ai g
//! ```
//! For example this will generate 5 setups with app ids set to `my-app`
//! using the `elemental-chat.saf` from the current directory with a quic
//! network setup to localhost.
//! _You don't need to specify safs when they are in the directory._
//! ```shell
//!  ai gen -a "my-app" -n 5 ./elemental-chat.saf network quic
//! ```
//! You can also generate and run in the same command:
//! (Notice the number of conductors and saf path must come before the gen sub-command).
//! ```shell
//!  ai r -n 5 ./elemental-chat.saf gen -a "my-app" network quic
//! ```
//! #### Call
//! Allows calling the [`AdminRequest`] api.
//! If the conductors are not already running they
//! will be run to make the call.
//!
//! ```shell
//! ai call list-cells
//! ```
//! #### List and Clean
//! These commands allow you to list the persisted setups
//! in the current directory (from the`.ai`) file.
//! You can use the index from:
//! ```shell
//! ai list
//! ```
//! Output:
//! ```shell
//! ai-sandbox:
//! Setups contained in `.ai`
//! 0: /tmp/KOXgKVLBVvoxe8iKD4iSS
//! 1: /tmp/m8VHwwt93Uh-nF-vr6nf6
//! 2: /tmp/t6adQomMLI5risj8K2Tsd
//! ```
//! To then call or run an individual setup (or subset):
//!
//! ```shell
//! ai r -i=0,2
//! ```
//! You can clean up these setups with:
//! ```shell
//! ai clean 0 2
//! # Or clean all
//! ai clean
//! ```
//! ## Library
//! This crate can also be used as a library so you can create more
//! complex setups / admin calls.
//! See the docs:
//! ```shell
//! cargo doc --open
//! ```
//! and the examples.

use aingle_cli_bundle as ai_bundle;
use aingle_cli_sandbox as ai_sandbox;
use structopt::StructOpt;

/// AIngle CLI
///
/// Work with SAF and hApp bundle files, set up sandbox environments for testing
/// and development purposes, make direct admin calls to running conductors,
/// and more.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::InferSubcommands)]
pub enum Opt {
    /// Work with hApp bundles
    App(ai_bundle::AinAppBundle),
    /// Work with SAF bundles
    Saf(ai_bundle::AinSafBundle),
    /// Work with sandboxed environments for testing and development
    Sandbox(ai_sandbox::AinSandbox),
}

impl Opt {
    /// Run this command
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::App(cmd) => cmd.run().await?,
            Self::Saf(cmd) => cmd.run().await?,
            Self::Sandbox(cmd) => cmd.run().await?,
        }
        Ok(())
    }
}
