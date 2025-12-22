use ai_hash::SgdOpHash;
use aingle_state::query::prelude::*;
use aingle_types::env::EnvRead;
use aingle_types::sgd_op::SgdOp;
use aingle_types::sgd_op::SgdOpHashed;
use aingle_types::sgd_op::SgdOpType;
use aingle_zome_types::Entry;
use aingle_zome_types::SignedHeader;

use crate::core::workflow::error::WorkflowResult;

/// Get all ops that need to sys or app validated in order.
/// - Sys validated or awaiting app dependencies.
/// - Ordered by type then timestamp (See [`SgdOpOrder`])
pub async fn get_ops_to_app_validate(env: &EnvRead) -> WorkflowResult<Vec<SgdOpHashed>> {
    get_ops_to_validate(env, false).await
}

/// Get all ops that need to sys or app validated in order.
/// - Pending or awaiting sys dependencies.
/// - Ordered by type then timestamp (See [`SgdOpOrder`])
pub async fn get_ops_to_sys_validate(env: &EnvRead) -> WorkflowResult<Vec<SgdOpHashed>> {
    get_ops_to_validate(env, true).await
}

async fn get_ops_to_validate(env: &EnvRead, system: bool) -> WorkflowResult<Vec<SgdOpHashed>> {
    let mut sql = "
        SELECT 
        Header.blob as header_blob,
        Entry.blob as entry_blob,
        SgdOp.type as sgd_type,
        SgdOp.hash as sgd_hash
        FROM Header
        JOIN
        SgdOp ON SgdOp.header_hash = Header.hash
        LEFT JOIN
        Entry ON Header.entry_hash = Entry.hash
        "
    .to_string();
    if system {
        sql.push_str(
            "
            WHERE
            (SgdOp.validation_status IS NULL OR SgdOp.validation_stage = 0)
            ",
        );
    } else {
        sql.push_str(
            "
            WHERE
            (SgdOp.validation_stage = 1 OR SgdOp.validation_stage = 2)
            ",
        );
    }
    sql.push_str(
        "
        ORDER BY 
        SgdOp.op_order ASC
        ",
    );
    env.async_reader(move |txn| {
        let mut stmt = txn.prepare(&sql)?;
        let r = stmt.query_and_then([], |row| {
            let header = from_blob::<SignedHeader>(row.get("header_blob")?)?;
            let op_type: SgdOpType = row.get("sgd_type")?;
            let hash: SgdOpHash = row.get("sgd_hash")?;
            let entry: Option<Vec<u8>> = row.get("entry_blob")?;
            let entry = match entry {
                Some(entry) => Some(from_blob::<Entry>(entry)?),
                None => None,
            };
            WorkflowResult::Ok(SgdOpHashed::with_pre_hashed(
                SgdOp::from_type(op_type, header, entry)?,
                hash,
            ))
        })?;
        let r = r.collect();
        WorkflowResult::Ok(r)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use ai_hash::HasHash;
    use aingle_sqlite::db::WriteManager;
    use aingle_sqlite::prelude::DatabaseResult;
    use aingle_state::prelude::*;
    use aingle_state::validation_db::ValidationLimboStatus;
    use aingle_types::sgd_op::OpOrder;
    use aingle_types::sgd_op::SgdOpHashed;
    use aingle_zome_types::fixt::*;
    use aingle_zome_types::ValidationStatus;
    use fixt::prelude::*;

    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct Facts {
        pending: bool,
        awaiting_sys_deps: bool,
        has_validation_status: bool,
    }

    struct Expected {
        results: Vec<SgdOpHashed>,
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sys_validation_query() {
        observability::test_run().ok();
        let env = test_cell_env();
        let expected = test_data(&env.env().into());
        let r = get_ops_to_validate(&env.env().into(), true).await.unwrap();
        let mut r_sorted = r.clone();
        // Sorted by OpOrder
        r_sorted.sort_by_key(|d| {
            let op_type = d.as_content().get_type();
            let timestamp = d.as_content().header().timestamp();
            OpOrder::new(op_type, timestamp)
        });
        assert_eq!(r, r_sorted);
        for op in r {
            assert!(expected.results.iter().any(|i| *i == op));
        }
    }

    fn create_and_insert_op(env: &EnvRead, facts: Facts) -> SgdOpHashed {
        let state = SgdOpHashed::from_content_sync(SgdOp::RegisterAgentActivity(
            fixt!(Signature),
            fixt!(Header),
        ));

        env.conn()
            .unwrap()
            .with_commit_sync(|txn| {
                let hash = state.as_hash().clone();
                insert_op(txn, state.clone(), false).unwrap();
                if facts.has_validation_status {
                    set_validation_status(txn, hash.clone(), ValidationStatus::Valid).unwrap();
                }
                if facts.pending {
                    // No need to do anything because status and stage are null already.
                } else if facts.awaiting_sys_deps {
                    set_validation_stage(
                        txn,
                        hash,
                        ValidationLimboStatus::AwaitingSysDeps(fixt!(AnySgdHash)),
                    )
                    .unwrap();
                }
                DatabaseResult::Ok(())
            })
            .unwrap();
        state
    }

    fn test_data(env: &EnvRead) -> Expected {
        let mut results = Vec::new();
        // We **do** expect any of these in the results:
        let facts = Facts {
            pending: true,
            awaiting_sys_deps: false,
            has_validation_status: false,
        };
        for _ in 0..20 {
            let op = create_and_insert_op(env, facts);
            results.push(op);
        }

        let facts = Facts {
            pending: false,
            awaiting_sys_deps: true,
            has_validation_status: false,
        };
        for _ in 0..20 {
            let op = create_and_insert_op(env, facts);
            results.push(op);
        }

        // We **don't** expect any of these in the results:
        let facts = Facts {
            pending: false,
            awaiting_sys_deps: false,
            has_validation_status: true,
        };
        for _ in 0..20 {
            create_and_insert_op(env, facts);
        }

        Expected { results }
    }
}
