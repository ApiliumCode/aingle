use crate::mutations::*;
use ai_hash::HasHash;
use aingle_sqlite::rusqlite::Transaction;
use aingle_types::sgd_op::SgdOpHashed;
use aingle_types::timestamp;

pub fn insert_valid_authored_op(txn: &mut Transaction, op: SgdOpHashed) -> StateMutationResult<()> {
    let hash = op.as_hash().clone();
    insert_op(txn, op, true)?;
    set_validation_status(txn, hash, aingle_zome_types::ValidationStatus::Valid)?;

    Ok(())
}

pub fn insert_valid_integrated_op(
    txn: &mut Transaction,
    op: SgdOpHashed,
) -> StateMutationResult<()> {
    let hash = op.as_hash().clone();
    insert_op(txn, op, false)?;
    set_validation_status(
        txn,
        hash.clone(),
        aingle_zome_types::ValidationStatus::Valid,
    )?;
    set_when_integrated(txn, hash, timestamp::now())?;

    Ok(())
}
