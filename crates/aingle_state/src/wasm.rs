use ai_hash::WasmHash;
use aingle_sqlite::rusqlite::named_params;
use aingle_sqlite::rusqlite::OptionalExtension;
use aingle_sqlite::rusqlite::Transaction;
use aingle_types::prelude::*;

use crate::mutations;
use crate::prelude::from_blob;
use crate::prelude::StateMutationResult;
use crate::prelude::StateQueryResult;

pub fn get(txn: &Transaction<'_>, hash: &WasmHash) -> StateQueryResult<Option<SafWasmHashed>> {
    let item = txn
        .query_row(
            "SELECT hash, blob FROM Wasm WHERE hash = :hash",
            named_params! {
                ":hash": hash
            },
            |row| {
                let hash: WasmHash = row.get("hash")?;
                let wasm = row.get("blob")?;
                Ok((hash, wasm))
            },
        )
        .optional()?;
    match item {
        Some((hash, wasm)) => Ok(Some(SafWasmHashed::with_pre_hashed(from_blob(wasm)?, hash))),
        None => Ok(None),
    }
}

pub fn contains(txn: &Transaction<'_>, hash: &WasmHash) -> StateQueryResult<bool> {
    Ok(txn.query_row(
        "SELECT EXISTS(SELECT 1 FROM Wasm WHERE hash = :hash)",
        named_params! {
            ":hash": hash
        },
        |row| row.get(0),
    )?)
}

pub fn put(txn: &mut Transaction, wasm: SafWasmHashed) -> StateMutationResult<()> {
    mutations::insert_wasm(txn, wasm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_hash::HasHash;
    use aingle_sqlite::prelude::DatabaseResult;
    use aingle_types::saf::wasm::SafWasm;

    #[tokio::test(flavor = "multi_thread")]
    async fn wasm_store_round_trip() -> DatabaseResult<()> {
        use aingle_sqlite::prelude::*;
        observability::test_run().ok();

        // all the stuff needed to have a WasmBuf
        let env = crate::test_utils::test_wasm_env();

        // a wasm
        let wasm =
            SafWasmHashed::from_content(SafWasm::from(aingle_wasm_test_utils::TestWasm::Foo)).await;

        // Put wasm
        env.conn()?
            .with_commit_sync(|txn| put(txn, wasm.clone()))
            .unwrap();
        fresh_reader_test!(env, |txn| {
            assert!(contains(&txn, &wasm.as_hash()).unwrap());
            // a wasm from the WasmBuf
            let ret = get(&txn, &wasm.as_hash()).unwrap().unwrap();

            // assert the round trip
            assert_eq!(ret, wasm);
        });

        Ok(())
    }
}
