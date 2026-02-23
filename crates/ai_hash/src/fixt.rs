#![allow(missing_docs)]

use crate::encode::ai_sgd_location_bytes;
use crate::hash_type;
use crate::AgentPubKey;
use crate::AgentPubKeyB64;
use crate::AnySgdHash;
use crate::AnySgdHashB64;
use crate::EntryHash;
use crate::EntryHashB64;
use crate::HeaderHash;
use crate::HeaderHashB64;
use crate::NetIdHash;
use crate::NetIdHashB64;
use crate::SafHash;
use crate::SafHashB64;
use crate::SgdOpHash;
use crate::SgdOpHashB64;
use crate::WasmHash;
use crate::WasmHashB64;
use ::ai_fixt::prelude::*;
use std::convert::TryFrom;

pub type HashTypeEntry = hash_type::Entry;
pub type HashTypeAnySgd = hash_type::AnySgd;

// TODO: use strum to do this:
//
// fixturator!(
//     HashTypeEntry;
//     unit variants [ Agent Content ] empty Content;
// );

fixturator!(
    HashTypeAnySgd;
    curve Empty HashTypeAnySgd::Header;
    curve Unpredictable HashTypeAnySgd::Header;
    curve Predictable HashTypeAnySgd::Header;
);

/// A type alias for a Vec<u8> whose fixturator is expected to only return
/// a Vec of length 36
pub type ThirtySixHashBytes = Vec<u8>;

// Simply generate "bytes" which is a Vec<u8> of 36 bytes
fixturator!(
    ThirtySixHashBytes,
    append_location([0; 32].to_vec()),
    {
        let mut u8_fixturator = U8Fixturator::new(Unpredictable);
        let mut bytes = vec![];
        for _ in 0..32 {
            bytes.push(u8_fixturator.next().unwrap());
        }
        append_location(bytes)
    },
    {
        let mut index = get_fixt_index!();
        let mut u8_fixturator = U8Fixturator::new_indexed(Predictable, index);
        let mut bytes = vec![];
        for _ in 0..32 {
            bytes.push(u8_fixturator.next().unwrap());
        }
        index += 1;
        set_fixt_index!(index);
        append_location(bytes)
    }
);

fn append_location(mut base: Vec<u8>) -> Vec<u8> {
    let mut loc_bytes = ai_sgd_location_bytes(&base);
    base.append(&mut loc_bytes);
    base
}

fixturator!(
    // with_vec 0 5;
    AgentPubKey;
    curve Empty AgentPubKey::from_raw_36(ThirtySixHashBytesFixturator::new_indexed(Empty, get_fixt_index!()).next().unwrap());
    curve Unpredictable AgentPubKey::from_raw_36(ThirtySixHashBytesFixturator::new_indexed(Unpredictable, get_fixt_index!()).next().unwrap());
    curve Predictable {
        // these agent keys match what the mock keystore spits out for the first two agents
        // don't mess with this unless you also update the keystore!!!
        let agents = [AgentPubKey::try_from("uhCAkmrkoAHPVf_eufG7eC5fm6QKrW5pPMoktvG5LOC0SnJ4vV1Uv")
            .unwrap(),
            AgentPubKey::try_from("uhCAke1j8Z2a-_min0h0pGuEMcYlo_V1l1mt9OtBuywKmHlg4L_R-")
                .unwrap()];
        agents[get_fixt_index!() % agents.len()].clone()
    };
);

fixturator!(
    AgentPubKeyB64;
    constructor fn new(AgentPubKey);
);

fixturator!(
    EntryHash;
    constructor fn from_raw_36(ThirtySixHashBytes);
);
fixturator!(
    EntryHashB64;
    constructor fn new(EntryHash);
);

fixturator!(
    SafHash;
    constructor fn from_raw_36(ThirtySixHashBytes);
);
fixturator!(
    SafHashB64;
    constructor fn new(SafHash);
);

fixturator!(
    SgdOpHash;
    constructor fn from_raw_36(ThirtySixHashBytes);
);
fixturator!(
    SgdOpHashB64;
    constructor fn new(SgdOpHash);
);

fixturator!(
    HeaderHash;
    constructor fn from_raw_36(ThirtySixHashBytes);
);
fixturator!(
    HeaderHashB64;
    constructor fn new(HeaderHash);
);

fixturator!(
    NetIdHash;
    constructor fn from_raw_36(ThirtySixHashBytes);
);
fixturator!(
    NetIdHashB64;
    constructor fn new(NetIdHash);
);

fixturator!(
    WasmHash;
    constructor fn from_raw_36(ThirtySixHashBytes);
);
fixturator!(
    WasmHashB64;
    constructor fn new(WasmHash);
);

fixturator!(
    AnySgdHash;
    constructor fn from_raw_36_and_type(ThirtySixHashBytes, HashTypeAnySgd);
);
fixturator!(
    AnySgdHashB64;
    constructor fn new(AnySgdHash);
);
