//! Helpers for working with saf files.
use std::path::Path;
use std::path::PathBuf;

use anyhow::bail;
use anyhow::ensure;
use walkdir::WalkDir;

/// Parse a list of safs.
/// If paths are directories then each directory
/// will be searched for the first file that matches
/// `*.saf`.
pub fn parse_safs(mut safs: Vec<PathBuf>) -> anyhow::Result<Vec<PathBuf>> {
    if safs.is_empty() {
        safs.push(std::env::current_dir()?);
    }
    for saf in safs.iter_mut() {
        if saf.is_dir() {
            let file_path = search_for_saf(saf)?;
            *saf = file_path;
        }
        ensure!(
            saf.file_name()
                .map(|f| f.to_string_lossy().ends_with(".saf"))
                .unwrap_or(false),
            "File {} is not a valid saf file name: (e.g. my-saf.saf)",
            saf.display()
        );
    }
    Ok(safs)
}

/// Parse a happ bundle.
/// If paths are directories then each directory
/// will be searched for the first file that matches
/// `*.saf`.
pub fn parse_happ(happ: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let mut happ = happ.unwrap_or(std::env::current_dir()?);
    if happ.is_dir() {
        let file_path = search_for_happ(&happ)?;
        happ = file_path;
    }
    ensure!(
        happ.file_name()
            .map(|f| f.to_string_lossy().ends_with(".happ"))
            .unwrap_or(false),
        "File {} is not a valid happ file name: (e.g. my-happ.happ)",
        happ.display()
    );
    Ok(happ)
}

// TODO: Look for multiple safs
fn search_for_saf(saf: &Path) -> anyhow::Result<PathBuf> {
    let dir: Vec<_> = WalkDir::new(saf)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|d| d.file_type().is_file())
        .filter(|f| f.file_name().to_string_lossy().ends_with(".saf"))
        .map(|f| f.into_path())
        .collect();
    if dir.len() != 1 {
        bail!(
            "Could not find a SAF file (e.g. my-saf.saf) in directory {}",
            saf.display()
        )
    }
    Ok(dir.into_iter().next().expect("Safe due to check above"))
}

fn search_for_happ(happ: &Path) -> anyhow::Result<PathBuf> {
    let dir = WalkDir::new(happ)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|d| d.file_type().is_file())
        .find(|f| f.file_name().to_string_lossy().ends_with(".happ"))
        .map(|f| f.into_path());
    match dir {
        Some(dir) => Ok(dir),
        None => {
            bail!(
                "Could not find a happ file (e.g. my-happ.happ) in directory {}",
                happ.display()
            )
        }
    }
}
