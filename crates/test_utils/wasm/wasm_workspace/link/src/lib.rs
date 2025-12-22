use adk::prelude::*;

entry_defs![Path::entry_def()];

fn path(s: &str) -> ExternResult<EntryHash> {
    let path = Path::from(s);
    path.ensure()?;
    path.hash()
}

fn base() -> ExternResult<EntryHash> {
    path("a")
}

fn target() -> ExternResult<EntryHash> {
    path("b")
}

#[adk_extern]
fn create_link(_: ()) -> ExternResult<HeaderHash> {
    adk::prelude::create_link(base()?, target()?, ())
}

#[adk_extern]
fn delete_link(input: HeaderHash) -> ExternResult<HeaderHash> {
    adk::prelude::delete_link(input)
}

#[adk_extern]
fn get_links(_: ()) -> ExternResult<Links> {
    adk::prelude::get_links(base()?, None)
}

#[adk_extern]
fn delete_all_links(_: ()) -> ExternResult<()> {
    for link in adk::prelude::get_links(base()?, None)?.into_inner() {
        adk::prelude::delete_link(link.create_link_hash)?;
    }
    Ok(())
}

/// Same as path.ensure() but doesn't check for
/// exists. This can happen when ensuring paths
/// in partitions.
#[adk_extern]
fn commit_existing_path(_: ()) -> ExternResult<()> {
    let path = Path::from("a.c");
    create_entry(&path)?;
    if let Some(parent) = path.parent() {
        parent.ensure()?;
        adk::prelude::create_link(parent.hash()?, path.hash()?, LinkTag::try_from(&path)?)?;
    }
    Ok(())
}

#[adk_extern]
fn get_long_path(_: ()) -> ExternResult<Links> {
    Path::from("a").children()
}
