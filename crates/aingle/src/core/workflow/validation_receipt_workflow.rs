use aingle_p2p::AIngleP2pCell;
use aingle_p2p::AIngleP2pCellT;
use aingle_state::prelude::*;
use aingle_types::prelude::*;
use aingle_zome_types::TryInto;
use tracing::*;

#[cfg(feature = "ai-integration")]
use std::convert::TryInto as _;

use crate::conductor::api::CellConductorApiT;
use crate::core::queue_consumer::WorkComplete;

#[cfg(feature = "ai-integration")]
use crate::conductor::ai_service::consensus_level_to_receipt_ratio;

use super::error::WorkflowResult;

#[cfg(test)]
mod tests;

#[instrument(skip(vault, network, conductor_api))]
/// Send validation receipts to their authors in serial and without waiting for
/// responses.
///
/// With AI integration enabled, this workflow uses adaptive consensus to determine
/// which receipts to send. Operations with `ConsensusLevel::Local` don't require
/// external validation receipts, reducing network overhead.
///
/// TODO: Currently still waiting for responses because we don't have a network call
/// that doesn't.
pub async fn validation_receipt_workflow<Api: CellConductorApiT>(
    vault: EnvWrite,
    network: &mut AIngleP2pCell,
    #[allow(unused_variables)] conductor_api: Api,
) -> WorkflowResult<WorkComplete> {
    // Get the env and keystore
    let keystore = vault.keystore();
    // Who we are.
    let validator = network.from_agent();

    // Get out all ops that are marked for sending receipt.
    // FIXME: Test this query.
    // Note: With AI integration, we also fetch timestamp data for consensus analysis.
    let receipts = vault
        .async_reader({
            let validator = validator.clone();
            move |txn| {
                let mut stmt = txn.prepare(
                    "
            SELECT Header.author, SgdOp.hash, SgdOp.validation_status,
            SgdOp.when_integrated_ns, Header.type as header_type
            From SgdOp
            JOIN Header ON SgdOp.header_hash = Header.hash
            WHERE
            SgdOp.require_receipt = 1
            AND
            SgdOp.when_integrated_ns IS NOT NULL
            AND
            SgdOp.validation_status IS NOT NULL
            ",
                )?;
                let ops = stmt
                    .query_and_then([], |r| {
                        let author: AgentPubKey = r.get("author")?;
                        let sgd_op_hash: SgdOpHash = r.get("hash")?;
                        let validation_status = r.get("validation_status")?;
                        let when_integrated = from_blob::<Timestamp>(r.get("when_integrated_ns")?)?;
                        let header_type: Option<String> = r.get("header_type").ok();
                        StateQueryResult::Ok((
                            ValidationReceipt {
                                sgd_op_hash: sgd_op_hash.clone(),
                                validation_status,
                                validator: validator.clone(),
                                when_integrated,
                            },
                            author,
                            sgd_op_hash,
                            when_integrated,
                            header_type,
                        ))
                    })?
                    .collect::<StateQueryResult<Vec<_>>>()?;
                StateQueryResult::Ok(ops)
            }
        })
        .await?;

    // Counters for adaptive consensus metrics
    #[cfg(feature = "ai-integration")]
    let mut skipped_local = 0u64;
    #[cfg(feature = "ai-integration")]
    let mut sent_receipts = 0u64;

    // Send the validation receipts
    for (receipt, author, op_hash_for_ai, when_integrated, header_type) in receipts {
        // Don't send receipt to self.
        if author == validator {
            continue;
        }

        let op_hash = receipt.sgd_op_hash.clone();

        // AI Adaptive Consensus: Determine if this receipt needs to be sent
        #[cfg(feature = "ai-integration")]
        {
            // Convert data for AI analysis
            let hash: [u8; 32] = op_hash_for_ai.get_raw_32().try_into().unwrap_or([0u8; 32]);
            let timestamp = when_integrated.to_sql_ms_lossy() as u64;
            let agent: [u8; 32] = author.get_raw_32().try_into().unwrap_or([0u8; 32]);
            let entry_type = header_type.unwrap_or_else(|| "unknown".to_string());

            // Get AI consensus level determination
            let consensus_level = conductor_api
                .ai_determine_consensus_level(
                    hash,
                    timestamp,
                    agent,
                    entry_type,
                    Vec::new(), // No additional data needed for consensus determination
                )
                .await;

            // Check if we need external receipts based on consensus level
            let receipt_ratio = consensus_level_to_receipt_ratio(consensus_level.clone());

            if receipt_ratio == 0.0 {
                // Local consensus - no external validation needed
                trace!(
                    op_hash = ?op_hash,
                    consensus_level = ?consensus_level,
                    "AI determined Local consensus - skipping receipt send"
                );
                skipped_local += 1;

                // Mark as not requiring receipt since local validation is sufficient
                vault
                    .async_commit(|txn| set_require_receipt(txn, op_hash, false))
                    .await?;
                continue;
            }

            debug!(
                op_hash = ?op_hash,
                consensus_level = ?consensus_level,
                receipt_ratio = receipt_ratio,
                "AI consensus level determined - sending receipt"
            );
            sent_receipts += 1;
        }

        // Sign on the dotted line.
        let receipt = receipt.sign(&keystore).await?;

        // Send it and don't wait for response.
        // TODO: When networking has a send without response we can use that
        // instead of waiting for response.
        if let Err(e) = network
            .send_validation_receipt(author, receipt.try_into()?)
            .await
        {
            // No one home, they will need to publish again.
            info!(failed_send_receipt = ?e);
        }
        // Attempted to send the receipt so we now mark
        // it to not send in the future.
        vault
            .async_commit(|txn| set_require_receipt(txn, op_hash, false))
            .await?;
    }

    // Log adaptive consensus metrics
    #[cfg(feature = "ai-integration")]
    if skipped_local > 0 || sent_receipts > 0 {
        info!(
            skipped_local = skipped_local,
            sent_receipts = sent_receipts,
            "Validation receipt workflow completed with adaptive consensus"
        );
    }

    Ok(WorkComplete::Complete)
}
