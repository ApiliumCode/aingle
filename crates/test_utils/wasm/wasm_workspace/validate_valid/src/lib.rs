use adk::prelude::*;

#[adk_extern]
fn validate(_: ValidateData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}
