//! Cryptographic commitment schemes
//!
//! Commitments allow you to commit to a value without revealing it,
//! then later prove the committed value.

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

use crate::error::{Result, ZkError};

/// Opening information for a Pedersen commitment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentOpening {
    /// The blinding factor (randomness)
    pub blinding: [u8; 32],
}

impl CommitmentOpening {
    /// Create from scalar bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { blinding: bytes }
    }

    /// Get as scalar
    pub fn to_scalar(&self) -> Scalar {
        Scalar::from_bytes_mod_order(self.blinding)
    }
}

/// Pedersen commitment
///
/// A Pedersen commitment to value `v` with blinding factor `r` is:
/// `C = v*G + r*H`
///
/// where G and H are independent generator points.
///
/// Properties:
/// - **Hiding**: Given C, you cannot determine v
/// - **Binding**: Given C, you cannot find different (v', r') such that C = v'*G + r'*H
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PedersenCommitment {
    /// Compressed point representation
    pub point: [u8; 32],
}

impl PedersenCommitment {
    /// Second generator H (derived from hashing G)
    fn generator_h() -> RistrettoPoint {
        let mut hasher = Sha512::new();
        hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
        hasher.update(b"aingle_zk_pedersen_h");
        RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
    }

    /// Create a commitment to a value
    ///
    /// Returns the commitment and opening (blinding factor)
    pub fn commit(value: u64) -> (Self, CommitmentOpening) {
        let mut rng = OsRng;
        let blinding = Scalar::random(&mut rng);
        let value_scalar = Scalar::from(value);

        let g = RISTRETTO_BASEPOINT_POINT;
        let h = Self::generator_h();

        // C = v*G + r*H
        let commitment_point = g * value_scalar + h * blinding;
        let compressed = commitment_point.compress();

        let opening = CommitmentOpening {
            blinding: blinding.to_bytes(),
        };

        (
            Self {
                point: compressed.to_bytes(),
            },
            opening,
        )
    }

    /// Create a commitment with a specific blinding factor
    pub fn commit_with_blinding(value: u64, blinding: &Scalar) -> Self {
        let value_scalar = Scalar::from(value);
        let g = RISTRETTO_BASEPOINT_POINT;
        let h = Self::generator_h();

        let commitment_point = g * value_scalar + h * blinding;
        let compressed = commitment_point.compress();

        Self {
            point: compressed.to_bytes(),
        }
    }

    /// Verify that this commitment opens to the given value
    pub fn verify(&self, value: u64, opening: &CommitmentOpening) -> bool {
        let blinding = opening.to_scalar();
        let expected = Self::commit_with_blinding(value, &blinding);
        self.point == expected.point
    }

    /// Get the commitment as a RistrettoPoint
    pub fn to_point(&self) -> Option<RistrettoPoint> {
        CompressedRistretto::from_slice(&self.point)
            .ok()
            .and_then(|c| c.decompress())
    }

    /// Create from compressed point bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { point: bytes }
    }

    /// Get the commitment bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.point
    }

    /// Add two commitments (homomorphic addition)
    ///
    /// If C1 = v1*G + r1*H and C2 = v2*G + r2*H
    /// Then C1 + C2 = (v1+v2)*G + (r1+r2)*H
    pub fn add(&self, other: &Self) -> Result<Self> {
        let p1 = self
            .to_point()
            .ok_or(ZkError::InvalidInput("Invalid commitment point".into()))?;
        let p2 = other
            .to_point()
            .ok_or(ZkError::InvalidInput("Invalid commitment point".into()))?;
        let sum = (p1 + p2).compress();
        Ok(Self {
            point: sum.to_bytes(),
        })
    }

    /// Subtract two commitments (homomorphic subtraction)
    pub fn sub(&self, other: &Self) -> Result<Self> {
        let p1 = self
            .to_point()
            .ok_or(ZkError::InvalidInput("Invalid commitment point".into()))?;
        let p2 = other
            .to_point()
            .ok_or(ZkError::InvalidInput("Invalid commitment point".into()))?;
        let diff = (p1 - p2).compress();
        Ok(Self {
            point: diff.to_bytes(),
        })
    }
}

/// Simple hash-based commitment
///
/// Commit to a value by hashing it with random salt.
/// Less sophisticated than Pedersen but simpler and faster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashCommitment {
    /// The commitment hash
    pub hash: [u8; 32],
    /// Salt used (for opening)
    pub salt: [u8; 32],
}

impl HashCommitment {
    /// Create a commitment to arbitrary data
    pub fn commit(data: &[u8]) -> Self {
        let mut rng = OsRng;
        let mut salt = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rng, &mut salt);

        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(data);
        let hash: [u8; 32] = hasher.finalize().into();

        Self { hash, salt }
    }

    /// Commit with a specific salt (deterministic)
    pub fn commit_with_salt(data: &[u8], salt: [u8; 32]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(data);
        let hash: [u8; 32] = hasher.finalize().into();

        Self { hash, salt }
    }

    /// Verify that this commitment opens to the given data
    pub fn verify(&self, data: &[u8]) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(self.salt);
        hasher.update(data);
        let computed: [u8; 32] = hasher.finalize().into();
        self.hash == computed
    }

    /// Get the commitment hash as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.hash)
    }

    /// Get commitment hash bytes
    pub fn hash_bytes(&self) -> &[u8; 32] {
        &self.hash
    }
}

/// Blinded commitment for confidential values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindedValue {
    /// The Pedersen commitment
    pub commitment: PedersenCommitment,
    /// Encrypted value (for authorized parties)
    pub encrypted_value: Option<Vec<u8>>,
}

impl BlindedValue {
    /// Create a blinded value
    pub fn new(value: u64) -> (Self, CommitmentOpening) {
        let (commitment, opening) = PedersenCommitment::commit(value);
        (
            Self {
                commitment,
                encrypted_value: None,
            },
            opening,
        )
    }

    /// Verify the blinded value
    pub fn verify(&self, value: u64, opening: &CommitmentOpening) -> bool {
        self.commitment.verify(value, opening)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pedersen_commitment() {
        let value = 42u64;
        let (commitment, opening) = PedersenCommitment::commit(value);

        // Should verify with correct value
        assert!(commitment.verify(value, &opening));

        // Should not verify with wrong value
        assert!(!commitment.verify(43u64, &opening));
    }

    #[test]
    fn test_pedersen_homomorphic_addition() {
        let v1 = 100u64;
        let v2 = 50u64;

        let (c1, o1) = PedersenCommitment::commit(v1);
        let (c2, o2) = PedersenCommitment::commit(v2);

        // Add commitments
        let c_sum = c1.add(&c2).unwrap();

        // Combined opening
        let blinding_sum =
            Scalar::from_bytes_mod_order(o1.blinding) + Scalar::from_bytes_mod_order(o2.blinding);
        let opening_sum = CommitmentOpening {
            blinding: blinding_sum.to_bytes(),
        };

        // Sum commitment should verify to sum of values
        assert!(c_sum.verify(v1 + v2, &opening_sum));
    }

    #[test]
    fn test_hash_commitment() {
        let data = b"secret value";
        let commitment = HashCommitment::commit(data);

        // Should verify with correct data
        assert!(commitment.verify(data));

        // Should not verify with wrong data
        assert!(!commitment.verify(b"wrong value"));
    }

    #[test]
    fn test_hash_commitment_deterministic() {
        let data = b"test data";
        let salt = [1u8; 32];

        let c1 = HashCommitment::commit_with_salt(data, salt);
        let c2 = HashCommitment::commit_with_salt(data, salt);

        assert_eq!(c1.hash, c2.hash);
    }

    #[test]
    fn test_blinded_value() {
        let value = 1000u64;
        let (blinded, opening) = BlindedValue::new(value);

        assert!(blinded.verify(value, &opening));
        assert!(!blinded.verify(999u64, &opening));
    }

    #[test]
    fn test_commitment_serialization() {
        let (commitment, opening) = PedersenCommitment::commit(42u64);

        let json = serde_json::to_string(&commitment).unwrap();
        let deserialized: PedersenCommitment = serde_json::from_str(&json).unwrap();

        assert_eq!(commitment.point, deserialized.point);
        assert!(deserialized.verify(42u64, &opening));
    }
}
