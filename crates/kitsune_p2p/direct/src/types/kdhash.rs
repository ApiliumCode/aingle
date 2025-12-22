//! kdirect kdhash type

use crate::*;
use futures::future::{BoxFuture, FutureExt};
use kitsune_p2p::*;

pub use kitsune_p2p_direct_api::KdHash;

/// Extension trait to augment the direct_api version of KdHash
pub trait KdHashExt: Sized {
    /// convert to kitsune space
    fn to_kitsune_space(&self) -> Arc<KitsuneSpace>;

    /// convert from kitsune space
    fn from_kitsune_space(space: &KitsuneSpace) -> Self;

    /// convert to kitsune agent
    fn to_kitsune_agent(&self) -> Arc<KitsuneAgent>;

    /// convert from kitsune agent
    fn from_kitsune_agent(agent: &KitsuneAgent) -> Self;

    /// convert to kitsune op hash
    fn to_kitsune_op_hash(&self) -> Arc<KitsuneOpHash>;

    /// convert from kitsune op hash
    fn from_kitsune_op_hash(op_hash: &KitsuneOpHash) -> Self;

    /// Treating this hash as a sodoken pubkey,
    /// verify the given data / signature
    fn verify_signature(
        &self,
        data: sodoken::BufRead,
        signature: Arc<[u8; 64]>,
    ) -> BoxFuture<'static, bool>;

    /// Generate a KdHash from data
    fn from_data(data: &[u8]) -> BoxFuture<'static, KdResult<Self>>;

    /// Coerce 32 bytes of signing pubkey data into a KdHash
    fn from_coerced_pubkey(data: [u8; 32]) -> BoxFuture<'static, KdResult<Self>>;
}

impl KdHashExt for KdHash {
    fn to_kitsune_space(&self) -> Arc<KitsuneSpace> {
        Arc::new(KitsuneSpace(self.0 .1[3..].to_vec()))
    }

    fn from_kitsune_space(space: &KitsuneSpace) -> Self {
        (*arrayref::array_ref![&space.0, 0, 36]).into()
    }

    fn to_kitsune_agent(&self) -> Arc<KitsuneAgent> {
        Arc::new(KitsuneAgent(self.0 .1[3..].to_vec()))
    }

    fn from_kitsune_agent(agent: &KitsuneAgent) -> Self {
        (*arrayref::array_ref![&agent.0, 0, 36]).into()
    }

    fn to_kitsune_op_hash(&self) -> Arc<KitsuneOpHash> {
        Arc::new(KitsuneOpHash(self.0 .1[3..].to_vec()))
    }

    fn from_kitsune_op_hash(op_hash: &KitsuneOpHash) -> Self {
        (*arrayref::array_ref![&op_hash.0, 0, 36]).into()
    }

    /// Treating this hash as a sodoken pubkey,
    /// verify the given data / signature
    fn verify_signature(
        &self,
        data: sodoken::BufRead,
        signature: Arc<[u8; 64]>,
    ) -> BoxFuture<'static, bool> {
        let mut pk_bytes = [0u8; 32];
        pk_bytes.copy_from_slice(self.as_core_bytes());
        let pk = sodoken::BufReadSized::from(pk_bytes);
        async move {
            async {
                let mut sig_bytes = [0u8; 64];
                sig_bytes.copy_from_slice(&signature[..]);
                let sig = sodoken::BufReadSized::from(sig_bytes);
                sodoken::sign::verify_detached(sig, data, pk)
                    .await
                    .map_err(KdError::other)
            }
            .await
            .unwrap_or(false)
        }
        .boxed()
    }

    /// Generate a KdHash from data
    fn from_data(data: &[u8]) -> BoxFuture<'static, KdResult<Self>> {
        let r = sodoken::BufRead::from(data.to_vec());
        async move {
            let hash = sodoken::BufWriteSized::<32>::new_no_lock();
            sodoken::hash::blake2b::hash(hash.clone(), r)
                .await
                .map_err(KdError::other)?;
            let mut out = [0; 32];
            out.copy_from_slice(&hash.read_lock()[0..32]);

            // we can use the coerce function now that we have a real hash
            // for the data... even though it's not a pubkey--DRY
            Self::from_coerced_pubkey(out).await
        }
        .boxed()
    }

    /// Coerce 32 bytes of signing pubkey data into a KdHash
    fn from_coerced_pubkey(data: [u8; 32]) -> BoxFuture<'static, KdResult<Self>> {
        async move {
            let r = sodoken::BufRead::from(data.to_vec());
            let loc = loc_hash(r).await?;

            let mut out = [0; 36];
            out[0..32].copy_from_slice(&data);
            out[32..].copy_from_slice(&loc);

            Ok(out.into())
        }
        .boxed()
    }
}

async fn loc_hash(d: sodoken::BufRead) -> KdResult<[u8; 4]> {
    let mut out = [0; 4];

    let hash = sodoken::BufWriteSized::<16>::new_no_lock();
    sodoken::hash::blake2b::hash(hash.clone(), d)
        .await
        .map_err(KdError::other)?;

    let hash = hash.read_lock();
    out[0] = hash[0];
    out[1] = hash[1];
    out[2] = hash[2];
    out[3] = hash[3];
    for i in (4..16).step_by(4) {
        out[0] ^= hash[i];
        out[1] ^= hash[i + 1];
        out[2] ^= hash[i + 2];
        out[3] ^= hash[i + 3];
    }

    Ok(out)
}
