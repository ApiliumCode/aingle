// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Triple validation business logic shared by REST and MCP.

use crate::error::{Error, Result};
use crate::middleware::is_in_namespace;
use crate::rest::pol_evidence::{
    pol_procedure, RuleSetFingerprint, TripleIdentity, HASH_ALG, VALIDATION_PROOF_SPEC,
};
use crate::rest::{
    TripleDto, TripleValidationResult, ValidateRequest, ValidateResponse, ValidationMessage,
    ValidationProofDto,
};
use crate::state::{AppState, Event};
use aingle_graph::{NodeId, Predicate, Triple, Value};

/// How `proof_hash` is reproduced, spelled out for a client holding only the
/// response.
fn validation_proof_procedure() -> Vec<String> {
    [
        "1. For each entry of `proof.triples`, recompute the triple id as \
         blake3-256(subject_bytes || predicate_bytes || object_bytes) — hex-decode each \
         field first, then concatenate the raw bytes with no separators and no length \
         prefixes. It MUST equal that entry's `triple_id`; otherwise the published \
         parts do not describe the triples being reported.",
        "2. Concatenate the entries of `proof.preimage_parts` — the ASCII lowercase-hex \
         triple ids, in order, with no separator — and take blake3-256 of those ASCII \
         bytes. The hex digest MUST equal `proof_hash`.",
        "3. Read what this establishes: `proof_hash` commits to WHICH triples were \
         submitted, in what order. It does not commit to the verdict, to the rule set, \
         or to a timestamp, and it is not signed — so it identifies a validation \
         request, it does not attest to its outcome. Nothing stops the same digest \
         being produced for the same triples under different rules.",
        "4. To say anything about the outcome, follow the `procedure` on the response \
         itself and read `rule_set`.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Validate triple(s) against the logic engine.
///
/// Semantics preserved from the REST `POST /api/v1/validate` handler: each input
/// triple is run through the PoL logic engine and reported with per-triple
/// validity + messages. A `proof_hash` is generated only when every triple is
/// valid, and a `ValidationCompleted` event is broadcast in that case (matching
/// the handler's side-effect). Validation answering "this triple is invalid" is
/// a successful response (`valid:false`), NOT an error.
///
/// `namespace` enforces that input subjects fall within the request namespace;
/// REST passes the request namespace, MCP passes `None` (no namespace
/// enforcement). An out-of-namespace subject yields `Err(Error::Forbidden(..))`,
/// exactly as the REST handler does.
pub async fn validate_triples(
    state: &AppState,
    req: ValidateRequest,
    namespace: Option<String>,
) -> Result<ValidateResponse> {
    let logic = state.logic.read().await;

    let ns_filter = namespace;

    let rule_set = RuleSetFingerprint::of(&logic);

    let mut results = Vec::new();
    let mut identities = Vec::new();
    // Tracks the engine's own answer. It is turned into a published verdict by
    // `rule_set.verdict`, which refuses to call an unexamined triple valid.
    let mut all_accepted = true;

    for input in req.triples {
        // Enforce namespace on input subjects.
        if let Some(ref ns) = ns_filter {
            if !is_in_namespace(&input.subject, ns) {
                return Err(Error::Forbidden(format!(
                    "Subject \"{}\" is not in namespace \"{}\"",
                    input.subject, ns
                )));
            }
        }
        let object: Value = input.object.clone().into();

        // Create a triple for validation.
        let triple = Triple::new(
            NodeId::named(&input.subject),
            Predicate::named(&input.predicate),
            object,
        );

        // Validate using logic engine.
        let validation = logic.validate(&triple);

        let accepted = validation.is_valid();
        if !accepted {
            all_accepted = false;
        }
        let (valid, outcome) = rule_set.verdict(accepted);

        // Convert messages.
        let mut messages = Vec::new();
        for rejection in &validation.rejections {
            messages.push(ValidationMessage {
                level: "error".to_string(),
                message: rejection.reason.clone(),
                rule: Some(rejection.rule_id.clone()),
            });
        }
        for warning in &validation.warnings {
            messages.push(ValidationMessage {
                level: "warning".to_string(),
                message: warning.message.clone(),
                rule: Some(warning.rule_id.clone()),
            });
        }

        let triple_dto = TripleDto {
            id: Some(triple.id().to_hex()),
            subject: input.subject.clone(),
            predicate: input.predicate.clone(),
            object: input.object,
            created_at: None,
        };

        // The literal hash inputs of this triple, so the id above is checkable
        // rather than another opaque server-computed string.
        identities.push(TripleIdentity::of(&triple));

        results.push(TripleValidationResult {
            triple: triple_dto,
            valid,
            outcome: outcome.to_string(),
            messages,
        });
    }

    drop(logic);

    let (valid, outcome) = rule_set.verdict(all_accepted);

    // Generate a simple proof hash if nothing was rejected. The digest commits
    // to WHICH triples were submitted, not to the verdict, so it is produced for
    // an unevaluated batch too — but `outcome` above still says nothing was
    // checked, and `proof.does_not_cover` says the digest is not evidence of
    // validity in either case.
    let (proof_hash, proof) = if all_accepted {
        // The preimage is the concatenation of the ASCII triple-id hex strings,
        // in order. Collected here rather than described in prose so the client
        // hashes exactly what this loop hashes.
        let preimage_parts: Vec<String> =
            results.iter().filter_map(|r| r.triple.id.clone()).collect();

        let mut hasher = blake3::Hasher::new();
        for part in &preimage_parts {
            hasher.update(part.as_bytes());
        }
        let hash = hasher.finalize().to_hex().to_string();

        let dto = ValidationProofDto {
            spec: VALIDATION_PROOF_SPEC.to_string(),
            hash_alg: HASH_ALG.to_string(),
            covers: "the identities of the submitted triples, in submission order.".to_string(),
            does_not_cover:
                "the verdict. `valid` is not hashed, the rule set is not hashed, nothing \
                 is signed and no timestamp is bound in — so this digest identifies WHICH \
                 triples were validated, never that they are valid. A digest sitting next \
                 to `valid: true` is not evidence for it."
                    .to_string(),
            preimage_parts,
            triples: identities,
            procedure: validation_proof_procedure(),
        };
        (Some(hash), Some(dto))
    } else {
        (None, None)
    };

    // Broadcast validation event (same side-effect as the REST handler).
    if let Some(ref hash) = proof_hash {
        state.broadcaster.broadcast(Event::ValidationCompleted {
            hash: hash.clone(),
            valid,
            outcome: outcome.to_string(),
            proof_hash: proof_hash.clone(),
        });
    }

    Ok(ValidateResponse {
        valid,
        outcome: outcome.to_string(),
        results,
        proof_hash,
        proof,
        rule_set,
        procedure: pol_procedure(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::{ValidateTripleInput, ValueDto};

    #[tokio::test]
    async fn validate_minimal_triple_returns_per_triple_result() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = ValidateRequest {
            triples: vec![ValidateTripleInput {
                subject: "ex:alice".to_string(),
                predicate: "ex:knows".to_string(),
                object: ValueDto::Node {
                    node: "ex:bob".to_string(),
                },
            }],
            rule_set: None,
        };

        let resp = validate_triples(&state, req, None)
            .await
            .expect("validation must return Ok for a well-formed triple");

        // One input => one per-triple result.
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].triple.subject, "ex:alice");
        assert_eq!(resp.results[0].triple.predicate, "ex:knows");
        // A plain triple passes the loaded rules and a proof hash is produced.
        assert_eq!(resp.valid, Some(true));
        assert!(resp.proof_hash.is_some());
    }

    #[tokio::test]
    async fn validate_empty_request_is_valid_with_proof_hash() {
        let state = AppState::with_db_path(":memory:", None).unwrap();

        let req = ValidateRequest {
            triples: vec![],
            rule_set: None,
        };

        let resp = validate_triples(&state, req, None)
            .await
            .expect("empty validation must return Ok");
        // No triple failed, so the verdict is valid and a (degenerate) proof
        // hash is generated.
        assert_eq!(resp.valid, Some(true));
        assert!(resp.results.is_empty());
        assert!(resp.proof_hash.is_some());
    }

    // ========================================================================
    // Independent checkability
    //
    // `proof_hash` used to be an anchor with nothing behind it: a digest a
    // client could neither reproduce nor interpret. Below the "from here on"
    // marker these tests hold only the serialized response and a generic hash
    // library — no aingle type, no server state.
    // ========================================================================

    fn unhex(v: &serde_json::Value) -> Vec<u8> {
        let s = v
            .as_str()
            .unwrap_or_else(|| panic!("expected hex, got {v}"));
        assert!(s.len().is_multiple_of(2), "hex must be byte-aligned: {s:?}");
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex digit"))
            .collect()
    }

    async fn validated(subjects: &[&str]) -> serde_json::Value {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        validated_with(&state, subjects).await
    }

    async fn validated_with(state: &AppState, subjects: &[&str]) -> serde_json::Value {
        let req = ValidateRequest {
            triples: subjects
                .iter()
                .map(|s| ValidateTripleInput {
                    subject: (*s).to_string(),
                    predicate: "ex:knows".to_string(),
                    object: ValueDto::Node {
                        node: "ex:bob".to_string(),
                    },
                })
                .collect(),
            rule_set: None,
        };
        let resp = validate_triples(state, req, None).await.expect("validate");
        serde_json::to_value(&resp).expect("serialize")
    }

    /// A state whose operator has switched every rule off.
    async fn state_without_rules() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        *state.logic.write().await = aingle_logic::RuleEngine::new();
        state
    }

    #[tokio::test]
    async fn validate_response_alone_lets_a_client_recompute_the_proof_hash() {
        let json = validated(&["ex:alice", "ex:carol"]).await;

        // ------------------------------------------------------------------
        // From here on: ONLY `json` and a blake3 implementation.
        // ------------------------------------------------------------------
        let proof = &json["proof"];
        assert!(
            !proof.is_null(),
            "a proof_hash a client cannot reproduce is an anchor, not a proof: {json}"
        );
        assert_eq!(proof["spec"], "aingle-validation-proof-v1");
        assert_eq!(proof["hash_alg"], "blake3-256");
        assert!(!proof["procedure"].as_array().expect("procedure").is_empty());

        // 1. Each triple identity must be recomputable from the literal bytes
        //    that were hashed — otherwise the ids are just more server output.
        let triples = proof["triples"].as_array().expect("proof.triples");
        assert_eq!(triples.len(), 2);
        let mut concatenated = String::new();
        for t in triples {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&unhex(&t["subject_bytes"]));
            hasher.update(&unhex(&t["predicate_bytes"]));
            hasher.update(&unhex(&t["object_bytes"]));
            assert_eq!(
                hasher.finalize().to_hex().to_string(),
                t["triple_id"].as_str().expect("triple_id"),
                "the published bytes must hash to the published triple id"
            );
            concatenated.push_str(t["triple_id"].as_str().unwrap());
        }

        // 2. The published preimage must be exactly those ids, in order.
        let parts: Vec<&str> = proof["preimage_parts"]
            .as_array()
            .expect("preimage_parts")
            .iter()
            .map(|p| p.as_str().expect("part"))
            .collect();
        assert_eq!(parts.concat(), concatenated);

        // 3. blake3 over that concatenation must be the advertised proof_hash.
        assert_eq!(
            blake3::hash(concatenated.as_bytes()).to_hex().to_string(),
            json["proof_hash"].as_str().expect("proof_hash"),
            "the client's own digest must equal the one the server published"
        );

        // 4. The identity has to bind the content it claims to. The hashed
        //    subject bytes must actually contain the subject being displayed.
        let subject_bytes = unhex(&triples[0]["subject_bytes"]);
        assert!(
            String::from_utf8_lossy(&subject_bytes).contains("ex:alice"),
            "the hashed bytes must embed the subject they are said to identify"
        );
    }

    #[tokio::test]
    async fn a_changed_triple_id_breaks_the_recomputed_proof_hash() {
        // Negative control: if the recomputation did not depend on the published
        // parts, the test above would prove nothing.
        let json = validated(&["ex:alice"]).await;
        let mut parts: Vec<String> = json["proof"]["preimage_parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();
        let flipped = if parts[0].starts_with('a') { "b" } else { "a" };
        parts[0].replace_range(0..1, flipped);
        assert_ne!(
            blake3::hash(parts.concat().as_bytes()).to_hex().to_string(),
            json["proof_hash"].as_str().unwrap(),
            "a one-character edit to the preimage must change the hash"
        );
    }

    #[tokio::test]
    async fn validate_states_that_its_verdict_rests_on_an_empty_rule_set() {
        // The verdict this node returns is "no loaded rule rejected the triple".
        // A node with no rules loaded rejects nothing, so a `true` there would be
        // vacuous — and saying so is the difference between an honest response
        // and a misleading one.
        let state = state_without_rules().await;
        let json = validated_with(&state, &["ex:alice"]).await;

        let rs = &json["rule_set"];
        assert_eq!(rs["rule_count"], 0, "{json}");
        assert_eq!(
            rs["vacuous"], true,
            "an empty rule set must be reported as vacuous, not as a clean pass: {json}"
        );
        let note = rs["note"].as_str().expect("note").to_lowercase();
        assert!(
            note.contains("no rules"),
            "the note must say plainly that nothing was checked: {note}"
        );

        // And the proof must not be mistaken for a proof of validity.
        let does_not = json["proof"]["does_not_cover"]
            .as_str()
            .expect("does_not_cover")
            .to_lowercase();
        assert!(
            does_not.contains("verdict") || does_not.contains("valid"),
            "the digest covers the inputs, not the verdict; say so: {does_not}"
        );
    }

    // ========================================================================
    // The correctness floor: nothing evaluated => nothing passed
    //
    // Saying "the rule set was empty" in a side field is not enough while the
    // headline answer still reads `valid: true`. A client that checks the
    // boolean — which is every client — is told the triple passed a check that
    // never ran. The verdict itself has to carry the distinction.
    // ========================================================================

    #[tokio::test]
    async fn an_unevaluated_validation_never_reports_a_passing_verdict() {
        let state = state_without_rules().await;
        let json = validated_with(&state, &["ex:alice"]).await;

        assert_eq!(json["rule_set"]["vacuous"], true, "precondition: {json}");
        assert_ne!(
            json["valid"],
            serde_json::json!(true),
            "with no rules enabled nothing was examined; `valid: true` claims a \
             check that never ran: {json}"
        );
        assert_eq!(
            json["outcome"], "not_evaluated",
            "the verdict must name the third state, not collapse into the \
             pass/fail boolean: {json}"
        );
        let result = &json["results"][0];
        assert_ne!(result["valid"], serde_json::json!(true), "{json}");
        assert_eq!(result["outcome"], "not_evaluated", "{json}");
    }

    #[tokio::test]
    async fn the_default_rule_set_is_loaded_and_actually_rejects_something() {
        // (b): a node out of the box must evaluate against real rules, and those
        // rules must be capable of saying no. A rule set that accepts everything
        // is an empty one wearing a name.
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let json = validated_with(&state, &["ex:alice"]).await;
        assert_eq!(
            json["rule_set"]["vacuous"], false,
            "the shipped node must load a real rule set: {json}"
        );
        assert_eq!(json["valid"], true, "a well-formed triple passes: {json}");
        assert_eq!(json["outcome"], "valid", "{json}");

        // A triple that says a thing relates to itself is rejected.
        let req = ValidateRequest {
            triples: vec![ValidateTripleInput {
                subject: "ex:alice".to_string(),
                predicate: "ex:knows".to_string(),
                object: ValueDto::Node {
                    node: "ex:alice".to_string(),
                },
            }],
            rule_set: None,
        };
        let json =
            serde_json::to_value(validate_triples(&state, req, None).await.expect("validate"))
                .unwrap();
        assert_eq!(
            json["valid"], false,
            "the default rule set must be able to reject: {json}"
        );
        assert_eq!(json["outcome"], "invalid", "{json}");
        assert!(
            !json["results"][0]["messages"]
                .as_array()
                .expect("messages")
                .is_empty(),
            "a rejection must name the rule and the reason: {json}"
        );
    }

    #[tokio::test]
    async fn the_rule_set_fingerprint_enumerates_the_rules_an_operator_can_inspect() {
        let json = validated(&["ex:alice"]).await;
        let rs = &json["rule_set"];
        let rules = rs["rules"].as_array().expect("rules");
        assert_eq!(
            rules.len(),
            rs["enabled_rule_count"].as_u64().expect("count") as usize,
            "every evaluated rule must be listed: {json}"
        );
        let first = &rules[0];
        for field in ["id", "name", "description", "kind", "effect"] {
            assert!(
                first[field].as_str().is_some_and(|s| !s.is_empty()),
                "rule field `{field}` must be published for an operator to read: {first}"
            );
        }
    }
}
