use adk::prelude::*;

#[adk_extern]
fn agent_info(_: ()) -> ExternResult<AgentInfo> {
    adk::prelude::agent_info()
}

#[cfg(test)]
pub mod test {
    use ::ai_fixt::prelude::*;
    use adk::prelude::*;

    #[test]
    fn agent_info_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let agent_info = fixt!(AgentInfo);
        let closure_agent_info = agent_info.clone();
        mock_adk
            .expect_agent_info()
            .with(adk::prelude::mockall::predicate::eq(()))
            .times(1)
            .return_once(move |_| Ok(closure_agent_info));

        adk::prelude::set_adk(mock_adk);

        let result = super::agent_info(());

        assert_eq!(result, Ok(agent_info))
    }
}
