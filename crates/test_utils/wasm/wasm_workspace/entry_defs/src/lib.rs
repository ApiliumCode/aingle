use adk::prelude::*;

#[adk_entry(id = "post")]
struct Post;

#[adk_entry(id = "comment", visibility = "private")]
struct Comment;

entry_defs![Post::entry_def(), Comment::entry_def()];

#[adk_extern]
pub fn assert_indexes(_: ()) -> ExternResult<()> {
    assert_eq!(EntryDefIndex(0), entry_def_index!(Post)?);
    assert_eq!(EntryDefIndex(1), entry_def_index!(Comment)?);
    Ok(())
}
