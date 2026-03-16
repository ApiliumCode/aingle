// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! DAG action synchronization over P2P.
//!
//! Extends the gossip loop with tip-based DAG sync: nodes exchange their
//! current DAG tips and request missing actions from peers.

#[cfg(feature = "dag")]
use aingle_graph::dag::DagActionHash;
#[cfg(feature = "dag")]
use aingle_graph::GraphDB;

/// Collects the local DAG tips as hex strings for tip exchange.
#[cfg(feature = "dag")]
pub fn collect_local_tips(graph: &GraphDB) -> (Vec<String>, u64) {
    if let Some(dag_store) = graph.dag_store() {
        let tips = dag_store
            .tips_raw()
            .unwrap_or_default()
            .into_iter()
            .map(|h| hex::encode(h))
            .collect::<Vec<_>>();
        let count = dag_store.action_count() as u64;
        (tips, count)
    } else {
        (Vec::new(), 0)
    }
}

/// Given remote tips, compute which actions we have that the remote is missing,
/// and return them as serialized bytes ready for sending.
#[cfg(feature = "dag")]
pub fn compute_missing_from_tips(
    graph: &GraphDB,
    remote_tips: &[String],
) -> Vec<Vec<u8>> {
    let Some(dag_store) = graph.dag_store() else {
        return Vec::new();
    };

    let remote_hashes: Vec<DagActionHash> = remote_tips
        .iter()
        .filter_map(|h| {
            let bytes = hex::decode(h).ok()?;
            if bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Some(DagActionHash(arr))
            } else {
                None
            }
        })
        .collect();

    dag_store
        .compute_missing(&remote_hashes)
        .unwrap_or_default()
        .into_iter()
        .map(|action| action.to_bytes())
        .collect()
}

/// Fetch serialized DAG actions by their hex hashes for sending to a peer.
#[cfg(feature = "dag")]
pub fn fetch_actions_by_hash(
    graph: &GraphDB,
    hashes: &[String],
) -> Vec<Vec<u8>> {
    let Some(dag_store) = graph.dag_store() else {
        return Vec::new();
    };

    hashes
        .iter()
        .filter_map(|hex_hash| {
            let bytes = hex::decode(hex_hash).ok()?;
            if bytes.len() != 32 {
                return None;
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            let action = dag_store.get(&DagActionHash(arr)).ok()??;
            Some(action.to_bytes())
        })
        .collect()
}

/// Ingest received DAG actions into the local store.
#[cfg(feature = "dag")]
pub fn ingest_actions(
    graph: &GraphDB,
    action_bytes_list: &[Vec<u8>],
) -> (usize, usize) {
    let Some(dag_store) = graph.dag_store() else {
        return (0, action_bytes_list.len());
    };

    let mut ingested = 0;
    let mut errors = 0;

    for action_bytes in action_bytes_list {
        use aingle_graph::dag::DagAction;
        if let Some(action) = DagAction::from_bytes(action_bytes) {
            match dag_store.ingest(&action) {
                Ok(_) => ingested += 1,
                Err(e) => {
                    tracing::debug!("DAG ingest error: {e}");
                    errors += 1;
                }
            }
        } else {
            errors += 1;
        }
    }

    (ingested, errors)
}
