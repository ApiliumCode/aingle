use adk::prelude::*;
use aingle_test_wasm_common::*;

entry_defs![Anchor::entry_def()];

#[adk_extern]
fn anchor(input: AnchorInput) -> ExternResult<EntryHash> {
    adk::prelude::anchor(input.0, input.1)
}

#[adk_extern]
fn anchor_many(inputs: ManyAnchorInput) -> ExternResult<Vec<EntryHash>> {
    let mut out = Vec::with_capacity(inputs.0.len());
    for input in inputs.0 {
        out.push(adk::prelude::anchor(input.0, input.1)?);
    }
    Ok(out)
}

#[adk_extern]
fn get_anchor(address: EntryHash) -> ExternResult<Option<Anchor>> {
    adk::prelude::get_anchor(address)
}

#[adk_extern]
fn list_anchor_type_addresses(_: ()) -> ExternResult<Vec<EntryHash>> {
    adk::prelude::list_anchor_type_addresses()
}

#[adk_extern]
fn list_anchor_addresses(anchor_type: String) -> ExternResult<Vec<EntryHash>> {
    adk::prelude::list_anchor_addresses(anchor_type)
}

#[adk_extern]
fn list_anchor_tags(anchor_type: String) -> ExternResult<Vec<String>> {
    adk::prelude::list_anchor_tags(anchor_type)
}
