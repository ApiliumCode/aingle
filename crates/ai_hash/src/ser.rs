//! Defines the serialization rules for AiHashes

use crate::AiHash;
use crate::HashType;
use aingle_middleware_bytes::SerializedBytes;
use aingle_middleware_bytes::SerializedBytesError;
use aingle_middleware_bytes::UnsafeBytes;

impl<T: HashType> serde::Serialize for AiHash<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.get_raw_39())
    }
}

impl<'de, T: HashType> serde::Deserialize<'de> for AiHash<T> {
    fn deserialize<D>(deserializer: D) -> Result<AiHash<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(AiHashVisitor(std::marker::PhantomData))
    }
}

struct AiHashVisitor<T: HashType>(std::marker::PhantomData<T>);

impl<'de, T: HashType> serde::de::Visitor<'de> for AiHashVisitor<T> {
    type Value = AiHash<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a AiHash of primitive hash_type")
    }

    fn visit_bytes<E>(self, h: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if !h.len() == 39 {
            Err(serde::de::Error::custom(
                "AiHash serialized representation must be exactly 39 bytes",
            ))
        } else {
            AiHash::from_raw_39(h.to_vec())
                .map_err(|e| serde::de::Error::custom(format!("AiHash error: {:?}", e)))
        }
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));

        while let Some(b) = seq.next_element()? {
            vec.push(b);
        }

        self.visit_bytes(&vec)
    }

    #[cfg(feature = "string-encoding")]
    fn visit_str<E>(self, b64: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let h = crate::ai_hash_decode_unchecked(b64)
            .map_err(|e| serde::de::Error::custom(format!("AiHash error: {:?}", e)))?;
        if !h.len() == 39 {
            Err(serde::de::Error::custom(
                "AiHash serialized representation must be exactly 39 bytes",
            ))
        } else {
            AiHash::from_raw_39(h.to_vec())
                .map_err(|e| serde::de::Error::custom(format!("AiHash error: {:?}", e)))
        }
    }
}

impl<T: HashType> std::convert::TryFrom<&AiHash<T>> for SerializedBytes {
    type Error = SerializedBytesError;
    fn try_from(t: &AiHash<T>) -> std::result::Result<SerializedBytes, SerializedBytesError> {
        match aingle_middleware_bytes::encode(t) {
            Ok(v) => Ok(SerializedBytes::from(UnsafeBytes::from(v))),
            Err(e) => Err(SerializedBytesError::Serialize(e.to_string())),
        }
    }
}

impl<T: HashType> std::convert::TryFrom<AiHash<T>> for SerializedBytes {
    type Error = SerializedBytesError;
    fn try_from(t: AiHash<T>) -> std::result::Result<SerializedBytes, SerializedBytesError> {
        SerializedBytes::try_from(&t)
    }
}

impl<T: HashType> std::convert::TryFrom<SerializedBytes> for AiHash<T> {
    type Error = SerializedBytesError;
    fn try_from(sb: SerializedBytes) -> std::result::Result<AiHash<T>, SerializedBytesError> {
        match aingle_middleware_bytes::decode(sb.bytes()) {
            Ok(v) => Ok(v),
            Err(e) => Err(SerializedBytesError::Deserialize(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use aingle_middleware_bytes::prelude::*;
    use std::convert::TryInto;

    #[derive(serde::Deserialize, Debug)]
    #[serde(transparent)]
    struct TestByteArray(#[serde(with = "serde_bytes")] Vec<u8>);

    #[test]
    #[cfg(feature = "serialized-bytes")]
    fn test_serialized_bytes_roundtrip() {
        use aingle_middleware_bytes::SerializedBytes;
        use std::convert::TryInto;

        let h_orig = SafHash::from_raw_36(vec![0xdb; AI_HASH_UNTYPED_LEN]);
        let h: SerializedBytes = h_orig.clone().try_into().unwrap();
        let h: SafHash = h.try_into().unwrap();

        assert_eq!(h_orig, h);
        assert_eq!(*h.hash_type(), hash_type::Saf::new());
    }

    #[test]
    fn test_rmp_roundtrip() {
        let h_orig = AgentPubKey::from_raw_36(vec![0xdb; AI_HASH_UNTYPED_LEN]);
        let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
        let h: AgentPubKey = aingle_middleware_bytes::decode(&buf).unwrap();

        assert_eq!(h_orig, h);
        assert_eq!(*h.hash_type(), hash_type::Agent::new());

        // Make sure that the representation is a raw 39-byte array
        let array: TestByteArray = aingle_middleware_bytes::decode(&buf).unwrap();
        assert_eq!(array.0.len(), AI_HASH_FULL_LEN);
        assert_eq!(
            array.0,
            vec![
                132, 32, 36, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219,
                219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219,
                219, 219, 219, 219, 219, 219,
            ]
        );
    }

    #[test]
    fn test_composite_hashtype_roundtrips() {
        {
            let h_orig = AnySgdHash::from_raw_36_and_type(
                vec![0xdb; AI_HASH_UNTYPED_LEN],
                hash_type::AnySgd::Header,
            );
            let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
            let h: AnySgdHash = aingle_middleware_bytes::decode(&buf).unwrap();
            assert_eq!(h_orig, h);
            assert_eq!(*h.hash_type(), hash_type::AnySgd::Header);
        }
        {
            let h_orig = AnySgdHash::from_raw_36_and_type(
                vec![0xdb; AI_HASH_UNTYPED_LEN],
                hash_type::AnySgd::Entry,
            );
            let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
            let h: AnySgdHash = aingle_middleware_bytes::decode(&buf).unwrap();
            assert_eq!(h_orig, h);
            assert_eq!(*h.hash_type(), hash_type::AnySgd::Entry);
        }
        {
            let h_orig = AnySgdHash::from_raw_36_and_type(
                vec![0xdb; AI_HASH_UNTYPED_LEN],
                hash_type::AnySgd::Entry,
            );
            let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
            let h: AnySgdHash = aingle_middleware_bytes::decode(&buf).unwrap();
            assert_eq!(h_orig, h);
            assert_eq!(*h.hash_type(), hash_type::AnySgd::Entry);
        }
    }

    #[test]
    fn test_any_sgd_deserialization() {
        {
            let h_orig =
                EntryHash::from_raw_36_and_type(vec![0xdb; AI_HASH_UNTYPED_LEN], hash_type::Entry);
            let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
            let _: AnySgdHash = aingle_middleware_bytes::decode(&buf).unwrap();
        }
        {
            let h_orig = HeaderHash::from_raw_36_and_type(
                vec![0xdb; AI_HASH_UNTYPED_LEN],
                hash_type::Header,
            );
            let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
            let _: AnySgdHash = aingle_middleware_bytes::decode(&buf).unwrap();
        }
    }

    #[test]
    #[should_panic]
    fn test_any_sgd_deserialization_crossover_error() {
        {
            let h_orig =
                SgdOpHash::from_raw_36_and_type(vec![0xdb; AI_HASH_UNTYPED_LEN], hash_type::SgdOp);
            let buf = aingle_middleware_bytes::encode(&h_orig).unwrap();
            let _: AnySgdHash = aingle_middleware_bytes::decode(&buf).unwrap();
        }
    }

    #[test]
    fn test_struct_to_struct_roundtrip() {
        #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, SerializedBytes)]
        struct TestData {
            e: EntryHash,
            h: HeaderHash,
        }

        let orig = TestData {
            e: EntryHash::from_raw_36_and_type(vec![0xdb; AI_HASH_UNTYPED_LEN], hash_type::Entry),
            h: HeaderHash::from_raw_36(vec![0xdb; AI_HASH_UNTYPED_LEN]),
        };

        let sb: SerializedBytes = (&orig).try_into().unwrap();
        let res: TestData = sb.try_into().unwrap();

        assert_eq!(orig, res);
        assert_eq!(*orig.e.hash_type(), hash_type::Entry);
        assert_eq!(*orig.h.hash_type(), hash_type::Header);
    }

    #[test]
    fn test_json_to_rust() {
        #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, SerializedBytes)]
        struct Data {
            any_hash: AnySgdHash,
            content: String,
        }

        let any_hash = AnySgdHash::from_raw_36_and_type(
            b"000000000000000000000000000000000000".to_vec(),
            hash_type::AnySgd::Header,
        );
        let hash_type_sb: SerializedBytes = any_hash.hash_type().try_into().unwrap();
        let hash_type_json = r#"{"Header":[132,41,36]}"#;
        assert_eq!(format!("{:?}", hash_type_sb), hash_type_json.to_string());

        let hash_type_from_sb: hash_type::AnySgd = hash_type_sb.try_into().unwrap();
        assert_eq!(hash_type_from_sb, hash_type::AnySgd::Header);

        let hash_type_from_json: hash_type::AnySgd = serde_json::from_str(&hash_type_json).unwrap();
        assert_eq!(hash_type_from_json, hash_type::AnySgd::Header);
    }

    #[test]
    fn test_generic_content_roundtrip() {
        #[derive(Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        struct Generic<K> {
            bytes: Vec<u8>,
            __marker: std::marker::PhantomData<K>,
        }

        impl<K> Generic<K>
        where
            K: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
            // V: Serialize + DeserializeOwned + std::fmt::Debug,
        {
            fn new() -> Self {
                Self {
                    bytes: Vec::new(),
                    __marker: Default::default(),
                }
            }

            fn get(&self) -> K {
                aingle_middleware_bytes::decode(&self.bytes).unwrap()
            }

            fn put(&mut self, k: &K) {
                self.bytes = aingle_middleware_bytes::encode(k).unwrap();
            }
        }

        let mut g: Generic<HeaderHash> = Generic::new();
        let h = HeaderHash::from_raw_36(vec![0xdb; AI_HASH_UNTYPED_LEN]);
        g.put(&h);
        assert_eq!(h, g.get());
    }
}
