//! P2P networking for Cortex triple synchronization.
//!
//! Enables multi-node knowledge graph sync via QUIC transport,
//! bloom filter gossip, and optional mDNS discovery.

pub mod config;
pub mod discovery;
pub mod gossip;
pub mod identity;
pub mod manager;
pub mod message;
pub mod peer_store;
pub mod rate_limiter;
pub mod sync_manager;
pub mod transport;
