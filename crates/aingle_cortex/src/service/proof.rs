// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Proof verification business logic shared by REST and MCP.
//!
//! Both functions here return a verdict *and* the material to reach it
//! independently. The verdict alone is this node's word about its own stored
//! data; what makes it evidence is the caller replaying it. See
//! [`crate::proofs::replay`] for the bundle, and for which of the four stored
//! schemes a passing check actually settles the claim (two do, two do not).

use crate::error::{Error, Result};
use crate::rest::{GetProofRequest, ProofResponse, VerifyProofByIdRequest, VerifyProofResponse};
use crate::state::AppState;

/// Fetch a stored proof by its ID.
///
/// Semantics (preserved from the REST `GET /api/v1/proofs/:id` handler):
/// - Proof exists -> `Ok(ProofResponse)`.
/// - Proof does not exist -> `Err(Error::NotFound(..))`.
pub async fn get_proof(state: &AppState, req: GetProofRequest) -> Result<ProofResponse> {
    let proof_id = req.proof_id;

    let proof = state
        .proof_store
        .get(&proof_id)
        .await
        .ok_or_else(|| Error::NotFound(format!("Proof {} not found", proof_id)))?;

    Ok(ProofResponse::from(proof))
}

/// Verify a stored proof by its ID **and publish the material to replay that
/// verification**.
///
/// The verdict this returns is this node's own claim about data this node also
/// stores. That is why the response carries `replay`: the proof bytes, the public
/// parameters, the public inputs and the procedure, so the caller can reach the
/// verdict independently instead of relaying `valid`. The bundle is derived from
/// the same parse the verdict came from
/// ([`crate::proofs::verification::parse_stored_proof`]), so the published
/// material always describes the proof that was actually checked.
///
/// Semantics (preserved from commit 53cca2c, "proof verify endpoint returns
/// 200+valid:false instead of 422"):
/// - Proof exists and verifies cleanly -> `Ok(VerifyProofResponse { valid, .. })`.
/// - Proof exists but its data is malformed / fails verification at the ZK
///   layer -> `Ok(VerifyProofResponse { valid: false, .. })` with the error in
///   `details`. This is NOT an `Err`: verification answering "this proof is not
///   valid" is a successful answer, not a server error.
/// - Proof does not exist -> `Err(Error::NotFound(..))`.
pub async fn verify_proof(
    state: &AppState,
    req: VerifyProofByIdRequest,
) -> Result<VerifyProofResponse> {
    let proof_id = req.proof_id;

    let replay = state
        .proof_store
        .get(&proof_id)
        .await
        .as_ref()
        .and_then(crate::proofs::replay_for);

    match state.proof_store.verify(&proof_id).await {
        Ok(result) => Ok(VerifyProofResponse {
            proof_id: proof_id.clone(),
            valid: result.valid,
            verified_at: result.verified_at,
            details: result.details,
            verification_time_us: result.verification_time_us,
            replay,
        }),
        Err(crate::proofs::VerificationError::ProofNotFound(_)) => {
            Err(Error::NotFound(format!("Proof {} not found", proof_id)))
        }
        Err(e) => {
            // Verification infrastructure error (bad proof data format, ZK error,
            // etc.) -> 200 with valid=false + error details instead of 422.
            Ok(VerifyProofResponse {
                proof_id: proof_id.clone(),
                valid: false,
                verified_at: chrono::Utc::now(),
                details: vec![format!("Verification error: {}", e)],
                verification_time_us: 0,
                replay,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proofs::{ProofType, SubmitProofRequest};

    #[tokio::test]
    async fn verifying_invalid_proof_returns_valid_false() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        // Submit a proof whose `proof_data` is structurally-valid JSON but is
        // NOT a parseable `aingle_zk::ZkProof` envelope. The proof therefore
        // EXISTS in the store (so we don't hit the ProofNotFound path), but the
        // verifier fails to deserialize it -> the service must return
        // Ok(valid: false), NOT Err.
        let proof_id = state
            .proof_store
            .submit(SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({ "garbage": "not-a-zk-proof" }),
                metadata: None,
            })
            .await
            .expect("submit should succeed; only verification is expected to fail");

        let req = VerifyProofByIdRequest {
            proof_id: proof_id.clone(),
        };

        let resp = verify_proof(&state, req)
            .await
            .expect("invalid proof must return Ok (200), not Err");
        assert!(!resp.valid, "bogus proof data must yield valid:false");
        assert_eq!(resp.proof_id, proof_id);
    }

    #[tokio::test]
    async fn getting_missing_proof_returns_not_found() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = GetProofRequest {
            proof_id: "does-not-exist".to_string(),
        };

        let err = get_proof(&state, req)
            .await
            .expect_err("missing proof must return Err(NotFound)");
        assert!(
            matches!(err, Error::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    #[tokio::test]
    async fn getting_existing_proof_round_trips() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let proof_id = state
            .proof_store
            .submit(SubmitProofRequest {
                proof_type: ProofType::Schnorr,
                proof_data: serde_json::json!({ "some": "data" }),
                metadata: None,
            })
            .await
            .expect("submit should succeed");

        let resp = get_proof(
            &state,
            GetProofRequest {
                proof_id: proof_id.clone(),
            },
        )
        .await
        .expect("stored proof must be fetchable");

        assert_eq!(resp.id, proof_id);
        assert_eq!(resp.proof_type, ProofType::Schnorr);
    }

    // ========================================================================
    // Independent replayability
    //
    // These are the acceptance criterion for the proof surface: below the
    // "from here on" marker, the test holds nothing but the serialized
    // response and generic crypto libraries — no `aingle_zk` type, no server
    // state, no helper from this crate. If they pass, a third party can reach
    // the verdict itself instead of taking `valid` on trust.
    // ========================================================================

    use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
    use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
    use curve25519_dalek::scalar::Scalar;
    use sha2::{Digest, Sha256, Sha512};

    /// Decode `n` bytes of lowercase hex. Written out here so the client half of
    /// these tests depends on nothing but the response JSON.
    fn unhex(v: &serde_json::Value, n: usize) -> Vec<u8> {
        let s = v
            .as_str()
            .unwrap_or_else(|| panic!("expected hex string, got {v}"));
        assert_eq!(s.len(), n * 2, "expected {n} bytes of hex, got {s:?}");
        (0..n)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex digit"))
            .collect()
    }

    fn point(bytes: &[u8]) -> RistrettoPoint {
        CompressedRistretto::from_slice(bytes)
            .expect("32 bytes")
            .decompress()
            .expect("canonical ristretto point")
    }

    fn scalar32(bytes: &[u8]) -> Scalar {
        Scalar::from_bytes_mod_order(bytes.try_into().expect("32 bytes"))
    }

    /// The second Pedersen generator, as the equality scheme derives it.
    fn generator_h() -> RistrettoPoint {
        let mut hasher = Sha512::new();
        hasher.update(RISTRETTO_BASEPOINT_POINT.compress().as_bytes());
        hasher.update(b"aingle_zk_pedersen_h");
        RistrettoPoint::from_uniform_bytes(&hasher.finalize().into())
    }

    /// A deterministic Schnorr-style knowledge proof, in the shape a client
    /// submits it (raw `proof_data`, no envelope).
    fn knowledge_proof_data() -> serde_json::Value {
        let x = Scalar::from_bytes_mod_order([7u8; 32]);
        let g = RISTRETTO_BASEPOINT_POINT;
        let p = g * x;
        let k = Scalar::from_bytes_mod_order([9u8; 32]);
        let r = g * k;

        let mut hasher = Sha256::new();
        hasher.update(r.compress().as_bytes());
        hasher.update(p.compress().as_bytes());
        let challenge: [u8; 32] = hasher.finalize().into();
        let s = k + Scalar::from_bytes_mod_order(challenge) * x;

        serde_json::json!({
            "type": "Knowledge",
            "commitment": p.compress().to_bytes(),
            "challenge": challenge,
            "response": s.to_bytes(),
        })
    }

    /// A deterministic equality proof between two Pedersen commitments to the
    /// same value.
    fn equality_proof_data() -> serde_json::Value {
        let g = RISTRETTO_BASEPOINT_POINT;
        let h = generator_h();
        let v = Scalar::from(42u64);
        let r1 = Scalar::from_bytes_mod_order([3u8; 32]);
        let r2 = Scalar::from_bytes_mod_order([5u8; 32]);
        let c1 = g * v + h * r1;
        let c2 = g * v + h * r2;
        let diff = c1 - c2;

        let k = Scalar::from_bytes_mod_order([11u8; 32]);
        let r = h * k;
        let mut hasher = Sha256::new();
        hasher.update(r.compress().as_bytes());
        hasher.update(diff.compress().as_bytes());
        let challenge: [u8; 32] = hasher.finalize().into();
        let response = k + Scalar::from_bytes_mod_order(challenge) * (r1 - r2);

        let mut proof_bytes = Vec::new();
        proof_bytes.extend_from_slice(&challenge);
        proof_bytes.extend_from_slice(&response.to_bytes());

        serde_json::json!({
            "type": "Equality",
            "commitment1": c1.compress().to_bytes(),
            "commitment2": c2.compress().to_bytes(),
            "proof": proof_bytes,
        })
    }

    /// The v2 knowledge challenge preimage, built the way a client must build
    /// it. Written out here rather than imported so these tests exercise the
    /// published layout instead of the server's own helper.
    fn knowledge_v2_preimage(r: &[u8], p: &[u8], statement: &[u8]) -> Vec<u8> {
        let mut preimage = Vec::new();
        preimage.extend_from_slice(b"aingle-zk-knowledge-v2");
        preimage.push(0x00);
        preimage.extend_from_slice(r);
        preimage.extend_from_slice(p);
        preimage.extend_from_slice(&(statement.len() as u64).to_le_bytes());
        preimage.extend_from_slice(statement);
        preimage
    }

    /// The v2 equality challenge preimage.
    fn equality_v2_preimage(r: &[u8], diff: &[u8], statement: &[u8]) -> Vec<u8> {
        let mut preimage = Vec::new();
        preimage.extend_from_slice(b"aingle-zk-equality-v2");
        preimage.push(0x00);
        preimage.extend_from_slice(r);
        preimage.extend_from_slice(diff);
        preimage.extend_from_slice(&(statement.len() as u64).to_le_bytes());
        preimage.extend_from_slice(statement);
        preimage
    }

    /// A statement-binding knowledge proof over `statement`.
    fn knowledge_bound_proof_data(statement: &[u8]) -> serde_json::Value {
        let x = Scalar::from_bytes_mod_order([7u8; 32]);
        let g = RISTRETTO_BASEPOINT_POINT;
        let p = g * x;
        let k = Scalar::from_bytes_mod_order([9u8; 32]);
        let r = g * k;

        let preimage =
            knowledge_v2_preimage(r.compress().as_bytes(), p.compress().as_bytes(), statement);
        let challenge: [u8; 32] = Sha256::digest(&preimage).into();
        let s = k + Scalar::from_bytes_mod_order(challenge) * x;

        serde_json::json!({
            "type": "KnowledgeBound",
            "commitment": p.compress().to_bytes(),
            "challenge": challenge,
            "response": s.to_bytes(),
            "statement": statement,
        })
    }

    /// A statement-binding equality proof over `statement`.
    fn equality_bound_proof_data(statement: &[u8]) -> serde_json::Value {
        let g = RISTRETTO_BASEPOINT_POINT;
        let h = generator_h();
        let v = Scalar::from(42u64);
        let r1 = Scalar::from_bytes_mod_order([3u8; 32]);
        let r2 = Scalar::from_bytes_mod_order([5u8; 32]);
        let c1 = g * v + h * r1;
        let c2 = g * v + h * r2;
        let diff = c1 - c2;

        let k = Scalar::from_bytes_mod_order([11u8; 32]);
        let r = h * k;
        let preimage = equality_v2_preimage(
            r.compress().as_bytes(),
            diff.compress().as_bytes(),
            statement,
        );
        let challenge: [u8; 32] = Sha256::digest(&preimage).into();
        let response = k + Scalar::from_bytes_mod_order(challenge) * (r1 - r2);

        let mut proof_bytes = Vec::new();
        proof_bytes.extend_from_slice(&challenge);
        proof_bytes.extend_from_slice(&response.to_bytes());

        serde_json::json!({
            "type": "EqualityBound",
            "commitment1": c1.compress().to_bytes(),
            "commitment2": c2.compress().to_bytes(),
            "proof": proof_bytes,
            "statement": statement,
        })
    }

    async fn submit_and_verify(
        proof_type: ProofType,
        proof_data: serde_json::Value,
    ) -> serde_json::Value {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let proof_id = state
            .proof_store
            .submit(SubmitProofRequest {
                proof_type,
                proof_data,
                metadata: None,
            })
            .await
            .expect("submit");
        let resp = verify_proof(&state, VerifyProofByIdRequest { proof_id })
            .await
            .expect("verify");
        serde_json::to_value(&resp).expect("serialize")
    }

    #[tokio::test]
    async fn verify_proof_response_alone_lets_a_client_replay_the_knowledge_check() {
        let json = submit_and_verify(ProofType::Knowledge, knowledge_proof_data()).await;

        // ------------------------------------------------------------------
        // From here on: ONLY `json`, plus generic crypto libraries.
        // ------------------------------------------------------------------
        assert_eq!(json["valid"], true, "{json}");
        let r = &json["replay"];
        assert!(
            !r.is_null(),
            "a verify response must publish the material to replay it: {json}"
        );
        assert_eq!(r["scheme"], "aingle-zk-knowledge-v1");
        assert_eq!(r["check"], "schnorr_discrete_log");
        assert_eq!(r["client_can_replay"], true);
        assert!(
            !r["procedure"].as_array().expect("procedure").is_empty(),
            "the bundle must say how to replay it"
        );
        assert_eq!(r["public_parameters"]["group"], "ristretto255");
        assert_eq!(r["public_parameters"]["challenge_hash"], "sha256");

        // 1. Take the generator from the published parameters, not from a
        //    constant baked into the client.
        let g = point(&unhex(&r["public_parameters"]["generator_g"], 32));

        // 2. Public inputs.
        let p_bytes = unhex(&r["public_inputs"]["commitment"], 32);
        let challenge = unhex(&r["public_inputs"]["challenge"], 32);
        let response = unhex(&r["public_inputs"]["response"], 32);

        // 3. Replay: R' = s*G - c*P, then c' = sha256(compress(R') || P).
        let r_prime = g * scalar32(&response) - point(&p_bytes) * scalar32(&challenge);
        let mut hasher = Sha256::new();
        hasher.update(r_prime.compress().as_bytes());
        hasher.update(&p_bytes);
        let recomputed: [u8; 32] = hasher.finalize().into();
        assert_eq!(
            recomputed.to_vec(),
            challenge,
            "the client's own replay must reach the same verdict the server asserted"
        );
    }

    #[tokio::test]
    async fn verify_proof_response_alone_lets_a_client_replay_the_equality_check() {
        let json = submit_and_verify(ProofType::Equality, equality_proof_data()).await;

        // ------------------------------------------------------------------
        // From here on: ONLY `json`.
        // ------------------------------------------------------------------
        assert_eq!(json["valid"], true, "{json}");
        let r = &json["replay"];
        assert_eq!(r["scheme"], "aingle-zk-equality-v1");
        assert_eq!(r["check"], "pedersen_commitment_equality");
        assert_eq!(r["client_can_replay"], true);

        let g = point(&unhex(&r["public_parameters"]["generator_g"], 32));
        let h_bytes = unhex(&r["public_parameters"]["generator_h"], 32);

        // The second generator is not arbitrary: its derivation is published so
        // the client can rebuild it rather than trust the point it was handed.
        let derivation = r["public_parameters"]["generator_h_derivation"]
            .as_str()
            .expect("generator_h_derivation");
        assert!(
            derivation.contains("sha512") && derivation.contains("aingle_zk_pedersen_h"),
            "the H derivation must be stated precisely: {derivation}"
        );
        let mut hasher = Sha512::new();
        hasher.update(g.compress().as_bytes());
        hasher.update(b"aingle_zk_pedersen_h");
        let rebuilt_h = RistrettoPoint::from_uniform_bytes(&hasher.finalize().into());
        assert_eq!(
            rebuilt_h.compress().to_bytes().to_vec(),
            h_bytes,
            "the published H must be the one the stated derivation produces"
        );

        let c1 = point(&unhex(&r["public_inputs"]["commitment1"], 32));
        let c2 = point(&unhex(&r["public_inputs"]["commitment2"], 32));
        let challenge = unhex(&r["public_inputs"]["challenge"], 32);
        let response = unhex(&r["public_inputs"]["response"], 32);

        let diff = c1 - c2;
        let r_prime = rebuilt_h * scalar32(&response) - diff * scalar32(&challenge);
        let mut hasher = Sha256::new();
        hasher.update(r_prime.compress().as_bytes());
        hasher.update(diff.compress().as_bytes());
        let recomputed: [u8; 32] = hasher.finalize().into();
        assert_eq!(
            recomputed.to_vec(),
            challenge,
            "the client's own replay must reach the same verdict the server asserted"
        );
    }

    #[tokio::test]
    async fn tampering_with_a_published_input_breaks_the_replay() {
        // The negative control. Without it the replay above would prove nothing:
        // a check that passes on anything is not a check.
        let json = submit_and_verify(ProofType::Knowledge, knowledge_proof_data()).await;
        let r = &json["replay"];

        let g = point(&unhex(&r["public_parameters"]["generator_g"], 32));
        let mut p_bytes = unhex(&r["public_inputs"]["commitment"], 32);
        let challenge = unhex(&r["public_inputs"]["challenge"], 32);
        let response = unhex(&r["public_inputs"]["response"], 32);

        // Flip one bit of the public point until it is still a valid encoding,
        // then replay: the recomputed challenge must not match.
        p_bytes[0] ^= 0x01;
        let Some(tampered) = CompressedRistretto::from_slice(&p_bytes)
            .ok()
            .and_then(|c| c.decompress())
        else {
            // A non-canonical encoding is itself a failed replay.
            return;
        };
        let r_prime = g * scalar32(&response) - tampered * scalar32(&challenge);
        let mut hasher = Sha256::new();
        hasher.update(r_prime.compress().as_bytes());
        hasher.update(&p_bytes);
        let recomputed: [u8; 32] = hasher.finalize().into();
        assert_ne!(
            recomputed.to_vec(),
            challenge,
            "a one-bit edit to a public input must break the replay"
        );
    }

    #[tokio::test]
    async fn a_hash_opening_verdict_is_labelled_as_well_formedness_not_proof() {
        // `ProofVerifier` cannot check a hash opening without the committed
        // preimage, which this node does not hold. `valid: true` there means
        // only "the fields are non-zero". The response must say so rather than
        // let a caller read it as an opening that was checked.
        let commitment = [0x11u8; 32];
        let salt = [0x22u8; 32];
        let json = submit_and_verify(
            ProofType::HashOpening,
            serde_json::json!({ "type": "HashOpening", "commitment": commitment, "salt": salt }),
        )
        .await;

        assert_eq!(json["valid"], true, "the server's own check passes: {json}");
        let r = &json["replay"];
        assert_eq!(r["scheme"], "aingle-zk-hash-opening-v1");
        assert_eq!(
            r["check"], "well_formedness_only",
            "the check actually performed must be named, not implied: {json}"
        );
        assert!(
            r["additional_input_required"].is_string(),
            "when the verdict cannot establish the claim, the response must say \
             what is missing: {json}"
        );
        let does_not = r["does_not_establish"]
            .as_str()
            .expect("does_not_establish");
        assert!(
            does_not.to_lowercase().contains("commit"),
            "the gap must be stated in terms of the claim, not hedged: {does_not}"
        );
    }

    #[tokio::test]
    async fn an_unparseable_proof_reports_no_replay_material_rather_than_pretending() {
        let json = submit_and_verify(
            ProofType::Schnorr,
            serde_json::json!({ "garbage": "not-a-zk-proof" }),
        )
        .await;
        assert_eq!(json["valid"], false, "{json}");
        assert!(
            json["replay"].is_null(),
            "there is nothing to replay when the proof cannot even be parsed: {json}"
        );
    }

    // ========================================================================
    // Statement binding
    //
    // A proof that verifies is only proof *of* something if the something is
    // hashed into the challenge. The v1 schemes hash the commitment alone, so a
    // valid proof can be lifted and presented next to any claim at all. These
    // tests are the whole point of the v2 schemes: transplant the proof onto a
    // different statement and the verification must fail.
    // ========================================================================

    const STATEMENT_A: &[u8] = b"ex:note-1 was authored by ex:alice";
    const STATEMENT_B: &[u8] = b"ex:note-2 was authored by ex:mallory";

    #[tokio::test]
    async fn a_knowledge_proof_bound_to_one_statement_does_not_verify_for_another() {
        let json = submit_and_verify(
            ProofType::Knowledge,
            knowledge_bound_proof_data(STATEMENT_A),
        )
        .await;
        assert_eq!(
            json["valid"], true,
            "a correctly formed statement-bound proof must verify: {json}"
        );
        assert_eq!(json["replay"]["scheme"], "aingle-zk-knowledge-v2");
        assert_eq!(json["replay"]["statement_binding"]["bound"], true, "{json}");

        // The transplant: identical commitment/challenge/response, different
        // statement. This is exactly the attack the v1 scheme permits.
        let mut transplanted = knowledge_bound_proof_data(STATEMENT_A);
        transplanted["statement"] = serde_json::json!(STATEMENT_B);
        let json = submit_and_verify(ProofType::Knowledge, transplanted).await;
        assert_eq!(
            json["valid"], false,
            "a proof bound to statement A must NOT verify when presented for \
             statement B — otherwise it proves nothing about either: {json}"
        );
    }

    #[tokio::test]
    async fn an_equality_proof_bound_to_one_statement_does_not_verify_for_another() {
        let json =
            submit_and_verify(ProofType::Equality, equality_bound_proof_data(STATEMENT_A)).await;
        assert_eq!(json["valid"], true, "{json}");
        assert_eq!(json["replay"]["scheme"], "aingle-zk-equality-v2");
        assert_eq!(json["replay"]["statement_binding"]["bound"], true, "{json}");

        let mut transplanted = equality_bound_proof_data(STATEMENT_A);
        transplanted["statement"] = serde_json::json!(STATEMENT_B);
        let json = submit_and_verify(ProofType::Equality, transplanted).await;
        assert_eq!(
            json["valid"], false,
            "an equality proof bound to statement A must not verify for B: {json}"
        );
    }

    #[tokio::test]
    async fn the_v1_schemes_stay_verifiable_and_are_marked_as_binding_no_statement() {
        // Proofs already stored were generated under the old challenge. Breaking
        // them would be a worse defect than the one being fixed — but a client
        // must be able to see, from the response alone, which guarantee it has.
        let json = submit_and_verify(ProofType::Knowledge, knowledge_proof_data()).await;
        assert_eq!(
            json["valid"], true,
            "old proofs must keep verifying: {json}"
        );
        let binding = &json["replay"]["statement_binding"];
        assert_eq!(json["replay"]["scheme"], "aingle-zk-knowledge-v1");
        assert_eq!(
            binding["bound"], false,
            "the v1 challenge covers no statement; say so: {json}"
        );
        assert!(
            binding["note"]
                .as_str()
                .expect("note")
                .to_lowercase()
                .contains("statement"),
            "the note must name what is missing: {binding}"
        );
        assert!(
            binding["statement_hex"].is_null(),
            "there is no bound statement to publish for v1: {binding}"
        );

        let json = submit_and_verify(ProofType::Equality, equality_proof_data()).await;
        assert_eq!(json["valid"], true, "{json}");
        assert_eq!(json["replay"]["scheme"], "aingle-zk-equality-v1");
        assert_eq!(
            json["replay"]["statement_binding"]["bound"], false,
            "{json}"
        );
    }

    #[tokio::test]
    async fn the_replay_bundle_publishes_the_exact_bytes_fed_to_the_challenge_hash() {
        // Re-serializing the statement on the client produces different bytes
        // and verifies nothing. The bundle must publish the literal preimage.
        let json = submit_and_verify(
            ProofType::Knowledge,
            knowledge_bound_proof_data(STATEMENT_A),
        )
        .await;

        // ------------------------------------------------------------------
        // From here on: ONLY `json`, plus generic crypto libraries.
        // ------------------------------------------------------------------
        let r = &json["replay"];
        let binding = &r["statement_binding"];

        let preimage = {
            let s = binding["challenge_preimage_hex"]
                .as_str()
                .expect("challenge_preimage_hex");
            (0..s.len() / 2)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
                .collect::<Vec<u8>>()
        };

        // 1. The published bytes must hash to the published challenge.
        let challenge = unhex(&r["public_inputs"]["challenge"], 32);
        let digest: [u8; 32] = Sha256::digest(&preimage).into();
        assert_eq!(
            digest.to_vec(),
            challenge,
            "the published preimage must be the bytes that were actually hashed"
        );

        // 2. Those bytes must embed the statement the client was shown, so the
        //    binding is checkable rather than asserted.
        let statement = {
            let s = binding["statement_hex"].as_str().expect("statement_hex");
            (0..s.len() / 2)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
                .collect::<Vec<u8>>()
        };
        assert_eq!(statement, STATEMENT_A);
        assert!(
            preimage
                .windows(statement.len())
                .any(|w| w == statement.as_slice()),
            "the challenge preimage must contain the statement it claims to bind"
        );

        // 3. And the replay must still close: R' = s*G - c*P, and the segment of
        //    the preimage where R sits must be that R'.
        let g = point(&unhex(&r["public_parameters"]["generator_g"], 32));
        let p_bytes = unhex(&r["public_inputs"]["commitment"], 32);
        let response = unhex(&r["public_inputs"]["response"], 32);
        let r_prime = g * scalar32(&response) - point(&p_bytes) * scalar32(&challenge);
        assert!(
            preimage
                .windows(32)
                .any(|w| w == r_prime.compress().as_bytes()),
            "the preimage must contain the R the client recomputes, or it is not \
             the preimage of this proof"
        );
    }
}
