use adk::prelude::*;

#[adk_extern]
fn post_commit(_: HeaderHashes) -> ExternResult<PostCommitCallbackResult> {
    Ok(PostCommitCallbackResult::Success)
}
