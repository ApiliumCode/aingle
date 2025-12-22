use crate::assert_length;
use crate::error::AiHashError;
use crate::AiHash;
use crate::HashType;
use crate::PrimitiveHashType;
use crate::AI_HASH_CORE_LEN;
use crate::AI_HASH_FULL_LEN;
use crate::AI_HASH_PREFIX_LEN;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::convert::TryFrom;
use std::convert::TryInto;

impl<P: PrimitiveHashType> TryFrom<&str> for AiHash<P> {
    type Error = AiHashError;
    fn try_from(s: &str) -> Result<Self, AiHashError> {
        let hash_type = P::new();
        AiHash::from_raw_39(ai_hash_decode(hash_type.get_prefix(), s)?)
    }
}

impl<P: PrimitiveHashType> TryFrom<&String> for AiHash<P> {
    type Error = AiHashError;
    fn try_from(s: &String) -> Result<Self, AiHashError> {
        Self::try_from(s as &str)
    }
}

impl<P: PrimitiveHashType> TryFrom<String> for AiHash<P> {
    type Error = AiHashError;
    fn try_from(s: String) -> Result<Self, AiHashError> {
        Self::try_from(&s)
    }
}

impl<T: HashType> std::fmt::Display for AiHash<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}", ai_hash_encode(self.get_raw_39()))
    }
}

/// internal REPR for ai hash
pub fn ai_hash_encode(data: &[u8]) -> String {
    format!("u{}", URL_SAFE_NO_PAD.encode(data))
}

/// internal PARSE for ai hash REPR
pub fn ai_hash_decode_unchecked(s: &str) -> Result<Vec<u8>, AiHashError> {
    if &s[..1] != "u" {
        return Err(AiHashError::NoU);
    }
    let s = match URL_SAFE_NO_PAD.decode(&s[1..]) {
        Err(_) => return Err(AiHashError::BadBase64),
        Ok(s) => s,
    };
    if s.len() != AI_HASH_FULL_LEN {
        return Err(AiHashError::BadSize);
    }
    let loc_bytes =
        ai_sgd_location_bytes(&s[AI_HASH_PREFIX_LEN..AI_HASH_PREFIX_LEN + AI_HASH_CORE_LEN]);
    let loc_bytes: &[u8] = &loc_bytes;
    if loc_bytes != &s[AI_HASH_PREFIX_LEN + AI_HASH_CORE_LEN..] {
        return Err(AiHashError::BadChecksum);
    }
    assert_length!(AI_HASH_FULL_LEN, &s);
    Ok(s.to_vec())
}

/// internal PARSE for ai hash REPR
pub fn ai_hash_decode(prefix: &[u8], s: &str) -> Result<Vec<u8>, AiHashError> {
    if &s[..1] != "u" {
        return Err(AiHashError::NoU);
    }
    let s = match URL_SAFE_NO_PAD.decode(&s[1..]) {
        Err(_) => return Err(AiHashError::BadBase64),
        Ok(s) => s,
    };
    if s.len() != AI_HASH_FULL_LEN {
        return Err(AiHashError::BadSize);
    }
    let actual_prefix: [u8; AI_HASH_PREFIX_LEN] = s[..AI_HASH_PREFIX_LEN].try_into().unwrap();
    if actual_prefix != prefix {
        return Err(AiHashError::BadPrefix(
            format!("{:?}", prefix),
            actual_prefix,
        ));
    }
    let loc_bytes =
        ai_sgd_location_bytes(&s[AI_HASH_PREFIX_LEN..AI_HASH_PREFIX_LEN + AI_HASH_CORE_LEN]);
    let loc_bytes: &[u8] = &loc_bytes;
    if loc_bytes != &s[AI_HASH_PREFIX_LEN + AI_HASH_CORE_LEN..] {
        return Err(AiHashError::BadChecksum);
    }
    assert_length!(AI_HASH_FULL_LEN, &s);
    Ok(s.to_vec())
}

/// internal compute the ai sgd location u32
pub fn ai_sgd_location_bytes(data: &[u8]) -> Vec<u8> {
    // Assert the data size is relatively small so we are
    // comfortable executing this synchronously / blocking tokio thread.
    assert_eq!(32, data.len(), "only 32 byte hashes supported");

    let hash = blake2b_128(data);
    let mut out = vec![hash[0], hash[1], hash[2], hash[3]];
    for i in (4..16).step_by(4) {
        out[0] ^= hash[i];
        out[1] ^= hash[i + 1];
        out[2] ^= hash[i + 2];
        out[3] ^= hash[i + 3];
    }
    out
}

/// internal compute a 32 byte blake2b hash
pub fn blake2b_256(data: &[u8]) -> Vec<u8> {
    let hash = blake2b_simd::Params::new().hash_length(32).hash(data);
    hash.as_bytes().to_vec()
}

/// internal compute a 16 byte blake2b hash
pub fn blake2b_128(data: &[u8]) -> Vec<u8> {
    let hash = blake2b_simd::Params::new().hash_length(16).hash(data);
    hash.as_bytes().to_vec()
}
