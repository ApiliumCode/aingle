use super::error::SafError;
use crate::prelude::*;
use ai_hash::*;
use aingle_zome_types::ZomeName;
use std::collections::BTreeMap;

/// Wasms need to be an ordered map from WasmHash to a wasm::SafWasm
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_more::AsRef,
    derive_more::From,
    derive_more::IntoIterator,
)]
#[serde(from = "WasmMapSerialized", into = "WasmMapSerialized")]
pub struct WasmMap(BTreeMap<ai_hash::WasmHash, wasm::SafWasm>);

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct WasmMapSerialized(Vec<(ai_hash::WasmHash, wasm::SafWasm)>);

impl From<WasmMap> for WasmMapSerialized {
    fn from(w: WasmMap) -> Self {
        Self(w.0.into_iter().collect())
    }
}

impl From<WasmMapSerialized> for WasmMap {
    fn from(w: WasmMapSerialized) -> Self {
        Self(w.0.into_iter().collect())
    }
}

/// Represents a full SAF, including SafDef and WebAssembly bytecode.
///
/// Historical note: This struct was written before `SafBundle` was introduced.
/// This used to be our file representation of a full distributable SAF.
/// That function has been superseded by `SafBundle`, but we use this type
/// widely, so there is simply a way to convert from `SafBundle` to `SafFile`.
///
/// TODO: Once we remove the `InstallApp` command which accepts a `SafFile`,
///       we should remove the Serialize impl on this type, and perhaps rename
///       to indicate that this is simply a validated, fully-formed SafBundle
///       (i.e. all Wasms are bundled and immediately available, not remote.)
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, SerializedBytes)]
pub struct SafFile {
    /// The hashable portion that can be shared with hApp code.
    pub(super) saf: SafDefHashed,

    /// The bytes of the WASM zomes referenced in the Saf portion.
    pub(super) code: WasmMap,
}

impl From<SafFile> for (SafDef, Vec<wasm::SafWasm>) {
    fn from(saf_file: SafFile) -> (SafDef, Vec<wasm::SafWasm>) {
        (
            saf_file.saf.into_content(),
            saf_file.code.into_iter().map(|(_, w)| w).collect(),
        )
    }
}

impl SafFile {
    /// Construct a new SafFile instance.
    pub async fn new(
        saf: SafDef,
        wasm: impl IntoIterator<Item = wasm::SafWasm>,
    ) -> Result<Self, SafError> {
        let mut code = BTreeMap::new();
        for wasm in wasm {
            let wasm_hash = ai_hash::WasmHash::with_data(&wasm).await;
            code.insert(wasm_hash, wasm);
        }
        let saf = SafDefHashed::from_content_sync(saf);
        Ok(Self {
            saf,
            code: code.into(),
        })
    }

    /// Construct a SafFile from its constituent parts
    #[cfg(feature = "fixturators")]
    pub fn from_parts(saf: SafDefHashed, code: WasmMap) -> Self {
        Self { saf, code }
    }

    /// The SafDef along with its hash
    pub fn saf(&self) -> &SafDefHashed {
        &self.saf
    }

    /// Just the SafDef
    pub fn saf_def(&self) -> &SafDef {
        &self.saf
    }

    /// The hash of the SafDef
    pub fn saf_hash(&self) -> &ai_hash::SafHash {
        self.saf.as_hash()
    }

    /// Verify that the SAF hash in the file matches the SafDef
    pub fn verify_hash(&self) -> Result<(), SafError> {
        self.saf
            .verify_hash_sync()
            .map_err(|hash| SafError::SafHashMismatch(self.saf.as_hash().clone(), hash))
    }

    /// Load saf_file bytecode into this rust struct.
    #[deprecated = "remove after app bundles become standard; use SafBundle instead"]
    pub async fn from_file_content(data: &[u8]) -> Result<Self, SafError> {
        // Not super efficient memory-wise, but doesn't block any threads
        let data = data.to_vec();
        // Block because gzipping could take some time
        let saf_file = tokio::task::spawn_blocking(move || {
            let mut gz = flate2::read::GzDecoder::new(&data[..]);
            let mut bytes = Vec::new();
            use std::io::Read;
            gz.read_to_end(&mut bytes)?;
            let sb: SerializedBytes = UnsafeBytes::from(bytes).into();
            let saf_file: SafFile = sb.try_into()?;
            SafResult::Ok(saf_file)
        })
        .await
        .expect("blocking thread panicked - panicking here too")?;
        saf_file.verify_hash()?;
        Ok(saf_file)
    }

    /// Transform this SafFile into a new SafFile with different properties
    /// and, hence, a different SafHash.
    pub async fn with_properties(self, properties: SerializedBytes) -> Result<Self, SafError> {
        let (mut saf, wasm): (SafDef, Vec<wasm::SafWasm>) = self.into();
        saf.properties = properties;
        SafFile::new(saf, wasm).await
    }

    /// Transform this SafFile into a new SafFile with a different UID
    /// and, hence, a different SafHash.
    pub async fn with_uid(self, uid: String) -> Result<Self, SafError> {
        let (mut saf, wasm): (SafDef, Vec<wasm::SafWasm>) = self.into();
        saf.uid = uid;
        SafFile::new(saf, wasm).await
    }

    /// The bytes of the WASM zomes referenced in the Saf portion.
    pub fn code(&self) -> &BTreeMap<ai_hash::WasmHash, wasm::SafWasm> {
        &self.code.0
    }

    /// Fetch the Webassembly byte code for a zome.
    pub fn get_wasm_for_zome(&self, zome_name: &ZomeName) -> Result<&wasm::SafWasm, SafError> {
        let wasm_hash = &self.saf.get_wasm_zome(zome_name)?.wasm_hash;
        self.code.0.get(wasm_hash).ok_or(SafError::InvalidWasmHash)
    }

    #[deprecated = "remove after app bundles become standard; use SafBundle instead"]
    /// Render this saf_file as bytecode to send over the wire, or store in a file.
    pub async fn to_file_content(&self) -> Result<Vec<u8>, SafError> {
        // Not super efficient memory-wise, but doesn't block any threads
        let saf_file = self.clone();
        saf_file.verify_hash()?;
        // Block because gzipping could take some time
        tokio::task::spawn_blocking(move || {
            let data: SerializedBytes = saf_file.try_into()?;
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            use std::io::Write;
            enc.write_all(data.bytes())?;
            Ok(enc.finish()?)
        })
        .await
        .expect("blocking thread panic!d - panicing here too")
    }

    /// Change the "phenotype" of this SAF -- the UID and properties -- while
    /// leaving the "genotype" of actual SAF code intact
    pub fn modify_phenotype(&self, uid: Uid, properties: YamlProperties) -> SafResult<Self> {
        let mut clone = self.clone();
        clone.saf = SafDefHashed::from_content_sync(
            clone.saf.modify_phenotype(uid, properties.try_into()?),
        );
        Ok(clone)
    }
}

impl std::fmt::Debug for SafFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("SafFile(saf = {:?})", self.saf))
    }
}
