use adk::prelude::*;

#[adk_extern]
fn sys_time(_: ()) -> ExternResult<core::time::Duration> {
    adk::prelude::sys_time()
}

#[cfg(test)]
pub mod test {

    #[test]
    fn sys_time_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        mock_adk
            .expect_sys_time()
            .with(adk::prelude::mockall::predicate::eq(()))
            .times(1)
            .return_once(|_| Ok(core::time::Duration::new(5, 0)));

        adk::prelude::set_adk(mock_adk);

        let result = super::sys_time(());

        assert_eq!(result, Ok(core::time::Duration::new(5, 0)))
    }
}
