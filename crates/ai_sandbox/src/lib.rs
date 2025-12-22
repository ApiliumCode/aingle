#![warn(missing_docs)]

//! A library and CLI to help create, run and interact with aingle conductor sandboxes.
//! **Warning this is still WIP and subject to change**
//! There's probably a few bugs. If you find one please open an [issue](https://github.com/AIngleLab/aingle/issues)
//! or make a PR.
//!
//! ## CLI
//! The `ai sandbox` CLI makes it easy to run a saf that you are working on
//! or someone has sent you.
//! It has been designed to use sensible defaults but still give you
//! the configurability when that's required.
//! Sandboxes are stored in tmp directories by default and the paths are
//! persisted in a `.aiXXX` file which is created wherever you are using
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
//! cargo install --path crates/aiYYY
//! ```
//! ### Common usage
//! The best place to start is:
//! ```shell
//! ai sandbox -h
//! ```
//! This will be more up to date then this readme.
//! #### Run
//! This command can be used to generate and run conductor sandboxes.
//! ```shell
//! ai sandbox run -h
//! # or shorter
//! ai sandbox r -h
//! ```
//!  In a folder with where your `my-saf.saf` is you can generate and run
//!  a new sandbox with:
//! ```shell
//! ai sandbox r
//! ```
//! If you have already created a sandbox previously then it will be reused
//! (usually cleared on reboots).
//! #### Generate
//! Generates new conductor sandboxes and installs apps / safs.
//! ```shell
//! ai sandbox generate
//! # or shorter
//! ai sandbox g
//! ```
//! For example this will generate 5 sandboxes with app ids set to `my-app`
//! using the `elemental-chat.saf` from the current directory with a quic
//! network sandbox to localhost.
//! _You don't need to specify safs when they are in the directory._
//! ```shell
//! ai sandbox gen -a "my-app" -n 5 ./elemental-chat.saf network quic
//! ```
//! You can also generate and run in the same command:
//! (Notice the number of conductors and saf path must come before the gen sub-command).
//! ```shell
//! ai sandbox r -n 5 ./elemental-chat.saf gen -a "my-app" network quic
//! ```
//! #### Call
//! Allows calling the [`AdminRequest`] api.
//! If the conductors are not already running they
//! will be run to make the call.
//!
//! ```shell
//! ai sandbox call list-cells
//! ```
//! #### List and Clean
//! These commands allow you to list the persisted sandboxes
//! in the current directory (from the`.aiXXX`) file.
//! You can use the index from:
//! ```shell
//! ai sandbox list
//! ```
//! Output:
//! ```shell
//! ai-sandbox:
//! Sandboxes contained in `.aiXXX`
//! 0: /tmp/KOXgKVLBVvoxe8iKD4iSS
//! 1: /tmp/m8VHwwt93Uh-nF-vr6nf6
//! 2: /tmp/t6adQomMLI5risj8K2Tsd
//! ```
//! To then call or run an individual sandbox (or subset):
//!
//! ```shell
//! ai sandbox r -i=0,2
//! ```
//! You can clean up these sandboxes with:
//! ```shell
//! ai sandbox clean 0 2
//! # Or clean all
//! ai sandbox clean
//! ```
//! ## Library
//! This crate can also be used as a library so you can create more
//! complex sandboxes / admin calls.
//! See the docs:
//! ```shell
//! cargo doc --open
//! ```
//! and the examples.

#![allow(deprecated)]

use std::path::Path;
use std::path::PathBuf;

use aingle_conductor_api::{AdminRequest, AdminResponse};
use aingle_websocket::WebsocketResult;
use aingle_websocket::WebsocketSender;
use ports::get_admin_api;

pub use ports::force_admin_port;

/// Print a msg with `ai-sandbox: ` pre-pended
/// and ansi colors.
macro_rules! msg {
    ($($arg:tt)*) => ({
        use ansi_term::Color::*;
        print!("{} ", Blue.bold().paint("ai-sandbox:"));
        println!($($arg)*);
    })
}

pub mod bundles;
pub mod calls;
pub mod cli;
#[doc(hidden)]
pub mod cmds;
pub mod config;
pub mod generate;
pub mod run;
pub mod sandbox;
pub mod save;
pub use cli::AinSandbox;

mod ports;

/// An active connection to a running conductor.
pub struct CmdRunner {
    client: WebsocketSender,
}

impl CmdRunner {
    const AINGLE_PATH: &'static str = "aingle";
    /// Create a new connection for calling admin interface commands.
    /// Panics if admin port fails to connect.
    pub async fn new(port: u16) -> Self {
        Self::try_new(port)
            .await
            .expect("Failed to create CmdRunner because admin port failed to connect")
    }

    /// Create a new connection for calling admin interface commands.
    pub async fn try_new(port: u16) -> WebsocketResult<Self> {
        let client = get_admin_api(port).await?;
        Ok(Self { client })
    }

    /// Create a command runner from a sandbox path.
    /// This expects aingle to be on the path.
    pub async fn from_sandbox(
        sandbox_path: PathBuf,
    ) -> anyhow::Result<(Self, tokio::process::Child)> {
        Self::from_sandbox_with_bin_path(Path::new(Self::AINGLE_PATH), sandbox_path).await
    }

    /// Create a command runner from a sandbox path and
    /// set the path to the aingle binary.
    pub async fn from_sandbox_with_bin_path(
        aingle_bin_path: &Path,
        sandbox_path: PathBuf,
    ) -> anyhow::Result<(Self, tokio::process::Child)> {
        let conductor = run::run_async(aingle_bin_path, sandbox_path, None).await?;
        let cmd = CmdRunner::try_new(conductor.0).await?;
        Ok((cmd, conductor.1))
    }

    /// Make an Admin request to this conductor.
    pub async fn command(&mut self, cmd: AdminRequest) -> anyhow::Result<AdminResponse> {
        let response: Result<AdminResponse, _> = self.client.request(cmd).await;
        Ok(response?)
    }
}

#[macro_export]
/// Expect that an enum matches a variant and panic if it doesn't.
macro_rules! expect_variant {
    ($var:expr => $variant:path, $error_msg:expr) => {
        match $var {
            $variant(v) => v,
            _ => panic!(format!("{}: Expected {} but got {:?}", $error_msg, stringify!($variant), $var)),
        }
    };
    ($var:expr => $variant:path) => {
        expect_variant!($var => $variant, "")
    };
}

#[macro_export]
/// Expect that an enum matches a variant and return an error if it doesn't.
macro_rules! expect_match {
    ($var:expr => $variant:path, $error_msg:expr) => {
        match $var {
            $variant(v) => v,
            _ => anyhow::bail!("{}: Expected {} but got {:?}", $error_msg, stringify!($variant), $var),
        }
    };
    ($var:expr => $variant:path) => {
        expect_variant!($var => $variant, "")
    };
}
