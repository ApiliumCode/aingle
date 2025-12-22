use adk::prelude::*;

#[adk_extern]
fn foo(_: ()) -> ExternResult<String> {
    Ok(String::from("foo"))
}

#[adk_extern]
fn bar(_: ()) -> ExternResult<String> {
    // It should be possible to call our extern functions just like regular functions.
    #[allow(clippy::blacklisted_name)]
    let foo: String = foo(())?;
    Ok(format!("{}{}", foo, "bar"))
}
