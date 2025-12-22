use adk::prelude::*;

#[adk_extern]
fn validate_agent(_: Element) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

#[adk_extern]
fn validate(_: ValidateData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid("esoteric edge case".into()))
}
