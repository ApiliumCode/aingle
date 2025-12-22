//! The workflow and queue consumer for SgdOp integration

use super::error::WorkflowResult;
use crate::core::queue_consumer::TriggerSender;
use crate::core::queue_consumer::WorkComplete;
use aingle_state::prelude::*;
use aingle_types::prelude::*;

use tracing::*;

#[cfg(test)]
mod query_tests;
#[cfg(feature = "test_utils")]
mod tests;

#[instrument(skip(vault, trigger_sys, trigger_receipt))]
pub async fn integrate_sgd_ops_workflow(
    vault: EnvWrite,
    mut trigger_sys: TriggerSender,
    mut trigger_receipt: TriggerSender,
) -> WorkflowResult<WorkComplete> {
    let time = aingle_types::timestamp::now();
    let changed = vault
        .async_commit(move |txn| {
            let changed = txn
                .prepare_cached(aingle_sqlite::sql::sql_cell::UPDATE_INTEGRATE_OPS)?
                .execute(named_params! {
                    ":when_integrated": time,
                    ":when_integrated_ns": to_blob(time)?,
                    ":store_entry": SgdOpType::StoreEntry,
                    ":store_element": SgdOpType::StoreElement,
                    ":register_activity": SgdOpType::RegisterAgentActivity,
                    ":updated_content": SgdOpType::RegisterUpdatedContent,
                    ":updated_element": SgdOpType::RegisterUpdatedElement,
                    ":deleted_by": SgdOpType::RegisterDeletedBy,
                    ":deleted_entry_header": SgdOpType::RegisterDeletedEntryHeader,
                    ":create_link": SgdOpType::RegisterAddLink,
                    ":delete_link": SgdOpType::RegisterRemoveLink,

                })?;
            WorkflowResult::Ok(changed)
        })
        .await?;
    tracing::debug!(?changed);
    if changed > 0 {
        trigger_sys.trigger();
        trigger_receipt.trigger();
        Ok(WorkComplete::Incomplete)
    } else {
        Ok(WorkComplete::Complete)
    }
}
