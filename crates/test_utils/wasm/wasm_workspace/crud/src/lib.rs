use adk::prelude::*;
mod countree;

entry_defs![countree::CounTree::entry_def()];

#[adk_extern]
fn new(_: ()) -> ExternResult<HeaderHash> {
    countree::CounTree::new()
}

#[adk_extern]
fn header_details(header_hash: HeaderHash) -> ExternResult<Option<Details>> {
    countree::CounTree::header_details(header_hash)
}

#[adk_extern]
fn entry_details(entry_hash: EntryHash) -> ExternResult<Option<Details>> {
    countree::CounTree::entry_details(entry_hash)
}

#[adk_extern]
fn entry_hash(countree: crate::countree::CounTree) -> ExternResult<EntryHash> {
    hash_entry(&countree)
}

#[adk_extern]
fn inc(header_hash: HeaderHash) -> ExternResult<HeaderHash> {
    countree::CounTree::incsert(header_hash)
}

#[adk_extern]
fn dec(header_hash: HeaderHash) -> ExternResult<HeaderHash> {
    countree::CounTree::dec(header_hash)
}
