//! Common use sandboxes with lots of default choices.
use std::path::Path;
use std::path::PathBuf;

use aingle_types::prelude::InstalledAppId;

use crate::calls::InstallAppBundle;
use crate::cmds::*;
use crate::run::run_async;
use crate::CmdRunner;

/// Generates a new sandbox with a default [`ConductorConfig`]
/// and optional network.
/// Then installs the safs with a new app per saf.
pub async fn default_with_network(
    aingle_path: &Path,
    create: Create,
    directory: Option<PathBuf>,
    happ: PathBuf,
    app_id: InstalledAppId,
) -> anyhow::Result<PathBuf> {
    let Create { network, root, .. } = create;
    let path = crate::generate::generate(network.map(|n| n.into_inner().into()), root, directory)?;
    let conductor = run_async(aingle_path, path.clone(), None).await?;
    let mut cmd = CmdRunner::new(conductor.0).await;
    let install_bundle = InstallAppBundle {
        app_id: Some(app_id),
        agent_key: None,
        path: happ,
        uid: None,
    };
    crate::calls::install_app_bundle(&mut cmd, install_bundle).await?;
    Ok(path)
}

/// Same as [`default_with_network`] but creates n copies
/// of this sandbox in their own directories.
pub async fn default_n(
    aingle_path: &Path,
    create: Create,
    happ: PathBuf,
    app_id: InstalledAppId,
) -> anyhow::Result<Vec<PathBuf>> {
    let num_sandboxes = create.num_sandboxes;
    msg!(
        "Creating {} conductor sandboxes with same settings",
        num_sandboxes
    );
    let mut paths = Vec::with_capacity(num_sandboxes);
    for i in 0..num_sandboxes {
        let p = default_with_network(
            aingle_path,
            create.clone(),
            create.directories.get(i).cloned(),
            happ.clone(),
            app_id.clone(),
        )
        .await?;
        paths.push(p);
    }
    msg!("Created {:?}", paths);
    Ok(paths)
}
