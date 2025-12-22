use crate::entry::Entry;
use crate::header::CreateLink;
use crate::header::DeleteLink;
use crate::zome_io::ExternIO;
use crate::CallbackResult;
use ai_hash::AnySgdHash;
use aingle_middleware_bytes::prelude::*;

#[derive(Serialize, Deserialize, SerializedBytes, Debug)]
pub struct ValidateCreateLinkData {
    pub link_add: CreateLink,
    pub base: Entry,
    pub target: Entry,
}

#[derive(Serialize, Deserialize, SerializedBytes, Debug)]
pub struct ValidateDeleteLinkData {
    pub delete_link: DeleteLink,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SerializedBytes)]
pub enum ValidateLinkCallbackResult {
    Valid,
    Invalid(String),
    UnresolvedDependencies(Vec<AnySgdHash>),
}

impl CallbackResult for ValidateLinkCallbackResult {
    fn is_definitive(&self) -> bool {
        matches!(self, ValidateLinkCallbackResult::Invalid(_))
    }
}

impl From<ExternIO> for ValidateLinkCallbackResult {
    fn from(guest_output: ExternIO) -> Self {
        match guest_output.decode() {
            Ok(v) => v,
            Err(e) => Self::Invalid(format!("{:?}", e)),
        }
    }
}
