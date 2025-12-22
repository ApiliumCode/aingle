use adk::prelude::*;

#[adk_extern]
fn emit(_: ()) -> ExternResult<()> {
    emit_signal(&())?;
    Ok(())
}

#[adk_extern]
fn signal_others(signal: RemoteSignal) -> ExternResult<()> {
    remote_signal(&signal.signal, signal.agents)
}

#[adk_extern]
fn recv_remote_signal(signal: ExternIO) -> ExternResult<()> {
    ADK.with(|h| h.borrow().emit_signal(AppSignal::new(signal)))
}

#[adk_extern]
fn init(_: ()) -> ExternResult<InitCallbackResult> {
    let mut functions: GrantedFunctions = BTreeSet::new();
    functions.insert((zome_info()?.zome_name, "recv_remote_signal".into()));
    create_cap_grant(CapGrantEntry {
        tag: "".into(),
        // empty access converts to unrestricted
        access: ().into(),
        functions,
    })?;

    Ok(InitCallbackResult::Pass)
}
