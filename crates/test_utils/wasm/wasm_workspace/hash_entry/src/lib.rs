use adk::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
enum TemperatureUnit {
    Kelvin,
    Farenheit,
    Celcius,
}

#[adk_entry(id = "temperature")]
struct Temperature(u32, TemperatureUnit);

entry_defs![Temperature::entry_def()];

fn temperature() -> Temperature {
    Temperature(32, TemperatureUnit::Celcius)
}

#[adk_extern]
fn twenty_three_degrees_entry_hash(_: ()) -> ExternResult<EntryHash> {
    let temp = temperature();
    let header_hash: HeaderHash = create_entry(&temp)?;
    let element: Element = get(header_hash, GetOptions::content())?.unwrap();
    match element.entry() {
        ElementEntry::Present(entry) => adk::prelude::hash_entry(entry.clone()),
        _ => unreachable!(),
    }
}

#[adk_extern]
fn twenty_three_degrees_hash(_: ()) -> ExternResult<EntryHash> {
    adk::prelude::hash_entry(&temperature())
}

#[adk_extern]
fn hash_entry(entry: Entry) -> ExternResult<EntryHash> {
    adk::prelude::hash_entry(entry)
}

#[cfg(test)]
pub mod tests {
    use ::ai_fixt::prelude::*;
    use adk::prelude::*;

    #[test]
    fn hash_entry_smoke() {
        let mut mock_adk = adk::prelude::MockAdkT::new();

        let input_entry = fixt!(Entry);
        let output_hash = fixt!(EntryHash);
        let output_hash_closure = output_hash.clone();
        mock_adk
            .expect_hash_entry()
            .with(adk::prelude::mockall::predicate::eq(input_entry.clone()))
            .times(1)
            .return_once(move |_| Ok(output_hash_closure));

        adk::prelude::set_adk(mock_adk);

        let result = super::hash_entry(input_entry);

        assert_eq!(result, Ok(output_hash))
    }
}
