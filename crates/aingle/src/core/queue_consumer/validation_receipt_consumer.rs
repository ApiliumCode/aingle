//! The workflow and queue consumer for validation receipt

use super::*;
use crate::conductor::manager::ManagedTaskResult;
use crate::core::workflow::validation_receipt_workflow::validation_receipt_workflow;
use tokio::task::JoinHandle;
use tracing::*;

/// Spawn the QueueConsumer for validation receipt workflow
///
/// With AI integration enabled, this consumer passes the conductor API to the workflow
/// for adaptive consensus determination. Operations with local consensus requirements
/// will skip external receipt sending, reducing network overhead.
#[instrument(skip(env, conductor_handle, stop, cell_network, conductor_api))]
pub fn spawn_validation_receipt_consumer(
    env: EnvWrite,
    conductor_handle: ConductorHandle,
    mut stop: sync::broadcast::Receiver<()>,
    mut cell_network: AIngleP2pCell,
    conductor_api: impl CellConductorApiT + 'static,
) -> (TriggerSender, JoinHandle<ManagedTaskResult>) {
    let (tx, mut rx) = TriggerSender::new();
    let mut trigger_self = tx.clone();
    let handle = tokio::spawn(async move {
        loop {
            // Wait for next job
            if let Job::Shutdown = next_job_or_exit(&mut rx, &mut stop).await {
                tracing::warn!(
                    "Cell is shutting down: stopping validation_receipt_workflow queue consumer."
                );
                break;
            }

            // Run the workflow with conductor API for adaptive consensus
            match validation_receipt_workflow(env.clone(), &mut cell_network, conductor_api.clone())
                .await
            {
                Ok(WorkComplete::Incomplete) => trigger_self.trigger(),
                Err(err) => {
                    handle_workflow_error(
                        conductor_handle.clone(),
                        cell_network.cell_id(),
                        err,
                        "validation_receipt_workflow failure",
                    )
                    .await?
                }
                _ => (),
            };
        }
        Ok(())
    });
    (tx, handle)
}
