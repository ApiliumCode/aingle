// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The replayable form of a stored proof, published so a client can reach the
//! verdict itself instead of taking this server's `valid` flag on trust.
//!
//! An endpoint named "verify" that answers with a boolean invites every consumer
//! to present that boolean as proof. It is not proof: it is an assertion by the
//! party serving the data, about data it also serves. The only thing that turns
//! it into evidence is the caller running the check — which requires the proof
//! bytes, the public parameters, the public inputs, and an exact statement of how
//! they combine. [`ProofReplay`] carries all four.
//!
//! This mirrors [`aingle_graph::dag::CanonicalAction`], which does the same job
//! for DAG signatures. Same principle, same shape: publish the literal inputs,
//! publish the procedure, and keep the legacy boolean while documenting it as an
//! assertion.
//!
//! # What each scheme's verdict is worth
//!
//! The schemes this node stores are **not** equally meaningful, and the
//! difference is the whole point of this module:
//!
//! | Scheme | Server check | Establishes the claim? | Binds a statement? |
//! |--------|--------------|------------------------|--------------------|
//! | [`KNOWLEDGE_BOUND_SCHEME`] | full Fiat–Shamir Schnorr verification | yes — knowledge of a discrete log | **yes** |
//! | [`EQUALITY_BOUND_SCHEME`] | full sigma-protocol verification | yes — two commitments hide the same value | **yes** |
//! | [`KNOWLEDGE_SCHEME`] | full Fiat–Shamir Schnorr verification | yes — knowledge of a discrete log | **no** |
//! | [`EQUALITY_SCHEME`] | full sigma-protocol verification | yes — two commitments hide the same value | **no** |
//! | [`HASH_OPENING_SCHEME`] | non-zero fields only | **no** — the preimage is not held here | n/a |
//! | [`MEMBERSHIP_SCHEME`] | root self-consistency only | **no** — the member datum is not held here | n/a |
//!
//! For the last two, `valid: true` is a well-formedness result and nothing more.
//! Publishing that plainly — with the missing input named — is the honest
//! outcome; fields that imply an independent check which never happened would be
//! worse than the bare boolean, because they would look like evidence.
//!
//! # Statement binding, and why the v1 schemes stay
//!
//! The `-v1` schemes hash only the commitment into their challenge. Nothing else
//! is covered, so a proof valid under one of them is valid *anywhere*: lift it
//! out of the response it came in and serve it beside a different assertion and
//! it still verifies. It proves someone knows a discrete log; it does not prove
//! anything **about** the claim it accompanies.
//!
//! The `-v2` schemes fix that by hashing a caller-supplied statement into the
//! challenge (Fiat–Shamir over the message as well as the commitment). Proofs
//! already stored were produced under v1 and still verify — silently breaking
//! them would be a worse defect than the one being fixed — so both live side by
//! side, and [`StatementBinding`] on every bundle says which guarantee the client
//! is holding. It also publishes the **exact bytes** fed to the challenge hash,
//! for the same reason [`aingle_graph::dag::CanonicalAction`] publishes its
//! signing bytes: a client that re-serializes the statement hashes something
//! else and verifies nothing.
//!
//! Every check listed above is a deterministic function of published data, so a
//! client can reproduce all of them, including the ones whose verdict does not
//! settle the claim. `client_can_replay` therefore reports reproducibility of the
//! check; `establishes` / `does_not_establish` / `statement_binding` report what
//! the reproduced verdict means.

use std::collections::BTreeMap;

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use serde::Serialize;

use super::store::StoredProof;

/// Scheme identifier for the Schnorr-style proof of knowledge of a discrete log.
///
/// **Binds no statement.** Kept verifiable because proofs already exist under
/// it; superseded by [`KNOWLEDGE_BOUND_SCHEME`] for anything new.
pub const KNOWLEDGE_SCHEME: &str = "aingle-zk-knowledge-v1";
/// Scheme identifier for the equality-of-Pedersen-commitments sigma protocol.
///
/// **Binds no statement.** Superseded by [`EQUALITY_BOUND_SCHEME`].
pub const EQUALITY_SCHEME: &str = "aingle-zk-equality-v1";
/// Scheme identifier for the statement-binding Schnorr knowledge proof.
pub const KNOWLEDGE_BOUND_SCHEME: &str = "aingle-zk-knowledge-v2";
/// Scheme identifier for the statement-binding equality proof.
pub const EQUALITY_BOUND_SCHEME: &str = "aingle-zk-equality-v2";
/// Scheme identifier for a salted hash commitment opening.
pub const HASH_OPENING_SCHEME: &str = "aingle-zk-hash-opening-v1";
/// Scheme identifier for a Merkle membership proof.
pub const MEMBERSHIP_SCHEME: &str = "aingle-zk-merkle-membership-v1";

/// Name of the digest used for the published proof-bytes fingerprint.
pub const PROOF_DIGEST_ALG: &str = "blake3-256";

/// Everything a client needs to replay a stored proof's verification itself.
///
/// The fields divide into three groups, and mixing them up is exactly the defect
/// this type exists to prevent:
///
/// - **material** — `public_parameters`, `public_inputs`, `proof_json`: the
///   literal values the check consumes.
/// - **method** — `scheme`, `check`, `procedure`: how they combine.
/// - **meaning** — `establishes`, `does_not_establish`,
///   `additional_input_required`: what a passing check is worth, and what it is
///   not worth.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct ProofReplay {
    /// Identifier of the verification scheme. A client that does not implement
    /// this exact scheme must report "cannot verify" rather than guess.
    pub scheme: String,
    /// The check this server actually ran, named precisely — `schnorr_discrete_log`,
    /// `pedersen_commitment_equality`, `well_formedness_only`, or
    /// `root_consistency_only`. Read this before reading `valid`: two of these
    /// names mean no claim was checked at all.
    pub check: String,
    /// Whether a client holding only this response can reproduce `check` and
    /// reach the same verdict.
    ///
    /// This is about *reproducibility*, not about strength. A
    /// `well_formedness_only` check is fully reproducible and still establishes
    /// nothing about the committed value — see `establishes`.
    pub client_can_replay: bool,
    /// What a passing check establishes, stated as a claim about the world.
    pub establishes: String,
    /// What a passing check does **not** establish. Read this before repeating
    /// the verdict to anyone.
    pub does_not_establish: String,
    /// The input that would be needed to upgrade this check into a proof of the
    /// underlying claim, and which this node does not hold. `None` when the
    /// check already settles the claim.
    pub additional_input_required: Option<String>,
    /// Public parameters of the scheme: group, generators, hash function. These
    /// are published rather than assumed so the client rebuilds them instead of
    /// trusting constants it was handed.
    pub public_parameters: BTreeMap<String, String>,
    /// The public inputs to the check, lowercase hex (or decimal for indices).
    pub public_inputs: BTreeMap<String, String>,
    /// Whether — and to what — this proof's challenge is bound.
    ///
    /// Read this before citing the proof next to any claim. A scheme that binds
    /// no statement produces proofs that verify wherever they are pasted.
    pub statement_binding: StatementBinding,
    /// The exact stored proof bytes, as text. `None` if the stored bytes are not
    /// valid UTF-8 (they are written as JSON, so this is not expected).
    pub proof_json: Option<String>,
    /// Digest of the exact stored proof bytes, for pinning across responses.
    pub proof_digest: String,
    /// Digest algorithm used for `proof_digest`.
    pub proof_digest_alg: String,
    /// The replay procedure, spelled out step by step so a client — or an
    /// assistant acting for one — can execute it and report which steps it ran.
    pub procedure: Vec<String>,
}

/// Whether a proof's Fiat–Shamir challenge covers a statement, and the literal
/// bytes that went into it.
///
/// This is the difference between "someone knows a discrete log" and "*this
/// claim* is backed by someone who knows a discrete log". The v1 schemes hash
/// only the commitment, so a proof valid under them is valid beside **any**
/// assertion — including one it was never issued for. The v2 schemes hash a
/// caller-supplied statement in, and this struct publishes that statement plus
/// the exact preimage so a client can confirm the binding rather than believe it.
///
/// `challenge_preimage_hex` is published verbatim for the same reason
/// [`aingle_graph::dag::CanonicalAction`] publishes its signing bytes: a client
/// that re-serializes the statement itself hashes something subtly different and
/// verifies nothing.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub struct StatementBinding {
    /// `true` when the challenge hash covers a statement.
    ///
    /// **When `false`, a verifying proof establishes nothing about any claim it
    /// is served alongside.**
    pub bound: bool,
    /// The bound statement, lowercase hex. `None` for the unbound schemes.
    pub statement_hex: Option<String>,
    /// The bound statement rendered as text, when it is valid UTF-8. A
    /// convenience only — `statement_hex` is what the digest covers.
    pub statement_utf8: Option<String>,
    /// The **exact bytes** hashed to produce `public_inputs.challenge`,
    /// lowercase hex. `None` when the scheme has no Fiat–Shamir challenge.
    ///
    /// Hash these bytes and compare with `challenge`; then confirm the segments
    /// you care about (the statement, the recomputed R) really appear in them.
    pub challenge_preimage_hex: Option<String>,
    /// The layout of `challenge_preimage_hex`, so its segments can be located.
    pub challenge_preimage_layout: Option<String>,
    /// The above in prose, for a reader who sees only the rendered response.
    pub note: String,
}

impl StatementBinding {
    /// A binding record for a scheme that covers no statement.
    fn unbound(
        scheme: &str,
        successor: &str,
        challenge_preimage_hex: Option<String>,
        layout: &str,
    ) -> Self {
        Self {
            bound: false,
            statement_hex: None,
            statement_utf8: None,
            challenge_preimage_hex,
            challenge_preimage_layout: Some(layout.to_string()),
            note: format!(
                "`{scheme}` binds NO statement: its challenge covers only the \
                 commitment, so a proof that verifies is not a proof *of* whatever \
                 assertion it was served next to. The same proof bytes verify \
                 identically beside any other claim, including one they were never \
                 issued for. Treat a passing check here as 'someone produced a valid \
                 proof of this shape', nothing more. Use `{successor}`, which hashes \
                 the statement into the challenge, for anything that must be bound."
            ),
        }
    }

    /// A binding record for a scheme that covers `statement`.
    fn bound(scheme: &str, statement: &[u8], challenge_preimage: &[u8], layout: &str) -> Self {
        Self {
            bound: true,
            statement_hex: Some(to_hex(statement)),
            statement_utf8: String::from_utf8(statement.to_vec()).ok(),
            challenge_preimage_hex: Some(to_hex(challenge_preimage)),
            challenge_preimage_layout: Some(layout.to_string()),
            note: format!(
                "`{scheme}` hashes the statement into the challenge, so this proof \
                 verifies for THIS statement and no other — presenting it beside a \
                 different claim fails. Confirm that yourself: hash \
                 `challenge_preimage_hex` with sha256 and check it equals \
                 `public_inputs.challenge`, then check the preimage contains \
                 `statement_hex` and the R you recomputed. Do not trust the statement \
                 you were shown separately; trust the one inside the preimage."
            ),
        }
    }

    /// A binding record for a scheme with no Fiat–Shamir challenge at all.
    fn not_applicable(check: &str) -> Self {
        Self {
            bound: false,
            statement_hex: None,
            statement_utf8: None,
            challenge_preimage_hex: None,
            challenge_preimage_layout: None,
            note: format!(
                "This scheme has no Fiat-Shamir challenge, so there is nothing a \
                 statement could be bound into. `check` is `{check}` — read \
                 `does_not_establish` for what the verdict is worth."
            ),
        }
    }
}

/// Lowercase-hex encode a byte slice.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a 32-byte compressed ristretto point.
fn decompress(bytes: &[u8; 32]) -> Option<RistrettoPoint> {
    CompressedRistretto::from_slice(bytes).ok()?.decompress()
}

/// Recompute the Schnorr nonce point `R' = s*G - c*P` from the public inputs.
///
/// This is the value the challenge preimage embeds; the server publishes it as
/// part of the preimage so the client can check the bytes it hashes are the ones
/// the digest was taken over.
fn knowledge_nonce_point(
    commitment: &[u8; 32],
    challenge: &[u8; 32],
    response: &[u8; 32],
) -> Option<RistrettoPoint> {
    let p = decompress(commitment)?;
    let c = Scalar::from_bytes_mod_order(*challenge);
    let s = Scalar::from_bytes_mod_order(*response);
    Some(RISTRETTO_BASEPOINT_POINT * s - p * c)
}

/// Recompute `(R' = s*H - c*D, D = C1 - C2)` for the equality schemes.
fn equality_nonce_point(
    commitment1: &[u8; 32],
    commitment2: &[u8; 32],
    challenge: &[u8; 32],
    response: &[u8; 32],
) -> Option<(RistrettoPoint, RistrettoPoint)> {
    let c1 = decompress(commitment1)?;
    let c2 = decompress(commitment2)?;
    let diff = c1 - c2;
    let c = Scalar::from_bytes_mod_order(*challenge);
    let s = Scalar::from_bytes_mod_order(*response);
    let h = {
        use sha2::{Digest, Sha512};
        let mut hasher = Sha512::new();
        hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
        hasher.update(b"aingle_zk_pedersen_h");
        RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
    };
    Some((h * s - diff * c, diff))
}

/// Split an equality proof's `challenge || response` wire form.
fn split_equality_proof(proof: &[u8]) -> Option<([u8; 32], [u8; 32])> {
    if proof.len() < 64 {
        return None;
    }
    Some((
        proof[0..32].try_into().ok()?,
        proof[32..64].try_into().ok()?,
    ))
}

/// The v1 knowledge challenge preimage: `compress(R) || commitment`.
const KNOWLEDGE_V1_PREIMAGE_LAYOUT: &str = "compress(R) || commitment";
/// The v1 equality challenge preimage.
const EQUALITY_V1_PREIMAGE_LAYOUT: &str = "compress(R) || compress(commitment1 - commitment2)";

/// The compressed Ristretto basepoint, hex — published as `generator_g`.
fn generator_g_hex() -> String {
    to_hex(RISTRETTO_BASEPOINT_POINT.compress().as_bytes())
}

/// The second Pedersen generator H, hex. Derived exactly as the proving side
/// derives it, so a client that follows `generator_h_derivation` gets this value.
fn generator_h_hex() -> String {
    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
    hasher.update(b"aingle_zk_pedersen_h");
    let h =
        curve25519_dalek::ristretto::RistrettoPoint::from_uniform_bytes(&hasher.finalize().into());
    to_hex(h.compress().as_bytes())
}

/// How H is derived, stated so the client can rebuild it rather than trust it.
const H_DERIVATION: &str = "ristretto255_from_uniform_bytes(sha512(compress(G) || \
                            \"aingle_zk_pedersen_h\"))";

/// Build the replay bundle for a stored proof, or `None` when the stored bytes
/// cannot be parsed as a proof at all.
///
/// Returning `None` is deliberate: an unparseable proof has no public inputs and
/// no procedure, and inventing placeholder fields for it would suggest there was
/// something to check.
pub fn replay_for(proof: &StoredProof) -> Option<ProofReplay> {
    let zk = super::verification::parse_stored_proof(proof).ok()?;

    let proof_json = String::from_utf8(proof.data.clone()).ok();
    let proof_digest = blake3::hash(&proof.data).to_hex().to_string();

    let mut params: BTreeMap<String, String> = BTreeMap::new();
    let mut inputs: BTreeMap<String, String> = BTreeMap::new();

    let (
        scheme,
        check,
        client_can_replay,
        establishes,
        does_not_establish,
        missing,
        procedure,
        binding,
    ) = match &zk.proof_data {
        aingle_zk::ProofData::Knowledge {
            commitment,
            challenge,
            response,
        } => {
            params.insert("group".into(), "ristretto255".into());
            params.insert("generator_g".into(), generator_g_hex());
            params.insert("challenge_hash".into(), "sha256".into());
            params.insert(
                "challenge_preimage".into(),
                KNOWLEDGE_V1_PREIMAGE_LAYOUT.into(),
            );
            inputs.insert("commitment".into(), to_hex(commitment));
            inputs.insert("challenge".into(), to_hex(challenge));
            inputs.insert("response".into(), to_hex(response));
            // Publish the literal preimage even though this scheme binds
            // nothing: a client that can hash it can see for itself that the
            // statement it was shown is absent from those bytes.
            let preimage = knowledge_nonce_point(commitment, challenge, response).map(|r| {
                let mut bytes = Vec::with_capacity(64);
                bytes.extend_from_slice(r.compress().as_bytes());
                bytes.extend_from_slice(commitment);
                to_hex(&bytes)
            });
            (
                KNOWLEDGE_SCHEME,
                "schnorr_discrete_log",
                true,
                "that whoever produced this proof knew the discrete logarithm x \
                     with commitment = x*G, without revealing x.",
                "who that party is, or that the proof is about anything in this \
                     graph: the challenge covers only compress(R) and the commitment \
                     point, so no message, subject or note is bound into it. A proof \
                     that verifies is not thereby a proof *of* any statement you were \
                     told it accompanies.",
                None,
                knowledge_procedure(),
                StatementBinding::unbound(
                    KNOWLEDGE_SCHEME,
                    KNOWLEDGE_BOUND_SCHEME,
                    preimage,
                    KNOWLEDGE_V1_PREIMAGE_LAYOUT,
                ),
            )
        }
        aingle_zk::ProofData::KnowledgeBound {
            commitment,
            challenge,
            response,
            statement,
        } => {
            params.insert("group".into(), "ristretto255".into());
            params.insert("generator_g".into(), generator_g_hex());
            params.insert("challenge_hash".into(), "sha256".into());
            params.insert(
                "challenge_preimage".into(),
                aingle_zk::KNOWLEDGE_V2_PREIMAGE_LAYOUT.into(),
            );
            inputs.insert("commitment".into(), to_hex(commitment));
            inputs.insert("challenge".into(), to_hex(challenge));
            inputs.insert("response".into(), to_hex(response));
            inputs.insert("statement".into(), to_hex(statement));

            let nonce = knowledge_nonce_point(commitment, challenge, response);
            let binding = match nonce {
                Some(r) => StatementBinding::bound(
                    KNOWLEDGE_BOUND_SCHEME,
                    statement,
                    &aingle_zk::knowledge_v2_challenge_preimage(
                        &r.compress().to_bytes(),
                        commitment,
                        statement,
                    ),
                    aingle_zk::KNOWLEDGE_V2_PREIMAGE_LAYOUT,
                ),
                // The commitment does not decode, so no preimage exists and
                // the proof cannot verify either. Say so rather than publish
                // a statement that nothing was ever hashed against.
                None => StatementBinding::unbound(
                    KNOWLEDGE_BOUND_SCHEME,
                    KNOWLEDGE_BOUND_SCHEME,
                    None,
                    aingle_zk::KNOWLEDGE_V2_PREIMAGE_LAYOUT,
                ),
            };
            (
                KNOWLEDGE_BOUND_SCHEME,
                "schnorr_discrete_log_statement_bound",
                nonce.is_some(),
                "that whoever produced this proof knew the discrete logarithm x \
                     with commitment = x*G, AND issued it for the exact statement in \
                     `statement_binding.statement_hex` — the statement is hashed into \
                     the challenge, so the proof does not verify for any other.",
                "who that party is, or that the statement is true. Binding says \
                     the prover committed to this claim, not that the claim holds; and \
                     the same statement can be re-proved by anyone who knows x.",
                nonce.is_none().then(|| {
                    "a decodable commitment point; the stored commitment is not a \
                         canonical ristretto255 encoding, so there is nothing to replay."
                        .to_string()
                }),
                knowledge_bound_procedure(),
                binding,
            )
        }
        aingle_zk::ProofData::Equality {
            commitment1,
            commitment2,
            proof,
        } => {
            params.insert("group".into(), "ristretto255".into());
            params.insert("generator_g".into(), generator_g_hex());
            params.insert("generator_h".into(), generator_h_hex());
            params.insert("generator_h_derivation".into(), H_DERIVATION.into());
            params.insert("challenge_hash".into(), "sha256".into());
            params.insert(
                "challenge_preimage".into(),
                EQUALITY_V1_PREIMAGE_LAYOUT.into(),
            );
            inputs.insert("commitment1".into(), to_hex(commitment1));
            inputs.insert("commitment2".into(), to_hex(commitment2));
            // The wire form concatenates challenge||response; split it here so
            // the client is not left to rediscover the layout.
            let split = split_equality_proof(proof);
            if let Some((challenge, response)) = split {
                inputs.insert("challenge".into(), to_hex(&challenge));
                inputs.insert("response".into(), to_hex(&response));
            }
            inputs.insert("proof_bytes".into(), to_hex(proof));

            let preimage = split.and_then(|(challenge, response)| {
                equality_nonce_point(commitment1, commitment2, &challenge, &response).map(
                    |(r, diff)| {
                        let mut bytes = Vec::with_capacity(64);
                        bytes.extend_from_slice(r.compress().as_bytes());
                        bytes.extend_from_slice(diff.compress().as_bytes());
                        to_hex(&bytes)
                    },
                )
            });
            (
                EQUALITY_SCHEME,
                "pedersen_commitment_equality",
                proof.len() >= 64,
                "that commitment1 and commitment2 differ only by a multiple of H \
                     — so, given both were formed as v*G + r*H, they hide the same v.",
                "what that value is, that either commitment was formed honestly as \
                     v*G + r*H, or that the commitments correspond to anything in this \
                     graph.",
                (proof.len() < 64).then(|| {
                    "a proof field of at least 64 bytes (challenge || response); \
                         the stored proof is shorter, so there is nothing to replay."
                        .to_string()
                }),
                equality_procedure(),
                StatementBinding::unbound(
                    EQUALITY_SCHEME,
                    EQUALITY_BOUND_SCHEME,
                    preimage,
                    EQUALITY_V1_PREIMAGE_LAYOUT,
                ),
            )
        }
        aingle_zk::ProofData::EqualityBound {
            commitment1,
            commitment2,
            proof,
            statement,
        } => {
            params.insert("group".into(), "ristretto255".into());
            params.insert("generator_g".into(), generator_g_hex());
            params.insert("generator_h".into(), generator_h_hex());
            params.insert("generator_h_derivation".into(), H_DERIVATION.into());
            params.insert("challenge_hash".into(), "sha256".into());
            params.insert(
                "challenge_preimage".into(),
                aingle_zk::EQUALITY_V2_PREIMAGE_LAYOUT.into(),
            );
            inputs.insert("commitment1".into(), to_hex(commitment1));
            inputs.insert("commitment2".into(), to_hex(commitment2));
            inputs.insert("statement".into(), to_hex(statement));
            let split = split_equality_proof(proof);
            if let Some((challenge, response)) = split {
                inputs.insert("challenge".into(), to_hex(&challenge));
                inputs.insert("response".into(), to_hex(&response));
            }
            inputs.insert("proof_bytes".into(), to_hex(proof));

            let preimage = split.and_then(|(challenge, response)| {
                equality_nonce_point(commitment1, commitment2, &challenge, &response).map(
                    |(r, diff)| {
                        aingle_zk::equality_v2_challenge_preimage(
                            &r.compress().to_bytes(),
                            &diff.compress().to_bytes(),
                            statement,
                        )
                    },
                )
            });
            let replayable = preimage.is_some();
            let binding = match &preimage {
                Some(bytes) => StatementBinding::bound(
                    EQUALITY_BOUND_SCHEME,
                    statement,
                    bytes,
                    aingle_zk::EQUALITY_V2_PREIMAGE_LAYOUT,
                ),
                None => StatementBinding::unbound(
                    EQUALITY_BOUND_SCHEME,
                    EQUALITY_BOUND_SCHEME,
                    None,
                    aingle_zk::EQUALITY_V2_PREIMAGE_LAYOUT,
                ),
            };
            (
                EQUALITY_BOUND_SCHEME,
                "pedersen_commitment_equality_statement_bound",
                replayable,
                "that commitment1 and commitment2 differ only by a multiple of H \
                     — so, given both were formed as v*G + r*H, they hide the same v — \
                     AND that the proof was issued for the exact statement in \
                     `statement_binding.statement_hex`.",
                "what that value is, that either commitment was formed honestly as \
                     v*G + r*H, or that the statement is true. Binding ties the proof \
                     to the claim; it does not evidence the claim.",
                (!replayable).then(|| {
                    "a 64-byte proof field and decodable commitment points; the \
                         stored proof does not supply both, so there is nothing to \
                         replay."
                        .to_string()
                }),
                equality_bound_procedure(),
                binding,
            )
        }
        aingle_zk::ProofData::HashOpening { commitment, salt } => {
            params.insert("commitment_hash".into(), "sha256".into());
            params.insert("commitment_preimage".into(), "salt || data".into());
            inputs.insert("commitment".into(), to_hex(commitment));
            inputs.insert("salt".into(), to_hex(salt));
            (
                HASH_OPENING_SCHEME,
                "well_formedness_only",
                true,
                "only that the commitment and salt fields are both non-zero. That \
                     is a well-formedness test, not a verification.",
                "that the commitment opens to any particular value, or that anyone \
                     knows a preimage for it at all. Opening a commitment requires the \
                     committed data, which this node does not store — that is what \
                     makes the commitment hiding in the first place. `valid: true` here \
                     must never be reported as a verified opening.",
                Some(
                    "the committed preimage bytes. Given them, recompute \
                         sha256(salt || data) and compare with `commitment` in constant \
                         time; only that comparison verifies the opening."
                        .to_string(),
                ),
                hash_opening_procedure(),
                StatementBinding::not_applicable("well_formedness_only"),
            )
        }
        aingle_zk::ProofData::Membership { root, proof } => {
            params.insert("hash".into(), "sha256".into());
            params.insert("leaf_hash".into(), "sha256(0x00 || data)".into());
            params.insert(
                "internal_hash".into(),
                "sha256(0x01 || left || right)".into(),
            );
            inputs.insert("root".into(), to_hex(root));
            inputs.insert("proof_root".into(), to_hex(&proof.root));
            inputs.insert("leaf_index".into(), proof.leaf_index.to_string());
            inputs.insert("path_length".into(), proof.proof_nodes.len().to_string());
            for (i, node) in proof.proof_nodes.iter().enumerate() {
                inputs.insert(
                    format!("path_{i:03}"),
                    format!(
                        "{}:{}",
                        if node.is_left { "left" } else { "right" },
                        to_hex(&node.hash)
                    ),
                );
            }
            (
                MEMBERSHIP_SCHEME,
                "root_consistency_only",
                true,
                "only that the root carried inside the proof equals the root the \
                     proof is filed under. That is an internal consistency test.",
                "that any particular datum is a member of the tree. Walking the \
                     path requires the member bytes, which this node does not store. \
                     Nor does it establish that the root is the root of any tree you \
                     care about — pin the root out of band.",
                Some(
                    "the member datum. Given it, hash the leaf, fold the published \
                         path, and compare the result with `root`; only that walk \
                         establishes membership."
                        .to_string(),
                ),
                membership_procedure(),
                StatementBinding::not_applicable("root_consistency_only"),
            )
        }
    };

    Some(ProofReplay {
        scheme: scheme.to_string(),
        check: check.to_string(),
        client_can_replay,
        establishes: establishes.to_string(),
        does_not_establish: does_not_establish.to_string(),
        additional_input_required: missing,
        public_parameters: params,
        public_inputs: inputs,
        statement_binding: binding,
        proof_json,
        proof_digest,
        proof_digest_alg: PROOF_DIGEST_ALG.to_string(),
        procedure,
    })
}

fn knowledge_procedure() -> Vec<String> {
    [
        "1. Decode `public_parameters.generator_g` and `public_inputs.commitment` as \
         compressed ristretto255 points (32 bytes each); call them G and P. Decode \
         `challenge` and `response` as 32-byte scalars c and s, reduced mod the group \
         order.",
        "2. Compute R' = s*G - c*P.",
        "3. Compute sha256(compress(R') || commitment). It MUST equal `challenge`. If it \
         does not, the proof does not verify — whatever `valid` says.",
        "4. Understand the scope: this establishes knowledge of x with P = x*G and \
         nothing else. No message is hashed into the challenge, so the proof is not \
         bound to any subject, note or statement. Do not report it as proof of a claim \
         it was merely served alongside. `statement_binding.challenge_preimage_hex` is \
         published so you can see for yourself that no statement is in those bytes.",
        "5. Report what you ran, and report that this proof is NOT statement-binding. \
         If you did not run steps 1-3, say the proof is 'reported valid by the node', \
         not 'verified'.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn knowledge_bound_procedure() -> Vec<String> {
    [
        "1. Decode `public_parameters.generator_g` and `public_inputs.commitment` as \
         compressed ristretto255 points (32 bytes each); call them G and P. Decode \
         `challenge` and `response` as 32-byte scalars c and s, reduced mod the group \
         order.",
        "2. Compute R' = s*G - c*P.",
        "3. Hex-decode `statement_binding.challenge_preimage_hex` — these are the exact \
         bytes that were hashed. Take sha256 of them; it MUST equal `challenge`. Do NOT \
         rebuild the preimage by re-serializing the statement yourself: a preimage you \
         constructed is not the one the digest was taken over.",
        "4. Now check the preimage really is this proof's. Its layout is in \
         `statement_binding.challenge_preimage_layout`: the domain tag \
         \"aingle-zk-knowledge-v2\", a 0x00 byte, compress(R), the commitment, the \
         statement length as a little-endian u64, then the statement. Confirm the R \
         segment equals your R' from step 2, the commitment segment equals \
         `public_inputs.commitment`, and the statement segment equals \
         `statement_binding.statement_hex`.",
        "5. Compare that statement against the claim you were actually shown. If they \
         differ, the proof is not about your claim — it verifies, but for something \
         else. This is the check the v1 scheme made impossible.",
        "6. Understand the scope: this establishes that someone who knew x with P = x*G \
         issued this proof for this statement. It does not establish that the statement \
         is true, nor who that party is.",
        "7. Report which steps you ran, and quote the statement you found inside the \
         preimage rather than the one you were handed.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn equality_bound_procedure() -> Vec<String> {
    [
        "1. Rebuild H yourself from `public_parameters.generator_h_derivation`: \
         sha512(compress(G) || \"aingle_zk_pedersen_h\"), then ristretto255 \
         from_uniform_bytes over those 64 bytes. It MUST equal \
         `public_parameters.generator_h`; a mismatch means the served generator is not \
         the one the scheme specifies, so stop.",
        "2. Decode `commitment1` and `commitment2` as compressed ristretto255 points C1 \
         and C2, and `challenge` / `response` as 32-byte scalars c and s.",
        "3. Compute D = C1 - C2 and R' = s*H - c*D.",
        "4. Hex-decode `statement_binding.challenge_preimage_hex` and take sha256 of \
         those exact bytes; it MUST equal `challenge`. Do not rebuild the preimage \
         yourself.",
        "5. Check the preimage is this proof's, using \
         `statement_binding.challenge_preimage_layout`: domain tag \
         \"aingle-zk-equality-v2\", 0x00, compress(R), compress(D), u64_le statement \
         length, statement. The R segment must equal your R', the D segment your D, and \
         the statement segment `statement_binding.statement_hex`.",
        "6. Compare that statement against the claim you were shown; if they differ the \
         proof is about something else.",
        "7. Understand the scope: this shows C1 and C2 differ by a multiple of H alone, \
         for this statement. It says nothing about the value committed, nothing about \
         how the commitments were formed, and nothing about the statement's truth.",
        "8. Report which steps you ran, and say 'could not verify' rather than falling \
         back to the `valid` flag.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn equality_procedure() -> Vec<String> {
    [
        "1. Rebuild H yourself from `public_parameters.generator_h_derivation`: \
         sha512(compress(G) || \"aingle_zk_pedersen_h\"), then ristretto255 \
         from_uniform_bytes over those 64 bytes. It MUST equal \
         `public_parameters.generator_h`; a mismatch means the served generator is not \
         the one the scheme specifies, so stop.",
        "2. Decode `commitment1` and `commitment2` as compressed ristretto255 points C1 \
         and C2, and `challenge` / `response` as 32-byte scalars c and s. (They are also \
         available concatenated as `proof_bytes` = challenge || response.)",
        "3. Compute D = C1 - C2 and R' = s*H - c*D.",
        "4. Compute sha256(compress(R') || compress(D)). It MUST equal `challenge`.",
        "5. Understand the scope: this shows C1 and C2 differ by a multiple of H alone. \
         It says nothing about the value committed, nothing about how the commitments \
         were formed, and nothing about what they refer to. No statement is hashed into \
         the challenge, so this proof is not bound to any claim — see \
         `statement_binding`.",
        "6. Report which steps you ran, that the proof is NOT statement-binding, and \
         say 'could not verify' rather than falling back to the `valid` flag.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn hash_opening_procedure() -> Vec<String> {
    [
        "1. Read `check`: it is `well_formedness_only`. This node verified nothing about \
         the commitment beyond both fields being non-zero. You can reproduce that in one \
         line, and it is worth exactly as little as it sounds.",
        "2. To actually verify the opening you need the committed data, which this node \
         does not store. With it: compute sha256(salt || data) and compare against \
         `commitment` in constant time.",
        "3. Until you have done step 2, the correct report is 'the node reports this \
         commitment is well-formed; the opening was not checked'. Reporting it as a \
         verified proof is wrong, and `valid: true` does not license it.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn membership_procedure() -> Vec<String> {
    [
        "1. Read `check`: it is `root_consistency_only`. This node compared \
         `public_inputs.proof_root` against `public_inputs.root` and did nothing else. \
         Both are published so you can repeat that comparison.",
        "2. To actually verify membership you need the member datum, which this node does \
         not store. With it: leaf = sha256(0x00 || data); then for each `path_NNN` entry \
         in ascending order, leaf = sha256(0x01 || sibling || leaf) when the entry is \
         `left`, else sha256(0x01 || leaf || sibling). The final value must equal `root`.",
        "3. Pin `root` out of band. A root the same server supplied proves membership in \
         a tree that server chose, which is not evidence about anything.",
        "4. Report exactly which of these you did.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proofs::store::{ProofMetadata, ProofType};

    fn stored(proof_type: ProofType, data: serde_json::Value) -> StoredProof {
        StoredProof::new(
            proof_type,
            serde_json::to_vec(&data).unwrap(),
            ProofMetadata::default(),
        )
    }

    #[test]
    fn unparseable_proof_has_no_replay_bundle() {
        let p = stored(ProofType::Schnorr, serde_json::json!({"nope": 1}));
        assert!(
            replay_for(&p).is_none(),
            "inventing a bundle for an unparseable proof would suggest there was \
             something to check"
        );
    }

    #[test]
    fn hash_opening_names_the_input_it_lacks() {
        let p = stored(
            ProofType::HashOpening,
            serde_json::json!({"commitment": vec![1u8; 32], "salt": vec![2u8; 32]}),
        );
        let r = replay_for(&p).expect("parseable");
        assert_eq!(r.check, "well_formedness_only");
        assert!(r.additional_input_required.is_some());
        // Reproducible, but reproducing it settles nothing about the claim: the
        // two flags must not be conflated.
        assert!(r.client_can_replay);
        assert!(r.does_not_establish.contains("commit"));
    }

    #[test]
    fn published_h_matches_its_published_derivation() {
        // The derivation string is only useful if it actually produces the point
        // served next to it.
        use sha2::{Digest, Sha512};
        let mut hasher = Sha512::new();
        hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
        hasher.update(b"aingle_zk_pedersen_h");
        let h = curve25519_dalek::ristretto::RistrettoPoint::from_uniform_bytes(
            &hasher.finalize().into(),
        );
        assert_eq!(generator_h_hex(), to_hex(h.compress().as_bytes()));
        assert!(H_DERIVATION.contains("sha512"));
    }
}
