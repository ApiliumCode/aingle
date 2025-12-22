use crate::AiHashOf;
use crate::HasHash;
use crate::HashableContent;
use aingle_middleware_bytes::prelude::*;

/// Represents some piece of content along with its hash representation, so that
/// hashes need not be calculated multiple times.
/// Provides an easy constructor which consumes the content.
// TODO: consider making lazy with OnceCell
#[derive(Debug, Serialize, Deserialize)]
pub struct AiHashed<C: HashableContent> {
    /// Whatever type C is as data.
    pub(crate) content: C,
    /// The hash of the content C.
    pub(crate) hash: AiHashOf<C>,
}

impl<C: HashableContent> HasHash<C::HashType> for AiHashed<C> {
    fn as_hash(&self) -> &AiHashOf<C> {
        &self.hash
    }

    fn into_hash(self) -> AiHashOf<C> {
        self.hash
    }
}

impl<C> AiHashed<C>
where
    C: HashableContent,
{
    /// Combine content with its precalculated hash
    pub fn with_pre_hashed(content: C, hash: AiHashOf<C>) -> Self {
        Self { content, hash }
    }

    // NB: as_hash and into_hash are provided by the HasHash impl

    /// Accessor for content
    pub fn as_content(&self) -> &C {
        &self.content
    }

    /// Mutable accessor for content.
    /// Only useful for heavily mocked/fixturated data in testing.
    /// Guaranteed the hash will no longer match the content if mutated.
    #[cfg(feature = "test_utils")]
    pub fn as_content_mut(&mut self) -> &mut C {
        &mut self.content
    }

    /// Convert to content
    pub fn into_content(self) -> C {
        self.content
    }

    /// Deconstruct as a tuple
    pub fn into_inner(self) -> (C, AiHashOf<C>) {
        (self.content, self.hash)
    }
}

impl<C> Clone for AiHashed<C>
where
    C: HashableContent + Clone,
{
    fn clone(&self) -> Self {
        Self {
            content: self.content.clone(),
            hash: self.hash.clone(),
        }
    }
}

impl<C> std::convert::From<AiHashed<C>> for (C, AiHashOf<C>)
where
    C: HashableContent,
{
    fn from(g: AiHashed<C>) -> (C, AiHashOf<C>) {
        g.into_inner()
    }
}

impl<C> std::ops::Deref for AiHashed<C>
where
    C: HashableContent,
{
    type Target = C;

    fn deref(&self) -> &Self::Target {
        self.as_content()
    }
}

impl<C> std::convert::AsRef<C> for AiHashed<C>
where
    C: HashableContent,
{
    fn as_ref(&self) -> &C {
        self.as_content()
    }
}

impl<C> std::borrow::Borrow<C> for AiHashed<C>
where
    C: HashableContent,
{
    fn borrow(&self) -> &C {
        self.as_content()
    }
}

impl<C> std::cmp::PartialEq for AiHashed<C>
where
    C: HashableContent,
{
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl<C> std::cmp::Eq for AiHashed<C> where C: HashableContent {}

impl<C> std::hash::Hash for AiHashed<C>
where
    C: HashableContent,
{
    fn hash<StdH: std::hash::Hasher>(&self, state: &mut StdH) {
        std::hash::Hash::hash(&self.hash, state)
    }
}
