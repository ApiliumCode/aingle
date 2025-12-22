//! Data structures representing the operations that can be performed within a AIngle SGD.
//!
//! See the [item-level documentation for `SgdOp`][SgdOp] for more details.
//!
//! [SgdOp]: enum.SgdOp.html

use std::str::FromStr;

use crate::element::ElementGroup;
use crate::header::NewEntryHeader;
use crate::prelude::*;
use ai_hash::hash_type;
use ai_hash::HashableContentBytes;
use aingle_sqlite::rusqlite::types::FromSql;
use aingle_sqlite::rusqlite::ToSql;
use error::SgdOpError;
use error::SgdOpResult;
use serde::Deserialize;
use serde::Serialize;

#[allow(missing_docs)]
pub mod error;

#[cfg(test)]
#[allow(missing_docs)]
mod tests;

/// A unit of SGD gossip. Used to notify an authority of new (meta)data to hold
/// as well as changes to the status of already held data.
#[derive(
    Clone, Debug, Serialize, Deserialize, SerializedBytes, Eq, PartialEq, derive_more::Display,
)]
pub enum SgdOp {
    #[display(fmt = "StoreElement")]
    /// Used to notify the authority for a header that it has been created.
    ///
    /// Conceptually, authorities receiving this `SgdOp` do three things:
    ///
    /// - Ensure that the element passes validation.
    /// - Store the header into their SGD shard.
    /// - Store the entry into their CAS.
    ///   - Note: they do not become responsible for keeping the set of
    ///     references from that entry up-to-date.
    StoreElement(Signature, Header, Option<Box<Entry>>),

    #[display(fmt = "StoreEntry")]
    /// Used to notify the authority for an entry that it has been created
    /// anew. (The same entry can be created more than once.)
    ///
    /// Conceptually, authorities receiving this `SgdOp` do four things:
    ///
    /// - Ensure that the element passes validation.
    /// - Store the entry into their SGD shard.
    /// - Store the header into their CAS.
    ///   - Note: they do not become responsible for keeping the set of
    ///     references from that header up-to-date.
    /// - Add a "created-by" reference from the entry to the hash of the header.
    ///
    /// TODO: document how those "created-by" references are stored in
    /// reality.
    StoreEntry(Signature, NewEntryHeader, Box<Entry>),

    #[display(fmt = "RegisterAgentActivity")]
    /// Used to notify the authority for an agent's public key that that agent
    /// has committed a new header.
    ///
    /// Conceptually, authorities receiving this `SgdOp` do three things:
    ///
    /// - Ensure that *the header alone* passes surface-level validation.
    /// - Store the header into their SGD shard.
    ///   - FIXME: @artbrock, do they?
    /// - Add an "agent-activity" reference from the public key to the hash
    ///   of the header.
    ///
    /// TODO: document how those "agent-activity" references are stored in
    /// reality.
    RegisterAgentActivity(Signature, Header),

    #[display(fmt = "RegisterUpdatedContent")]
    /// Op for updating an entry.
    /// This is sent to the entry authority.
    // TODO: This entry is here for validation by the entry update header holder
    // link's don't do this. The entry is validated by store entry. Maybe we either
    // need to remove the Entry here or add it to link.
    RegisterUpdatedContent(Signature, header::Update, Option<Box<Entry>>),

    #[display(fmt = "RegisterUpdatedElement")]
    /// Op for updating an element.
    /// This is sent to the element authority.
    RegisterUpdatedElement(Signature, header::Update, Option<Box<Entry>>),

    #[display(fmt = "RegisterDeletedBy")]
    /// Op for registering a Header deletion with the Header authority
    RegisterDeletedBy(Signature, header::Delete),

    #[display(fmt = "RegisterDeletedEntryHeader")]
    /// Op for registering a Header deletion with the Entry authority, so that
    /// the Entry can be marked Dead if all of its Headers have been deleted
    RegisterDeletedEntryHeader(Signature, header::Delete),

    #[display(fmt = "RegisterAddLink")]
    /// Op for adding a link
    RegisterAddLink(Signature, header::CreateLink),

    #[display(fmt = "RegisterRemoveLink")]
    /// Op for removing a link
    RegisterRemoveLink(Signature, header::DeleteLink),
}

/// Show that this type is used as the basis
type SgdBasis = AnySgdHash;

/// A type for storing in databases that don't need the actual
/// data. Everything is a hash of the type except the signatures.
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize, derive_more::Display)]
pub enum SgdOpLight {
    #[display(fmt = "StoreElement")]
    StoreElement(HeaderHash, Option<EntryHash>, SgdBasis),
    #[display(fmt = "StoreEntry")]
    StoreEntry(HeaderHash, EntryHash, SgdBasis),
    #[display(fmt = "RegisterAgentActivity")]
    RegisterAgentActivity(HeaderHash, SgdBasis),
    #[display(fmt = "RegisterUpdatedContent")]
    RegisterUpdatedContent(HeaderHash, EntryHash, SgdBasis),
    #[display(fmt = "RegisterUpdatedElement")]
    RegisterUpdatedElement(HeaderHash, EntryHash, SgdBasis),
    #[display(fmt = "RegisterDeletedBy")]
    RegisterDeletedBy(HeaderHash, SgdBasis),
    #[display(fmt = "RegisterDeletedEntryHeader")]
    RegisterDeletedEntryHeader(HeaderHash, SgdBasis),
    #[display(fmt = "RegisterAddLink")]
    RegisterAddLink(HeaderHash, SgdBasis),
    #[display(fmt = "RegisterRemoveLink")]
    RegisterRemoveLink(HeaderHash, SgdBasis),
}

impl PartialEq for SgdOpLight {
    fn eq(&self, other: &Self) -> bool {
        // The ops are the same if they are the same type on the same header hash.
        // We can't derive eq because `Option<EntryHash>` doesn't make the op different.
        // We can ignore the basis because the basis is derived from the header and op type.
        self.get_type() == other.get_type() && self.header_hash() == other.header_hash()
    }
}

impl Eq for SgdOpLight {}

impl std::hash::Hash for SgdOpLight {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.get_type().hash(state);
        self.header_hash().hash(state);
    }
}

/// This enum is used to
#[allow(missing_docs)]
#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    Hash,
    derive_more::Display,
    strum_macros::EnumString,
)]
pub enum SgdOpType {
    #[display(fmt = "StoreElement")]
    StoreElement,
    #[display(fmt = "StoreEntry")]
    StoreEntry,
    #[display(fmt = "RegisterAgentActivity")]
    RegisterAgentActivity,
    #[display(fmt = "RegisterUpdatedContent")]
    RegisterUpdatedContent,
    #[display(fmt = "RegisterUpdatedElement")]
    RegisterUpdatedElement,
    #[display(fmt = "RegisterDeletedBy")]
    RegisterDeletedBy,
    #[display(fmt = "RegisterDeletedEntryHeader")]
    RegisterDeletedEntryHeader,
    #[display(fmt = "RegisterAddLink")]
    RegisterAddLink,
    #[display(fmt = "RegisterRemoveLink")]
    RegisterRemoveLink,
}

impl ToSql for SgdOpType {
    fn to_sql(
        &self,
    ) -> aingle_sqlite::rusqlite::Result<aingle_sqlite::rusqlite::types::ToSqlOutput> {
        Ok(aingle_sqlite::rusqlite::types::ToSqlOutput::Owned(
            format!("{}", self).into(),
        ))
    }
}

impl FromSql for SgdOpType {
    fn column_result(
        value: aingle_sqlite::rusqlite::types::ValueRef<'_>,
    ) -> aingle_sqlite::rusqlite::types::FromSqlResult<Self> {
        String::column_result(value).and_then(|string| {
            SgdOpType::from_str(&string)
                .map_err(|_| aingle_sqlite::rusqlite::types::FromSqlError::InvalidType)
        })
    }
}

impl SgdOp {
    fn as_unique_form(&self) -> UniqueForm<'_> {
        match self {
            Self::StoreElement(_, header, _) => UniqueForm::StoreElement(header),
            Self::StoreEntry(_, header, _) => UniqueForm::StoreEntry(header),
            Self::RegisterAgentActivity(_, header) => UniqueForm::RegisterAgentActivity(header),
            Self::RegisterUpdatedContent(_, header, _) => {
                UniqueForm::RegisterUpdatedContent(header)
            }
            Self::RegisterUpdatedElement(_, header, _) => {
                UniqueForm::RegisterUpdatedElement(header)
            }
            Self::RegisterDeletedBy(_, header) => UniqueForm::RegisterDeletedBy(header),
            Self::RegisterDeletedEntryHeader(_, header) => {
                UniqueForm::RegisterDeletedEntryHeader(header)
            }
            Self::RegisterAddLink(_, header) => UniqueForm::RegisterAddLink(header),
            Self::RegisterRemoveLink(_, header) => UniqueForm::RegisterRemoveLink(header),
        }
    }

    /// Returns the basis hash which determines which agents will receive this SgdOp
    pub fn sgd_basis(&self) -> AnySgdHash {
        self.as_unique_form().basis()
    }

    /// Convert a [SgdOp] to a [SgdOpLight] and basis
    pub fn to_light(
        // Hoping one day we can work out how to go from `&Create`
        // to `&Header::Create(Create)` so punting on a reference
        &self,
    ) -> SgdOpLight {
        let basis = self.sgd_basis();
        match self {
            SgdOp::StoreElement(_, h, _) => {
                let e = h.entry_data().map(|(e, _)| e.clone());
                let h = HeaderHash::with_data_sync(h);
                SgdOpLight::StoreElement(h, e, basis)
            }
            SgdOp::StoreEntry(_, h, _) => {
                let e = h.entry().clone();
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::StoreEntry(h, e, basis)
            }
            SgdOp::RegisterAgentActivity(_, h) => {
                let h = HeaderHash::with_data_sync(h);
                SgdOpLight::RegisterAgentActivity(h, basis)
            }
            SgdOp::RegisterUpdatedContent(_, h, _) => {
                let e = h.entry_hash.clone();
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::RegisterUpdatedContent(h, e, basis)
            }
            SgdOp::RegisterUpdatedElement(_, h, _) => {
                let e = h.entry_hash.clone();
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::RegisterUpdatedElement(h, e, basis)
            }
            SgdOp::RegisterDeletedBy(_, h) => {
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::RegisterDeletedBy(h, basis)
            }
            SgdOp::RegisterDeletedEntryHeader(_, h) => {
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::RegisterDeletedEntryHeader(h, basis)
            }
            SgdOp::RegisterAddLink(_, h) => {
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::RegisterAddLink(h, basis)
            }
            SgdOp::RegisterRemoveLink(_, h) => {
                let h = HeaderHash::with_data_sync(&Header::from(h.clone()));
                SgdOpLight::RegisterRemoveLink(h, basis)
            }
        }
    }

    /// Get the signature for this op
    pub fn signature(&self) -> &Signature {
        match self {
            SgdOp::StoreElement(s, _, _)
            | SgdOp::StoreEntry(s, _, _)
            | SgdOp::RegisterAgentActivity(s, _)
            | SgdOp::RegisterUpdatedContent(s, _, _)
            | SgdOp::RegisterUpdatedElement(s, _, _)
            | SgdOp::RegisterDeletedBy(s, _)
            | SgdOp::RegisterDeletedEntryHeader(s, _)
            | SgdOp::RegisterAddLink(s, _)
            | SgdOp::RegisterRemoveLink(s, _) => s,
        }
    }

    /// Extract inner Signature, Header and Option<Entry> from an op
    pub fn into_inner(self) -> (Signature, Header, Option<Entry>) {
        match self {
            SgdOp::StoreElement(s, h, e) => (s, h, e.map(|e| *e)),
            SgdOp::StoreEntry(s, h, e) => (s, h.into(), Some(*e)),
            SgdOp::RegisterAgentActivity(s, h) => (s, h, None),
            SgdOp::RegisterUpdatedContent(s, h, e) => (s, h.into(), e.map(|e| *e)),
            SgdOp::RegisterUpdatedElement(s, h, e) => (s, h.into(), e.map(|e| *e)),
            SgdOp::RegisterDeletedBy(s, h) => (s, h.into(), None),
            SgdOp::RegisterDeletedEntryHeader(s, h) => (s, h.into(), None),
            SgdOp::RegisterAddLink(s, h) => (s, h.into(), None),
            SgdOp::RegisterRemoveLink(s, h) => (s, h.into(), None),
        }
    }

    /// Get the header from this op
    /// This requires cloning and converting the header
    /// as some ops don't hold the Header type
    pub fn header(&self) -> Header {
        match self {
            SgdOp::StoreElement(_, h, _) => h.clone(),
            SgdOp::StoreEntry(_, h, _) => h.clone().into(),
            SgdOp::RegisterAgentActivity(_, h) => h.clone(),
            SgdOp::RegisterUpdatedContent(_, h, _) => h.clone().into(),
            SgdOp::RegisterUpdatedElement(_, h, _) => h.clone().into(),
            SgdOp::RegisterDeletedBy(_, h) => h.clone().into(),
            SgdOp::RegisterDeletedEntryHeader(_, h) => h.clone().into(),
            SgdOp::RegisterAddLink(_, h) => h.clone().into(),
            SgdOp::RegisterRemoveLink(_, h) => h.clone().into(),
        }
    }

    /// Get the entry from this op, if one exists
    pub fn entry(&self) -> Option<&Entry> {
        match self {
            SgdOp::StoreElement(_, _, e) => e.as_ref().map(|b| &**b),
            SgdOp::StoreEntry(_, _, e) => Some(e),
            SgdOp::RegisterUpdatedContent(_, _, e) => e.as_ref().map(|b| &**b),
            SgdOp::RegisterUpdatedElement(_, _, e) => e.as_ref().map(|b| &**b),
            SgdOp::RegisterAgentActivity(_, _) => None,
            SgdOp::RegisterDeletedBy(_, _) => None,
            SgdOp::RegisterDeletedEntryHeader(_, _) => None,
            SgdOp::RegisterAddLink(_, _) => None,
            SgdOp::RegisterRemoveLink(_, _) => None,
        }
    }

    /// Get the type as a unit enum, for Display purposes
    pub fn get_type(&self) -> SgdOpType {
        match self {
            SgdOp::StoreElement(_, _, _) => SgdOpType::StoreElement,
            SgdOp::StoreEntry(_, _, _) => SgdOpType::StoreEntry,
            SgdOp::RegisterUpdatedContent(_, _, _) => SgdOpType::RegisterUpdatedContent,
            SgdOp::RegisterUpdatedElement(_, _, _) => SgdOpType::RegisterUpdatedElement,
            SgdOp::RegisterAgentActivity(_, _) => SgdOpType::RegisterAgentActivity,
            SgdOp::RegisterDeletedBy(_, _) => SgdOpType::RegisterDeletedBy,
            SgdOp::RegisterDeletedEntryHeader(_, _) => SgdOpType::RegisterDeletedEntryHeader,
            SgdOp::RegisterAddLink(_, _) => SgdOpType::RegisterAddLink,
            SgdOp::RegisterRemoveLink(_, _) => SgdOpType::RegisterRemoveLink,
        }
    }

    /// From a type, header and an entry (if there is one)
    pub fn from_type(
        op_type: SgdOpType,
        header: SignedHeader,
        entry: Option<Entry>,
    ) -> SgdOpResult<Self> {
        let SignedHeader(header, signature) = header;
        let r = match op_type {
            SgdOpType::StoreElement => SgdOp::StoreElement(signature, header, entry.map(Box::new)),
            SgdOpType::StoreEntry => {
                let entry = entry.ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?;
                let header = match header {
                    Header::Create(c) => NewEntryHeader::Create(c),
                    Header::Update(c) => NewEntryHeader::Update(c),
                    _ => return Err(SgdOpError::OpHeaderMismatch(op_type, header.header_type())),
                };
                SgdOp::StoreEntry(signature, header, Box::new(entry))
            }
            SgdOpType::RegisterAgentActivity => SgdOp::RegisterAgentActivity(signature, header),
            SgdOpType::RegisterUpdatedContent => {
                SgdOp::RegisterUpdatedContent(signature, header.try_into()?, entry.map(Box::new))
            }
            SgdOpType::RegisterUpdatedElement => {
                SgdOp::RegisterUpdatedElement(signature, header.try_into()?, entry.map(Box::new))
            }
            SgdOpType::RegisterDeletedBy => SgdOp::RegisterDeletedBy(signature, header.try_into()?),
            SgdOpType::RegisterDeletedEntryHeader => {
                SgdOp::RegisterDeletedBy(signature, header.try_into()?)
            }
            SgdOpType::RegisterAddLink => SgdOp::RegisterAddLink(signature, header.try_into()?),
            SgdOpType::RegisterRemoveLink => {
                SgdOp::RegisterRemoveLink(signature, header.try_into()?)
            }
        };
        Ok(r)
    }
}

impl SgdOpLight {
    /// Get the sgd basis for where to send this op
    pub fn sgd_basis(&self) -> &AnySgdHash {
        match self {
            SgdOpLight::StoreElement(_, _, b)
            | SgdOpLight::StoreEntry(_, _, b)
            | SgdOpLight::RegisterAgentActivity(_, b)
            | SgdOpLight::RegisterUpdatedContent(_, _, b)
            | SgdOpLight::RegisterUpdatedElement(_, _, b)
            | SgdOpLight::RegisterDeletedBy(_, b)
            | SgdOpLight::RegisterDeletedEntryHeader(_, b)
            | SgdOpLight::RegisterAddLink(_, b)
            | SgdOpLight::RegisterRemoveLink(_, b) => b,
        }
    }
    /// Get the header hash from this op
    pub fn header_hash(&self) -> &HeaderHash {
        match self {
            SgdOpLight::StoreElement(h, _, _)
            | SgdOpLight::StoreEntry(h, _, _)
            | SgdOpLight::RegisterAgentActivity(h, _)
            | SgdOpLight::RegisterUpdatedContent(h, _, _)
            | SgdOpLight::RegisterUpdatedElement(h, _, _)
            | SgdOpLight::RegisterDeletedBy(h, _)
            | SgdOpLight::RegisterDeletedEntryHeader(h, _)
            | SgdOpLight::RegisterAddLink(h, _)
            | SgdOpLight::RegisterRemoveLink(h, _) => h,
        }
    }

    /// Get the type as a unit enum, for Display purposes
    pub fn get_type(&self) -> SgdOpType {
        match self {
            SgdOpLight::StoreElement(_, _, _) => SgdOpType::StoreElement,
            SgdOpLight::StoreEntry(_, _, _) => SgdOpType::StoreEntry,
            SgdOpLight::RegisterUpdatedContent(_, _, _) => SgdOpType::RegisterUpdatedContent,
            SgdOpLight::RegisterUpdatedElement(_, _, _) => SgdOpType::RegisterUpdatedElement,
            SgdOpLight::RegisterAgentActivity(_, _) => SgdOpType::RegisterAgentActivity,
            SgdOpLight::RegisterDeletedBy(_, _) => SgdOpType::RegisterDeletedBy,
            SgdOpLight::RegisterDeletedEntryHeader(_, _) => SgdOpType::RegisterDeletedEntryHeader,
            SgdOpLight::RegisterAddLink(_, _) => SgdOpType::RegisterAddLink,
            SgdOpLight::RegisterRemoveLink(_, _) => SgdOpType::RegisterRemoveLink,
        }
    }

    /// From a type with the hashes.
    pub fn from_type(
        op_type: SgdOpType,
        header_hash: HeaderHash,
        header: &Header,
    ) -> SgdOpResult<Self> {
        let op = match op_type {
            SgdOpType::StoreElement => {
                let entry_hash = header.entry_hash().cloned();
                Self::StoreElement(header_hash.clone(), entry_hash, header_hash.into())
            }
            SgdOpType::StoreEntry => {
                let entry_hash = header
                    .entry_hash()
                    .cloned()
                    .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?;
                Self::StoreEntry(header_hash, entry_hash.clone(), entry_hash.into())
            }
            SgdOpType::RegisterAgentActivity => {
                Self::RegisterAgentActivity(header_hash, header.author().clone().into())
            }
            SgdOpType::RegisterUpdatedContent => {
                let entry_hash = header
                    .entry_hash()
                    .cloned()
                    .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?;
                let basis = match header {
                    Header::Update(update) => update.original_entry_address.clone(),
                    _ => return Err(SgdOpError::OpHeaderMismatch(op_type, header.header_type())),
                };
                Self::RegisterUpdatedContent(header_hash, entry_hash, basis.into())
            }
            SgdOpType::RegisterUpdatedElement => {
                let entry_hash = header
                    .entry_hash()
                    .cloned()
                    .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?;
                let basis = match header {
                    Header::Update(update) => update.original_entry_address.clone(),
                    _ => return Err(SgdOpError::OpHeaderMismatch(op_type, header.header_type())),
                };
                Self::RegisterUpdatedElement(header_hash, entry_hash, basis.into())
            }
            SgdOpType::RegisterDeletedBy => {
                Self::RegisterDeletedBy(header_hash.clone(), header_hash.into())
            }
            SgdOpType::RegisterDeletedEntryHeader => {
                let basis = header
                    .entry_hash()
                    .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?
                    .clone();
                Self::RegisterDeletedBy(header_hash, basis.into())
            }
            SgdOpType::RegisterAddLink => {
                let basis = match header {
                    Header::CreateLink(create_link) => create_link.base_address.clone(),
                    _ => return Err(SgdOpError::OpHeaderMismatch(op_type, header.header_type())),
                };
                Self::RegisterAddLink(header_hash, basis.into())
            }
            SgdOpType::RegisterRemoveLink => {
                let basis = match header {
                    Header::DeleteLink(delete_link) => delete_link.base_address.clone(),
                    _ => return Err(SgdOpError::OpHeaderMismatch(op_type, header.header_type())),
                };
                Self::RegisterRemoveLink(header_hash, basis.into())
            }
        };
        Ok(op)
    }
}

// FIXME: need to use this in HashableContent
#[allow(missing_docs)]
#[derive(Serialize, Debug)]
pub enum UniqueForm<'a> {
    // As an optimization, we don't include signatures. They would be redundant
    // with headers and therefore would waste hash/comparison time to include.
    StoreElement(&'a Header),
    StoreEntry(&'a NewEntryHeader),
    RegisterAgentActivity(&'a Header),
    RegisterUpdatedContent(&'a header::Update),
    RegisterUpdatedElement(&'a header::Update),
    RegisterDeletedBy(&'a header::Delete),
    RegisterDeletedEntryHeader(&'a header::Delete),
    RegisterAddLink(&'a header::CreateLink),
    RegisterRemoveLink(&'a header::DeleteLink),
}

impl<'a> UniqueForm<'a> {
    fn basis(&'a self) -> AnySgdHash {
        match self {
            UniqueForm::StoreElement(header) => HeaderHash::with_data_sync(*header).into(),
            UniqueForm::StoreEntry(header) => header.entry().clone().into(),
            UniqueForm::RegisterAgentActivity(header) => header.author().clone().into(),
            UniqueForm::RegisterUpdatedContent(header) => {
                header.original_entry_address.clone().into()
            }
            UniqueForm::RegisterUpdatedElement(header) => {
                header.original_header_address.clone().into()
            }
            UniqueForm::RegisterDeletedBy(header) => header.deletes_address.clone().into(),
            UniqueForm::RegisterDeletedEntryHeader(header) => {
                header.deletes_entry_address.clone().into()
            }
            UniqueForm::RegisterAddLink(header) => header.base_address.clone().into(),
            UniqueForm::RegisterRemoveLink(header) => header.base_address.clone().into(),
        }
    }

    /// Get the sgd op hash without cloning the header.
    pub fn op_hash(op_type: SgdOpType, header: Header) -> SgdOpResult<(Header, SgdOpHash)> {
        match op_type {
            SgdOpType::StoreElement => {
                let hash = SgdOpHash::with_data_sync(&UniqueForm::StoreElement(&header));
                Ok((header, hash))
            }
            SgdOpType::StoreEntry => {
                let header = header.try_into()?;
                let hash = SgdOpHash::with_data_sync(&UniqueForm::StoreEntry(&header));
                Ok((header.into(), hash))
            }
            SgdOpType::RegisterAgentActivity => {
                let hash = SgdOpHash::with_data_sync(&UniqueForm::RegisterAgentActivity(&header));
                Ok((header, hash))
            }
            SgdOpType::RegisterUpdatedContent => {
                let header = header.try_into()?;
                let hash = SgdOpHash::with_data_sync(&UniqueForm::RegisterUpdatedContent(&header));
                Ok((header.into(), hash))
            }
            SgdOpType::RegisterUpdatedElement => {
                let header = header.try_into()?;
                let hash = SgdOpHash::with_data_sync(&UniqueForm::RegisterUpdatedElement(&header));
                Ok((header.into(), hash))
            }
            SgdOpType::RegisterDeletedBy => {
                let header = header.try_into()?;
                let hash = SgdOpHash::with_data_sync(&UniqueForm::RegisterDeletedBy(&header));
                Ok((header.into(), hash))
            }
            SgdOpType::RegisterDeletedEntryHeader => {
                let header = header.try_into()?;
                let hash =
                    SgdOpHash::with_data_sync(&UniqueForm::RegisterDeletedEntryHeader(&header));
                Ok((header.into(), hash))
            }
            SgdOpType::RegisterAddLink => {
                let header = header.try_into()?;
                let hash = SgdOpHash::with_data_sync(&UniqueForm::RegisterAddLink(&header));
                Ok((header.into(), hash))
            }
            SgdOpType::RegisterRemoveLink => {
                let header = header.try_into()?;
                let hash = SgdOpHash::with_data_sync(&UniqueForm::RegisterRemoveLink(&header));
                Ok((header.into(), hash))
            }
        }
    }
}

/// Produce all SgdOps for a Element
pub fn produce_ops_from_element(element: &Element) -> SgdOpResult<Vec<SgdOp>> {
    let op_lights = produce_op_lights_from_elements(vec![element])?;
    let (shh, maybe_entry) = element.clone().into_inner();
    let (header, signature): (Header, Signature) = shh.into_inner().0.into();

    let mut ops = Vec::with_capacity(op_lights.len());

    for op_light in op_lights {
        let signature = signature.clone();
        let header = header.clone();
        let op = match op_light {
            SgdOpLight::StoreElement(_, _, _) => {
                let maybe_entry_box = maybe_entry.clone().into_option().map(Box::new);
                SgdOp::StoreElement(signature, header, maybe_entry_box)
            }
            SgdOpLight::StoreEntry(_, _, _) => {
                let new_entry_header = header.clone().try_into()?;
                let box_entry = match maybe_entry.clone().into_option() {
                    Some(entry) => Box::new(entry),
                    None => {
                        // Entry is private so continue
                        continue;
                    }
                };
                SgdOp::StoreEntry(signature, new_entry_header, box_entry)
            }
            SgdOpLight::RegisterAgentActivity(_, _) => {
                SgdOp::RegisterAgentActivity(signature, header)
            }
            SgdOpLight::RegisterUpdatedContent(_, _, _) => {
                let entry_update = header.try_into()?;
                let maybe_entry_box = maybe_entry.clone().into_option().map(Box::new);
                SgdOp::RegisterUpdatedContent(signature, entry_update, maybe_entry_box)
            }
            SgdOpLight::RegisterUpdatedElement(_, _, _) => {
                let entry_update = header.try_into()?;
                let maybe_entry_box = maybe_entry.clone().into_option().map(Box::new);
                SgdOp::RegisterUpdatedElement(signature, entry_update, maybe_entry_box)
            }
            SgdOpLight::RegisterDeletedEntryHeader(_, _) => {
                let element_delete = header.try_into()?;
                SgdOp::RegisterDeletedEntryHeader(signature, element_delete)
            }
            SgdOpLight::RegisterDeletedBy(_, _) => {
                let element_delete = header.try_into()?;
                SgdOp::RegisterDeletedBy(signature, element_delete)
            }
            SgdOpLight::RegisterAddLink(_, _) => {
                let link_add = header.try_into()?;
                SgdOp::RegisterAddLink(signature, link_add)
            }
            SgdOpLight::RegisterRemoveLink(_, _) => {
                let link_remove = header.try_into()?;
                SgdOp::RegisterRemoveLink(signature, link_remove)
            }
        };
        ops.push(op);
    }
    Ok(ops)
}

/// Produce all the op lights for tese elements
pub fn produce_op_lights_from_elements(headers: Vec<&Element>) -> SgdOpResult<Vec<SgdOpLight>> {
    let length = headers.len();
    let headers_and_hashes = headers.into_iter().map(|e| {
        (
            e.header_address(),
            e.header(),
            e.header().entry_data().map(|(h, _)| h.clone()),
        )
    });
    produce_op_lights_from_iter(headers_and_hashes, length)
}

/// Produce all the op lights from this element group
/// with a shared entry
pub fn produce_op_lights_from_element_group(
    elements: &ElementGroup<'_>,
) -> SgdOpResult<Vec<SgdOpLight>> {
    let len = elements.len();
    let headers_and_hashes = elements.headers_and_hashes();
    let maybe_entry_hash = Some(elements.entry_hash());
    produce_op_lights_from_parts(headers_and_hashes, maybe_entry_hash, len)
}

/// Data minimal clone (no cloning entries) cheap &Element to SgdOpLight conversion
fn produce_op_lights_from_parts<'a>(
    headers_and_hashes: impl Iterator<Item = (&'a HeaderHash, &'a Header)>,
    maybe_entry_hash: Option<&EntryHash>,
    length: usize,
) -> SgdOpResult<Vec<SgdOpLight>> {
    let iter = headers_and_hashes.map(|(head, hash)| (head, hash, maybe_entry_hash.cloned()));
    produce_op_lights_from_iter(iter, length)
}

/// Produce op lights from iter of (header hash, header, maybe entry).
pub fn produce_op_lights_from_iter<'a>(
    iter: impl Iterator<Item = (&'a HeaderHash, &'a Header, Option<EntryHash>)>,
    length: usize,
) -> SgdOpResult<Vec<SgdOpLight>> {
    // Each header will have at least 2 ops
    let mut ops = Vec::with_capacity(length * 2);

    for (header_hash, header, maybe_entry_hash) in iter {
        let header_hash = header_hash.clone();

        let store_element_basis = UniqueForm::StoreElement(header).basis();
        let register_activity_basis = UniqueForm::RegisterAgentActivity(header).basis();

        ops.push(SgdOpLight::StoreElement(
            header_hash.clone(),
            maybe_entry_hash.clone(),
            store_element_basis,
        ));
        ops.push(SgdOpLight::RegisterAgentActivity(
            header_hash.clone(),
            register_activity_basis,
        ));

        match header {
            Header::Saf(_)
            | Header::OpenChain(_)
            | Header::CloseChain(_)
            | Header::AgentValidationPkg(_)
            | Header::InitZomesComplete(_) => {}
            Header::CreateLink(link_add) => ops.push(SgdOpLight::RegisterAddLink(
                header_hash,
                UniqueForm::RegisterAddLink(link_add).basis(),
            )),
            Header::DeleteLink(link_remove) => ops.push(SgdOpLight::RegisterRemoveLink(
                header_hash,
                UniqueForm::RegisterRemoveLink(link_remove).basis(),
            )),
            Header::Create(entry_create) => ops.push(SgdOpLight::StoreEntry(
                header_hash,
                maybe_entry_hash.ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?,
                UniqueForm::StoreEntry(&NewEntryHeader::Create(entry_create.clone())).basis(),
            )),
            Header::Update(entry_update) => {
                let entry_hash = maybe_entry_hash
                    .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?;
                ops.push(SgdOpLight::StoreEntry(
                    header_hash.clone(),
                    entry_hash.clone(),
                    UniqueForm::StoreEntry(&NewEntryHeader::Update(entry_update.clone())).basis(),
                ));
                ops.push(SgdOpLight::RegisterUpdatedContent(
                    header_hash.clone(),
                    entry_hash.clone(),
                    UniqueForm::RegisterUpdatedContent(entry_update).basis(),
                ));
                ops.push(SgdOpLight::RegisterUpdatedElement(
                    header_hash,
                    entry_hash,
                    UniqueForm::RegisterUpdatedElement(entry_update).basis(),
                ));
            }
            Header::Delete(entry_delete) => {
                // TODO: VALIDATION: This only works if entry_delete.remove_address is either Create
                // or Update
                ops.push(SgdOpLight::RegisterDeletedBy(
                    header_hash.clone(),
                    UniqueForm::RegisterDeletedBy(entry_delete).basis(),
                ));
                ops.push(SgdOpLight::RegisterDeletedEntryHeader(
                    header_hash,
                    UniqueForm::RegisterDeletedEntryHeader(entry_delete).basis(),
                ));
            }
        }
    }
    Ok(ops)
}

// This has to be done manually because the macro
// implements both directions and that isn't possible with references
// TODO: Maybe add a one-way version to aingle_middleware_bytes?
impl<'a> TryFrom<&UniqueForm<'a>> for SerializedBytes {
    type Error = SerializedBytesError;
    fn try_from(u: &UniqueForm<'a>) -> Result<Self, Self::Error> {
        match aingle_middleware_bytes::encode(u) {
            Ok(v) => Ok(SerializedBytes::from(
                aingle_middleware_bytes::UnsafeBytes::from(v),
            )),
            Err(e) => Err(SerializedBytesError::Serialize(e.to_string())),
        }
    }
}

/// A SgdOp paired with its SgdOpHash
pub type SgdOpHashed = AiHashed<SgdOp>;

impl HashableContent for SgdOp {
    type HashType = hash_type::SgdOp;

    fn hash_type(&self) -> Self::HashType {
        hash_type::SgdOp
    }

    fn hashable_content(&self) -> HashableContentBytes {
        HashableContentBytes::Content(
            (&self.as_unique_form())
                .try_into()
                .expect("Could not serialize HashableContent"),
        )
    }
}

impl HashableContent for UniqueForm<'_> {
    type HashType = hash_type::SgdOp;

    fn hash_type(&self) -> Self::HashType {
        hash_type::SgdOp
    }

    fn hashable_content(&self) -> HashableContentBytes {
        HashableContentBytes::Content(
            self.try_into()
                .expect("Could not serialize HashableContent"),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SerializedBytes)]
/// Condensed version of ops for sending across the wire.
pub enum WireOps {
    /// Response for get entry.
    Entry(WireEntryOps),
    /// Response for get element.
    Element(WireElementOps),
}

impl WireOps {
    /// Render the wire ops to SgdOps.
    pub fn render(self) -> SgdOpResult<RenderedOps> {
        match self {
            WireOps::Entry(o) => o.render(),
            WireOps::Element(o) => o.render(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
/// The data rendered from a wire op to place in the database.
pub struct RenderedOp {
    /// The header to insert into the database.
    pub header: SignedHeaderHashed,
    /// The header to insert into the database.
    pub op_light: SgdOpLight,
    /// The hash of the [`SgdOp`]
    pub op_hash: SgdOpHash,
    /// The validation status of the header.
    pub validation_status: Option<ValidationStatus>,
}

impl RenderedOp {
    /// Try to create a new rendered op from wire data.
    /// This function computes all the hashes and
    /// reconstructs the full headers.
    pub fn new(
        header: Header,
        signature: Signature,
        validation_status: Option<ValidationStatus>,
        op_type: SgdOpType,
    ) -> SgdOpResult<Self> {
        let (header, op_hash) = UniqueForm::op_hash(op_type, header)?;
        let header_hashed = HeaderHashed::from_content_sync(header);
        // TODO: Verify signature?
        let header = SignedHeaderHashed::with_presigned(header_hashed, signature);
        let op_light = SgdOpLight::from_type(op_type, header.as_hash().clone(), header.header())?;
        Ok(Self {
            header,
            op_light,
            op_hash,
            validation_status,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
/// The full data for insertion into the database.
/// The reason we don't use [`SgdOp`] is because we don't
/// want to clone the entry for every header.
pub struct RenderedOps {
    /// Entry for the ops if there is one.
    pub entry: Option<EntryHashed>,
    /// Op data to insert.
    pub ops: Vec<RenderedOp>,
}

/// Type for deriving ordering of SgdOps
/// Don't change the order of this enum unless
/// you mean to change the order we process ops
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum OpNumericalOrder {
    RegisterAgentActivity = 0,
    StoreEntry,
    StoreElement,
    RegisterUpdatedContent,
    RegisterUpdatedElement,
    RegisterDeletedBy,
    RegisterDeletedEntryHeader,
    RegisterAddLink,
    RegisterRemoveLink,
}

/// This is used as an index for ordering ops in our database.
/// It gives the most likely ordering where dependencies will come
/// first.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OpOrder {
    order: OpNumericalOrder,
    timestamp: aingle_zome_types::timestamp::Timestamp,
}

impl std::fmt::Display for OpOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{:019}{:010}",
            self.order as u8, self.timestamp.0, self.timestamp.1
        )
    }
}

impl OpOrder {
    /// Create a new ordering from a op type and timestamp.
    pub fn new(op_type: SgdOpType, timestamp: aingle_zome_types::timestamp::Timestamp) -> Self {
        let order = match op_type {
            SgdOpType::StoreElement => OpNumericalOrder::StoreElement,
            SgdOpType::StoreEntry => OpNumericalOrder::StoreEntry,
            SgdOpType::RegisterAgentActivity => OpNumericalOrder::RegisterAgentActivity,
            SgdOpType::RegisterUpdatedContent => OpNumericalOrder::RegisterUpdatedContent,
            SgdOpType::RegisterUpdatedElement => OpNumericalOrder::RegisterUpdatedElement,
            SgdOpType::RegisterDeletedBy => OpNumericalOrder::RegisterDeletedBy,
            SgdOpType::RegisterDeletedEntryHeader => OpNumericalOrder::RegisterDeletedEntryHeader,
            SgdOpType::RegisterAddLink => OpNumericalOrder::RegisterAddLink,
            SgdOpType::RegisterRemoveLink => OpNumericalOrder::RegisterRemoveLink,
        };
        Self { order, timestamp }
    }
}

impl aingle_sqlite::rusqlite::ToSql for OpOrder {
    fn to_sql(
        &self,
    ) -> aingle_sqlite::rusqlite::Result<aingle_sqlite::rusqlite::types::ToSqlOutput> {
        Ok(aingle_sqlite::rusqlite::types::ToSqlOutput::Owned(
            self.to_string().into(),
        ))
    }
}
