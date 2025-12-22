use ai_hash::SafHash;
use aingle_sqlite::rusqlite::named_params;
use aingle_sqlite::rusqlite::OptionalExtension;
use aingle_sqlite::rusqlite::Transaction;
use aingle_types::prelude::SafDef;
use aingle_types::prelude::SafDefHashed;

use crate::mutations;
use crate::prelude::from_blob;
use crate::prelude::StateMutationResult;
use crate::prelude::StateQueryResult;

pub fn get(txn: &Transaction<'_>, hash: &SafHash) -> StateQueryResult<Option<SafDefHashed>> {
    let item = txn
        .query_row(
            "SELECT hash, blob FROM SafDef WHERE hash = :hash",
            named_params! {
                ":hash": hash
            },
            |row| {
                let hash: SafHash = row.get("hash")?;
                let wasm = row.get("blob")?;
                Ok((hash, wasm))
            },
        )
        .optional()?;
    match item {
        Some((hash, wasm)) => Ok(Some(SafDefHashed::with_pre_hashed(from_blob(wasm)?, hash))),
        None => Ok(None),
    }
}

pub fn get_all(txn: &Transaction<'_>) -> StateQueryResult<Vec<SafDefHashed>> {
    let mut stmt = txn.prepare(
        "
            SELECT hash, blob FROM SafDef
        ",
    )?;
    let items = stmt
        .query_and_then([], |row| {
            let hash: SafHash = row.get("hash")?;
            let wasm = row.get("blob")?;
            StateQueryResult::Ok(SafDefHashed::with_pre_hashed(from_blob(wasm)?, hash))
        })?
        .collect();
    items
}

pub fn contains(txn: &Transaction<'_>, hash: &SafHash) -> StateQueryResult<bool> {
    Ok(txn.query_row(
        "SELECT EXISTS(SELECT 1 FROM SafDef WHERE hash = :hash)",
        named_params! {
            ":hash": hash
        },
        |row| row.get(0),
    )?)
}

pub fn put(txn: &mut Transaction, saf_def: SafDef) -> StateMutationResult<()> {
    mutations::insert_saf_def(txn, SafDefHashed::from_content_sync(saf_def))
}
