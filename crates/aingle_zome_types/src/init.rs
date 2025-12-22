use crate::zome_io::ExternIO;
use crate::CallbackResult;
use ai_hash::EntryHash;
use aingle_middleware_bytes::prelude::*;

#[derive(Clone, PartialEq, Serialize, Deserialize, SerializedBytes, Debug)]
pub enum InitCallbackResult {
    Pass,
    Fail(String),
    UnresolvedDependencies(Vec<EntryHash>),
}

impl From<ExternIO> for InitCallbackResult {
    fn from(callback_guest_output: ExternIO) -> Self {
        match callback_guest_output.decode() {
            Ok(v) => v,
            Err(e) => Self::Fail(format!("{:?}", e)),
        }
    }
}

impl CallbackResult for InitCallbackResult {
    fn is_definitive(&self) -> bool {
        matches!(self, InitCallbackResult::Fail(_))
    }
}
