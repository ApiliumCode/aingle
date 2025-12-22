use super::*;
use crate::error::AiHashError;
use std::convert::TryInto;

#[cfg(all(test, feature = "serialized-bytes"))]
use aingle_middleware_bytes::prelude::*;

/// The AnySgd (composite) HashType
#[derive(
    Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(all(test, feature = "serialized-bytes"), derive(SerializedBytes))]
#[serde(from = "AnySgdSerial", into = "AnySgdSerial")]
pub enum AnySgd {
    /// The hash of an Entry
    Entry,
    /// The hash of a Header
    Header,
}

impl HashType for AnySgd {
    fn get_prefix(self) -> &'static [u8] {
        match self {
            AnySgd::Entry => Entry::new().get_prefix(),
            AnySgd::Header => Header::new().get_prefix(),
        }
    }

    fn try_from_prefix(prefix: &[u8]) -> AiHashResult<Self> {
        match prefix {
            primitive::ENTRY_PREFIX => Ok(AnySgd::Entry),
            primitive::HEADER_PREFIX => Ok(AnySgd::Header),
            _ => Err(AiHashError::BadPrefix(
                "AnySgd".to_string(),
                prefix.try_into().expect("3 byte prefix"),
            )),
        }
    }

    fn hash_name(self) -> &'static str {
        "AnySgdHash"
    }
}

impl HashTypeAsync for AnySgd {}

#[derive(serde::Deserialize, serde::Serialize)]
enum AnySgdSerial {
    /// The hash of an Entry of EntryType::Agent
    Header(Header),
    /// The hash of any other EntryType
    Entry(Entry),
}

impl From<AnySgd> for AnySgdSerial {
    fn from(t: AnySgd) -> Self {
        match t {
            AnySgd::Header => AnySgdSerial::Header(Header),
            AnySgd::Entry => AnySgdSerial::Entry(Entry),
        }
    }
}

impl From<AnySgdSerial> for AnySgd {
    fn from(t: AnySgdSerial) -> Self {
        match t {
            AnySgdSerial::Header(_) => AnySgd::Header,
            AnySgdSerial::Entry(_) => AnySgd::Entry,
        }
    }
}
