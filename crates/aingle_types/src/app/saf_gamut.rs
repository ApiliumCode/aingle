//! A SAF gamut is a representation of all SAFs available in a given context.

use super::SafVersionSpec;
use crate::prelude::*;
use std::collections::{hash_map, HashMap, HashSet};

/// Representation of all SAFs and Cells available in a given context.
/// When given a SafVersionSpec, a particular SAF can be selected from this
/// gamut.
///
/// Moreover, each SAF hash has associated with it a list of Agents. Each agent
/// represents a Cell which exists on the conductor, using that SAF and agent
/// pair. A SAF with no agents listed is simply registered but does not exist
/// in any Cell.
///
/// NB: since our SafVersionSpec is currently very simplistic, so is the gamut.
/// As our versioning becomes more expressive, so will this type. For instance,
/// if we introduce semver, the gamut will include versions of SAFs as well.
///
/// This type basically exists as an abstract adapter between the conductor's
/// SAF store and the app installation process. Without needing to know exactly
/// what we will need from the SAF store, we can define what questions we will
/// need to ask of it through this type.
pub struct SafGamut(HashMap<SafHash, HashSet<AgentPubKey>>);

/// We don't have any notion of SAF versioning other than the hash, but this is
/// a placeholder to indicate the need for it in the future and to start using
/// it in public interfaces.
pub struct SafVersion;

impl SafGamut {
    /// Constructor. Restructure a list of CellIds into the proper format.
    pub fn new<I: IntoIterator<Item = CellId>>(cells: I) -> Self {
        let mut map: HashMap<SafHash, HashSet<AgentPubKey>> = HashMap::new();
        for cell in cells {
            let (saf, agent) = cell.into_saf_and_agent();
            match map.entry(saf) {
                hash_map::Entry::Occupied(mut e) => {
                    e.get_mut().insert(agent);
                }
                hash_map::Entry::Vacant(e) => {
                    e.insert(vec![agent].into_iter().collect());
                }
            }
        }
        Self(map)
    }

    #[deprecated = "Stop using the placeholder"]
    #[allow(missing_docs)]
    pub fn placeholder() -> Self {
        Self::new(std::iter::empty())
    }

    /// Given a version spec, return the best-matching SAF in the gamut
    pub fn resolve_saf(&self, spec: SafVersionSpec) -> SafResolution {
        for hash in spec.saf_hashes() {
            if self.0.contains_key(hash.as_ref()) {
                return SafResolution::Match(hash.clone(), SafVersion);
            }
        }
        SafResolution::NoMatch
    }

    /// Given a version spec, return the best-matching CellId
    // TODO: use DPKI to filter Cells which belong to Agents that are not
    //       associated with the provided agent
    pub fn resolve_cell(&self, spec: SafVersionSpec, _agent: &AgentPubKey) -> CellResolution {
        for hash in spec.saf_hashes() {
            if let Some(agent) = self
                .0
                .get(hash.as_ref())
                // TODO: this is where an agent check could go, but for now we
                //       just return the first one available
                .map(|agents| agents.iter().next())
                .unwrap_or(None)
            {
                return CellResolution::Match(
                    CellId::new(hash.clone().into(), agent.clone()),
                    SafVersion,
                );
            }
        }
        CellResolution::NoMatch
    }
}

/// Possible results of SAF resolution
pub enum SafResolution {
    /// A match was found within the gamut
    Match(SafHashB64, SafVersion),
    /// No match was found
    NoMatch,
    /// Multiple matches were found, or other scenario that requires user
    /// intervention for resolution (TODO, placeholder)
    Conflict,
}

/// Possible results of Cell resolution
pub enum CellResolution {
    /// A match was found within the gamut
    Match(CellId, SafVersion),
    /// No match was found
    NoMatch,
    /// Multiple matches were found, or other scenario that requires user
    /// intervention for resolution (TODO, placeholder)
    Conflict,
}
