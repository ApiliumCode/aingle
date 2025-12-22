use crate::header::ZomeId;
use crate::zome::ZomeName;
use ai_hash::SafHash;
use aingle_middleware_bytes::prelude::*;

/// The properties of the current saf/zome being called.
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes, PartialEq)]
pub struct ZomeInfo {
    pub saf_name: String,
    pub saf_hash: SafHash,
    pub zome_name: ZomeName,
    /// The position of this zome in the `saf.yaml`
    pub zome_id: ZomeId,
    pub properties: SerializedBytes,
}
