use adk::prelude::*;

#[adk_entry(id = "post", required_validations = 5)]
struct Post(String);

#[adk_entry(id = "msg", required_validations = 5)]
struct Msg(String);

entry_defs![Post::entry_def(), Msg::entry_def()];

fn post() -> Post {
    Post("foo".into())
}

fn msg() -> Msg {
    Msg("hi".into())
}

#[adk_extern]
fn create_entry(_: ()) -> ExternResult<HeaderHash> {
    adk::prelude::create_entry(&post())
}

#[adk_extern]
fn get_entry(_: ()) -> ExternResult<Option<Element>> {
    get(hash_entry(&post())?, GetOptions::latest())
}

#[adk_extern]
fn update_entry(_: ()) -> ExternResult<HeaderHash> {
    let header_hash = adk::prelude::create_entry(&post())?;
    adk::prelude::update_entry(header_hash, &post())
}

#[adk_extern]
/// Updates to a different entry, this will fail
fn invalid_update_entry(_: ()) -> ExternResult<HeaderHash> {
    let header_hash = adk::prelude::create_entry(&post())?;
    adk::prelude::update_entry(header_hash, &msg())
}
