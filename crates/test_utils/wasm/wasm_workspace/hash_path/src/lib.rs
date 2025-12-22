use adk::prelude::*;

entry_defs![Path::entry_def()];

#[adk_extern]
fn hash(path_string: String) -> ExternResult<EntryHash> {
    Path::from(path_string).hash()
}

#[adk_extern]
fn exists(path_string: String) -> ExternResult<bool> {
    Path::from(path_string).exists()
}

#[adk_extern]
fn ensure(path_string: String) -> ExternResult<()> {
    Path::from(path_string).ensure()
}

#[adk_extern]
fn delete_link(delete_link: HeaderHash) -> ExternResult<HeaderHash> {
    adk::prelude::delete_link(delete_link)
}

#[adk_extern]
fn children(path_string: String) -> ExternResult<Links> {
    Path::from(path_string).children()
}

#[adk_extern]
fn children_details(path_string: String) -> ExternResult<LinkDetails> {
    Path::from(path_string).children_details()
}
