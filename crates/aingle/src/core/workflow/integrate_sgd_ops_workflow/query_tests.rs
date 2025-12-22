use std::collections::HashMap;
use std::collections::HashSet;

use ai_hash::HasHash;
use aingle_sqlite::db::WriteManager;
use aingle_sqlite::prelude::DatabaseResult;
use aingle_state::prelude::*;
use aingle_types::sgd_op::SgdOpHashed;
use aingle_zome_types::fixt::*;
use fixt::prelude::*;

use super::*;

struct Expected {
    hashes: HashSet<SgdOpHash>,
    ops: HashMap<SgdOpHash, SgdOpHashed>,
}

struct SharedData {
    seq: u32,
    prev_hash: HeaderHash,
    original_header: HeaderHash,
}
#[derive(Debug, Clone, Copy, Default)]
struct Facts {
    store_element: bool,
    register_activity: bool,
    update_element: bool,
    deleted_by: bool,
    integrated: bool,
    sequential: bool,
    original_header: bool,
    awaiting_integration: bool,
}

#[tokio::test(flavor = "multi_thread")]
async fn integrate_query() {
    observability::test_run().ok();
    let env = test_cell_env();
    let expected = test_data(&env.env().into());
    let (qt, _rx) = TriggerSender::new();
    let (qt2, _rx) = TriggerSender::new();
    // dump_tmp(&env.env());
    integrate_sgd_ops_workflow(env.env().into(), qt, qt2)
        .await
        .unwrap();
    let hashes = env
        .conn()
        .unwrap()
        .with_reader(|txn| {
            let mut stmt =
                txn.prepare("SELECT hash FROM SgdOp WHERE when_integrated IS NOT NULL")?;
            let hashes: HashSet<SgdOpHash> = stmt
                .query_map([], |row| {
                    let hash: SgdOpHash = row.get("hash").unwrap();
                    Ok(hash)
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            DatabaseResult::Ok(hashes)
        })
        .unwrap();
    let diff = hashes.symmetric_difference(&expected.hashes);
    for d in diff {
        tracing::debug!(?d, missing = ?expected.ops.get(d));
    }
    assert_eq!(hashes, expected.hashes);
}

fn create_and_insert_op(env: &EnvRead, facts: Facts, data: &mut SharedData) -> SgdOpHashed {
    let mut update = fixt!(Update);
    if facts.original_header && facts.update_element {
        update.original_header_address = data.original_header.clone();
    }

    if facts.sequential {
        update.header_seq = data.seq;
        update.prev_header = data.prev_hash.clone();
        data.seq += 1;
        data.prev_hash = HeaderHash::with_data_sync(&Header::Update(update.clone()));
    }

    let header = Header::Update(update.clone());
    data.original_header = HeaderHash::with_data_sync(&header);
    let state = if facts.register_activity {
        SgdOpHashed::from_content_sync(SgdOp::RegisterAgentActivity(
            fixt!(Signature),
            header.clone(),
        ))
    } else if facts.store_element {
        SgdOpHashed::from_content_sync(SgdOp::StoreElement(fixt!(Signature), header.clone(), None))
    } else if facts.update_element {
        SgdOpHashed::from_content_sync(SgdOp::RegisterUpdatedElement(
            fixt!(Signature),
            update.clone(),
            None,
        ))
    } else {
        unreachable!()
    };

    env.conn()
        .unwrap()
        .with_commit_sync(|txn| {
            let hash = state.as_hash().clone();
            insert_op(txn, state.clone(), false).unwrap();
            set_validation_status(txn, hash.clone(), ValidationStatus::Valid).unwrap();
            if facts.integrated {
                set_when_integrated(txn, hash.clone(), aingle_types::timestamp::now()).unwrap();
            }
            if facts.awaiting_integration {
                set_validation_stage(
                    txn,
                    hash.clone(),
                    ValidationLimboStatus::AwaitingIntegration,
                )
                .unwrap();
            }
            DatabaseResult::Ok(())
        })
        .unwrap();
    state
}

fn test_data(env: &EnvRead) -> Expected {
    let mut hashes = HashSet::new();
    let mut ops = HashMap::new();

    let mut data = SharedData {
        seq: 0,
        prev_hash: fixt!(HeaderHash),
        original_header: fixt!(HeaderHash),
    };
    let mut facts = Facts {
        register_activity: true,
        integrated: true,
        sequential: true,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    hashes.insert(op.as_hash().clone());
    ops.insert(op.as_hash().clone(), op);

    facts.integrated = false;
    facts.awaiting_integration = true;
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    hashes.insert(op.as_hash().clone());
    ops.insert(op.as_hash().clone(), op);

    let facts = Facts {
        store_element: true,
        integrated: false,
        awaiting_integration: true,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    hashes.insert(op.as_hash().clone());
    ops.insert(op.as_hash().clone(), op);

    let facts = Facts {
        register_activity: true,
        integrated: false,
        awaiting_integration: true,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    ops.insert(op.as_hash().clone(), op);

    // Original header but dep not integrated
    let facts = Facts {
        store_element: true,
        integrated: false,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    ops.insert(op.as_hash().clone(), op);

    let facts = Facts {
        update_element: true,
        original_header: false,
        integrated: false,
        awaiting_integration: true,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    ops.insert(op.as_hash().clone(), op);

    // Original header
    let facts = Facts {
        store_element: true,
        integrated: true,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    hashes.insert(op.as_hash().clone());
    ops.insert(op.as_hash().clone(), op);

    let facts = Facts {
        update_element: true,
        original_header: true,
        integrated: false,
        awaiting_integration: true,
        ..Default::default()
    };
    let op = create_and_insert_op(env, facts, &mut data);
    tracing::debug!(hash = ?op.as_hash());
    hashes.insert(op.as_hash().clone());
    ops.insert(op.as_hash().clone(), op);
    Expected { hashes, ops }
}
