use adk::prelude::*;

#[adk_extern]
fn random_bytes(bytes: u32) -> ExternResult<Bytes> {
    adk::prelude::random_bytes(bytes)
}

#[cfg(test)]
pub mod tests {
    #[test]
    fn random_bytes_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let input = 1;
        let output = adk::prelude::Bytes::from(vec![4_u8]);
        let output_closure = output.clone();
        mock_adk
            .expect_random_bytes()
            .with(adk::prelude::mockall::predicate::eq(input))
            .times(1)
            .return_once(move |_| Ok(output_closure));

        adk::prelude::set_adk(mock_adk);

        let result = super::random_bytes(input);

        assert_eq!(result, Ok(output));
    }
}
