use adk::prelude::*;

#[adk_entry(id = "thing")]
struct Thing;

entry_defs![Thing::entry_def()];

#[adk_extern]
fn create(_: ()) -> ExternResult<HeaderHash> {
    create_entry(&Thing)
}

/// `read` seems to be a reserved worked that causes SIGSEGV invalid memory reference when used as `#[adk_extern]`
#[adk_extern]
fn reed(header_hash: HeaderHash) -> ExternResult<Option<Element>> {
    get(header_hash, GetOptions::latest())
}

#[adk_extern]
fn delete(header_hash: HeaderHash) -> ExternResult<HeaderHash> {
    delete_entry(header_hash)
}

#[cfg(test)]
pub mod test {
    use ::ai_fixt::prelude::*;
    use adk::prelude::*;

    #[test]
    fn create_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let header_hash = fixt!(HeaderHash);
        let closure_header_hash = header_hash.clone();
        mock_adk
            .expect_create()
            .with(adk::prelude::mockall::predicate::eq(
                EntryWithDefId::try_from(&super::Thing).unwrap(),
            ))
            .times(1)
            .return_once(move |_| Ok(closure_header_hash));

        adk::prelude::set_adk(mock_adk);

        let result = super::create(());

        assert_eq!(result, Ok(header_hash))
    }

    #[test]
    fn get_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let input_header_hash = fixt!(HeaderHash);
        mock_adk
            .expect_get()
            .with(adk::prelude::mockall::predicate::eq(GetInput::new(
                input_header_hash.clone().into(),
                GetOptions::latest(),
            )))
            .times(1)
            .return_once(move |_| Ok(None));

        adk::prelude::set_adk(mock_adk);

        let result = super::reed(input_header_hash);

        assert_eq!(result, Ok(None))
    }

    #[test]
    fn delete_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let input_header_hash = fixt!(HeaderHash);
        let output_header_hash = fixt!(HeaderHash);
        let output_header_hash_closure = output_header_hash.clone();
        mock_adk
            .expect_delete()
            .with(adk::prelude::mockall::predicate::eq(
                input_header_hash.clone(),
            ))
            .times(1)
            .return_once(move |_| Ok(output_header_hash_closure));

        adk::prelude::set_adk(mock_adk);

        let result = super::delete(input_header_hash);

        assert_eq!(result, Ok(output_header_hash))
    }
}
