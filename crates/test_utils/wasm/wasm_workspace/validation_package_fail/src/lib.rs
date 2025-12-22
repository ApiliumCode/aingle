use adk::prelude::*;

#[adk_extern]
fn validation_package(_: AppEntryType) -> ExternResult<ValidationPackageCallbackResult> {
    Ok(ValidationPackageCallbackResult::Fail("bad package".into()))
}
