//! Definitions of StructOpt options for use in the CLI

use crate::cmds::*;
use aingle_types::prelude::InstalledAppId;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;

const DEFAULT_APP_ID: &str = "test-app";

#[derive(Debug, StructOpt)]
/// Helper for generating, running, and interacting with AIngle Conductor "sandboxes".
///
/// A sandbox is a directory containing a conductor config, databases, and keystore,
/// with a single AIngle app installed in the conductor:
/// Everything you need to quickly run your app in aingle,
/// or create complex multi-conductor sandboxes for testing.
pub struct AinSandbox {
    #[structopt(subcommand)]
    command: AinSandboxSubcommand,
    /// Force the admin port that ai uses to talk to aingle to a specific value.
    /// For example `ai -f=9000,9001 run`
    /// This must be set on each run or the port will change if it's in use.
    #[structopt(short, long, value_delimiter = ",")]
    force_admin_ports: Vec<u16>,
    /// Set the path to the aingle binary.
    #[structopt(short, long, env = "AI_AINGLE_PATH", default_value = "aingle")]
    aingle_path: PathBuf,
}

/// The list of subcommands for `ai sandbox`
#[derive(Debug, StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::InferSubcommands)]
pub enum AinSandboxSubcommand {
    /// Generate one or more new AIngle Conductor sandbox(es) for later use.
    ///
    /// A single app will be installed as part of this sandbox.
    /// See the help for the `<safs>` argument below to learn how to define the app to be installed.
    Generate {
        #[structopt(short, long, default_value = DEFAULT_APP_ID)]
        /// ID for the installed app.
        /// This is just a String to identify the app by.
        app_id: InstalledAppId,

        /// (flattened)
        #[structopt(flatten)]
        create: Create,

        /// Automatically run the sandbox(es) that were created.
        /// This is effectively a combination of `ai generate` and `ai run`
        ///
        /// You may optionally specify app interface ports to bind when running.
        /// This allows your UI to talk to the conductor.
        ///
        /// For example, `ai generate -r=0,9000,0` will create three app interfaces.
        /// Or, use `ai generate -r` to run without attaching any app interfaces.
        ///
        /// This follows the same structure as `ai run --ports`
        #[structopt(short, long, value_delimiter = ",")]
        run: Option<Vec<u16>>,

        /// A hApp bundle to install.
        happ: Option<PathBuf>,
    },
    /// Run conductor(s) from existing sandbox(es).
    Run(Run),

    /// Make a call to a conductor's admin interface.
    Call(crate::calls::Call),

    /// List sandboxes found in `$(pwd)/.ai`.
    List {
        /// Show more verbose information.
        #[structopt(short, long, parse(from_occurrences))]
        verbose: usize,
    },

    /// Clean (completely remove) sandboxes that are listed in the `$(pwd)/.ai` file.
    Clean,

    /// Create a fresh sandbox with no apps installed.
    Create(Create),
}

/// Options for running a sandbox
#[derive(Debug, StructOpt)]
pub struct Run {
    /// Optionally specifies app interface ports to bind when running.
    /// This allows your UI to talk to the conductor.
    /// For example, `ai -p=0,9000,0` will create three app interfaces.
    /// Important: Interfaces are persistent. If you add an interface
    /// it will be there next time you run the conductor.
    #[structopt(short, long, value_delimiter = ",")]
    ports: Vec<u16>,

    /// (flattened)
    #[structopt(flatten)]
    existing: Existing,
}

impl AinSandbox {
    /// Run this command
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            AinSandboxSubcommand::Generate {
                app_id,
                create,
                run,
                happ,
            } => {
                let paths = generate(&self.aingle_path, happ, create, app_id).await?;
                for (port, path) in self
                    .force_admin_ports
                    .clone()
                    .into_iter()
                    .zip(paths.clone().into_iter())
                {
                    crate::force_admin_port(path, port)?;
                }
                if let Some(ports) = run {
                    let aingle_path = self.aingle_path.clone();
                    let force_admin_ports = self.force_admin_ports.clone();
                    tokio::task::spawn(async move {
                        if let Err(e) = run_n(&aingle_path, paths, ports, force_admin_ports).await {
                            tracing::error!(failed_to_run = ?e);
                        }
                    });
                    tokio::signal::ctrl_c().await?;
                    crate::save::release_ports(std::env::current_dir()?).await?;
                }
            }
            AinSandboxSubcommand::Run(Run { ports, existing }) => {
                let paths = existing.load()?;
                if paths.is_empty() {
                    return Ok(());
                }
                let aingle_path = self.aingle_path.clone();
                let force_admin_ports = self.force_admin_ports.clone();
                tokio::task::spawn(async move {
                    if let Err(e) = run_n(&aingle_path, paths, ports, force_admin_ports).await {
                        tracing::error!(failed_to_run = ?e);
                    }
                });
                tokio::signal::ctrl_c().await?;
                crate::save::release_ports(std::env::current_dir()?).await?;
            }
            AinSandboxSubcommand::Call(call) => crate::calls::call(&self.aingle_path, call).await?,
            // AinSandboxSubcommand::Task => todo!("Running custom tasks is coming soon"),
            AinSandboxSubcommand::List { verbose } => {
                crate::save::list(std::env::current_dir()?, verbose)?
            }
            AinSandboxSubcommand::Clean => {
                crate::save::clean(std::env::current_dir()?, Vec::new())?
            }
            AinSandboxSubcommand::Create(Create {
                num_sandboxes,
                network,
                root,
                directories,
            }) => {
                let mut paths = Vec::with_capacity(num_sandboxes);
                msg!(
                    "Creating {} conductor sandboxes with same settings",
                    num_sandboxes
                );
                for i in 0..num_sandboxes {
                    let path = crate::generate::generate(
                        network.clone().map(|n| n.into_inner().into()),
                        root.clone(),
                        directories.get(i).cloned(),
                    )?;
                    paths.push(path);
                }
                crate::save::save(std::env::current_dir()?, paths.clone())?;
                msg!("Created {:?}", paths);
            }
        }

        Ok(())
    }
}

async fn run_n(
    aingle_path: &Path,
    paths: Vec<PathBuf>,
    app_ports: Vec<u16>,
    force_admin_ports: Vec<u16>,
) -> anyhow::Result<()> {
    let run_aingle = |aingle_path: PathBuf, path: PathBuf, ports, force_admin_port| async move {
        crate::run::run(&aingle_path, path, ports, force_admin_port).await?;
        Result::<_, anyhow::Error>::Ok(())
    };
    let mut force_admin_ports = force_admin_ports.into_iter();
    let mut app_ports = app_ports.into_iter();
    let jhs = paths
        .into_iter()
        .zip(std::iter::repeat_with(|| force_admin_ports.next()))
        .zip(std::iter::repeat_with(|| app_ports.next()))
        .map(|((path, force_admin_port), app_port)| {
            let f = run_aingle(
                aingle_path.to_path_buf(),
                path,
                app_port.map(|p| vec![p]).unwrap_or_default(),
                force_admin_port,
            );
            tokio::task::spawn(f)
        });
    futures::future::try_join_all(jhs).await?;
    Ok(())
}

async fn generate(
    aingle_path: &Path,
    happ: Option<PathBuf>,
    create: Create,
    app_id: InstalledAppId,
) -> anyhow::Result<Vec<PathBuf>> {
    let happ = crate::bundles::parse_happ(happ)?;
    let paths = crate::sandbox::default_n(aingle_path, create, happ, app_id).await?;
    crate::save::save(std::env::current_dir()?, paths.clone())?;
    Ok(paths)
}
