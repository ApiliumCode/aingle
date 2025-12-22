use adk::prelude::*;

#[adk_extern]
fn zome_info(_: ()) -> ExternResult<ZomeInfo> {
    adk::prelude::zome_info()
}

#[cfg(test)]
pub mod tests {
    use ::fixt::prelude::*;
    use adk::prelude::*;

    #[test]
    fn zome_info_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let output = fixt!(ZomeInfo);
        let output_closure = output.clone();
        mock_adk
            .expect_zome_info()
            .with(adk::prelude::mockall::predicate::eq(()))
            .times(1)
            .return_once(move |_| Ok(output_closure));

        adk::prelude::set_adk(mock_adk);

        let result = super::zome_info(());

        assert_eq!(result, Ok(output));
    }
}
