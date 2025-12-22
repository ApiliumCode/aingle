use aingle_types::prelude::*;
use std::path::Path;

/// Helpful constructors for SafFiles used in tests
#[derive(Clone, Debug, derive_more::From, derive_more::Into, shrinkwraprs::Shrinkwrap)]
pub struct SweetSafFile(SafFile);

impl SweetSafFile {
    /// Create a SafFile from a path to a *.saf bundle
    pub async fn from_bundle(path: &Path) -> SafResult<SafFile> {
        Ok(SafBundle::read_from_file(path)
            .await?
            .into_saf_file(None, None)
            .await?
            .0)
    }

    /// Create a SafFile from a collection of Zomes
    pub async fn from_zomes(
        uid: String,
        zomes: Vec<(ZomeName, ZomeDef)>,
        wasms: Vec<wasm::SafWasm>,
    ) -> SafResult<(SafFile, Vec<Zome>)> {
        let saf_def = SafDefBuilder::default()
            .uid(uid)
            .zomes(zomes.clone())
            .build()
            .unwrap();

        let saf_file = SafFile::new(saf_def, wasms).await?;
        let zomes: Vec<Zome> = zomes.into_iter().map(|(n, z)| Zome::new(n, z)).collect();
        Ok((saf_file, zomes))
    }

    /// Create a SafFile from a collection of Zomes,
    /// with a random UID
    pub async fn unique_from_zomes(
        zomes: Vec<(ZomeName, ZomeDef)>,
        wasms: Vec<wasm::SafWasm>,
    ) -> SafResult<(SafFile, Vec<Zome>)> {
        Self::from_zomes(random_uid(), zomes, wasms).await
    }

    /// Create a SafFile from a collection of TestWasm
    pub async fn from_test_wasms<W>(
        uid: String,
        test_wasms: Vec<W>,
    ) -> SafResult<(SafFile, Vec<Zome>)>
    where
        W: Into<(ZomeName, ZomeDef)> + Into<wasm::SafWasm> + Clone,
    {
        let zomes = test_wasms.clone().into_iter().map(Into::into).collect();
        let wasms = test_wasms.into_iter().map(Into::into).collect();
        Self::from_zomes(uid, zomes, wasms).await
    }

    /// Create a SafFile from a collection of TestWasm
    /// with a random UID
    pub async fn unique_from_test_wasms<W>(test_wasms: Vec<W>) -> SafResult<(SafFile, Vec<Zome>)>
    where
        W: Into<(ZomeName, ZomeDef)> + Into<wasm::SafWasm> + Clone,
    {
        Self::from_test_wasms(random_uid(), test_wasms).await
    }

    /// Create a SafFile from a collection of InlineZomes (no Wasm)
    pub async fn from_inline_zomes(
        uid: String,
        zomes: Vec<(&str, InlineZome)>,
    ) -> SafResult<(SafFile, Vec<Zome>)> {
        Self::from_zomes(
            uid,
            zomes
                .into_iter()
                .map(|(n, z)| (n.into(), z.into()))
                .collect(),
            Vec::new(),
        )
        .await
    }

    /// Create a SafFile from a collection of InlineZomes (no Wasm),
    /// with a random UID
    pub async fn unique_from_inline_zomes(
        zomes: Vec<(&str, InlineZome)>,
    ) -> SafResult<(SafFile, Vec<Zome>)> {
        Self::from_inline_zomes(random_uid(), zomes).await
    }

    /// Create a SafFile from a single InlineZome (no Wasm)
    pub async fn from_inline_zome(
        uid: String,
        zome_name: &str,
        zome: InlineZome,
    ) -> SafResult<(SafFile, Zome)> {
        let (saf_file, mut zomes) = Self::from_inline_zomes(uid, vec![(zome_name, zome)]).await?;
        Ok((saf_file, zomes.pop().unwrap()))
    }

    /// Create a SafFile from a single InlineZome (no Wasm)
    /// with a random UID
    pub async fn unique_from_inline_zome(
        zome_name: &str,
        zome: InlineZome,
    ) -> SafResult<(SafFile, Zome)> {
        Self::from_inline_zome(random_uid(), zome_name, zome).await
    }
}

/// Helpful constructors for SafDefs used in tests
pub struct SweetSafDef;

impl SweetSafDef {
    /// Create a SafDef with a random UID, useful for testing
    // TODO: move fully into sweettest when possible
    pub fn unique_from_zomes(zomes: Vec<Zome>) -> SafDef {
        SafDef::unique_from_zomes(zomes)
    }
}
