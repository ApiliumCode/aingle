//! Common helpers for writing tests against zome types
//!
//! We don't use fixturators for these, because this crate defines no fixturators

use crate::capability::CapSecret;
use crate::capability::CAP_SECRET_BYTES;
use crate::cell::CellId;
use ai_hash::hash_type;
use ai_hash::*;
use aingle_middleware_bytes::prelude::*;

fn fake_ai_hash<T: ai_hash::HashType>(name: u8, hash_type: T) -> AiHash<T> {
    AiHash::from_raw_36_and_type([name; AI_HASH_UNTYPED_LEN].to_vec(), hash_type)
}

/// A fixture SafHash for unit testing.
pub fn fake_saf_hash(name: u8) -> SafHash {
    fake_ai_hash(name, hash_type::Saf::new())
}

/// A fixture HeaderHash for unit testing.
pub fn fake_header_hash(name: u8) -> HeaderHash {
    fake_ai_hash(name, hash_type::Header::new())
}

/// A fixture SgdOpHash for unit testing.
pub fn fake_sgd_op_hash(name: u8) -> SgdOpHash {
    fake_ai_hash(name, hash_type::SgdOp::new())
}

/// A fixture EntryHash for unit testing.
pub fn fake_entry_hash(name: u8) -> EntryHash {
    fake_ai_hash(name, hash_type::Entry::new())
}

/// A fixture AgentPubKey for unit testing.
pub fn fake_agent_pub_key(name: u8) -> AgentPubKey {
    fake_ai_hash(name, hash_type::Agent::new())
}

/// A fixture AgentPubKey for unit testing.
/// NB: This must match up with AgentPubKeyFixturator's Predictable curve
pub fn fake_agent_pubkey_1() -> AgentPubKey {
    AgentPubKey::try_from("uhCAkmrkoAHPVf_eufG7eC5fm6QKrW5pPMoktvG5LOC0SnJ4vV1Uv").unwrap()
}

/// Another fixture AgentPubKey for unit testing.
/// NB: This must match up with AgentPubKeyFixturator's Predictable curve
pub fn fake_agent_pubkey_2() -> AgentPubKey {
    AgentPubKey::try_from("uhCAke1j8Z2a-_min0h0pGuEMcYlo_V1l1mt9OtBuywKmHlg4L_R-").unwrap()
}

/// A fixture CapSecret for unit testing.
pub fn fake_cap_secret() -> CapSecret {
    [0; CAP_SECRET_BYTES].into()
}

/// A fixture example CellId for unit testing.
pub fn fake_cell_id(name: u8) -> CellId {
    (fake_saf_hash(name), fake_agent_pubkey_1()).into()
}
