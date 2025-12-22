use adk::prelude::*;

#[adk_extern]
fn migrate_agent(_: MigrateAgent) -> ExternResult<MigrateAgentCallbackResult> {
    Ok(MigrateAgentCallbackResult::Fail("no migrate".into()))
}
