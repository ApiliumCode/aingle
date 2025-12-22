use adk::prelude::*;

#[adk_extern]
fn genesis_self_check(data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    let GenesisSelfCheckData {
        saf_def: _,
        membrane_proof: _,
        agent_key: _,
    } = data;
    Ok(ValidateCallbackResult::Valid)
}
