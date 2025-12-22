use aingle_types::prelude::*;
use aingle_util::ffs;
use assert_cmd::prelude::*;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn read_app(path: &Path) -> anyhow::Result<AppBundle> {
    Ok(AppBundle::decode(&ffs::sync::read(path).unwrap())?)
}

fn read_saf(path: &Path) -> anyhow::Result<SafBundle> {
    Ok(SafBundle::decode(&ffs::sync::read(path).unwrap())?)
}

#[tokio::test]
async fn roundtrip() {
    {
        let mut cmd = Command::cargo_bin("ai-saf").unwrap();
        let cmd = cmd.args(&["pack", "tests/fixtures/my-app/safs/saf1"]);
        cmd.assert().success();
    }
    {
        let mut cmd = Command::cargo_bin("ai-saf").unwrap();
        let cmd = cmd.args(&["pack", "tests/fixtures/my-app/safs/saf2"]);
        cmd.assert().success();
    }
    {
        let mut cmd = Command::cargo_bin("ai-app").unwrap();
        let cmd = cmd.args(&["pack", "tests/fixtures/my-app/"]);
        cmd.assert().success();
    }

    let app_path = PathBuf::from("tests/fixtures/my-app/fixture-app.happ");
    let saf1_path = PathBuf::from("tests/fixtures/my-app/safs/saf1/a saf.saf");
    let saf2_path = PathBuf::from("tests/fixtures/my-app/safs/saf2/another saf.saf");

    let _original_happ = read_app(&app_path).unwrap();
    let _original_saf1 = read_saf(&saf1_path).unwrap();
    let _original_saf2 = read_saf(&saf2_path).unwrap();
}
