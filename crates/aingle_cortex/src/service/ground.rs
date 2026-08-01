// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Grounded retrieval: turn a question into cited, provenance-backed context with
//! an explicit groundedness signal, so an LLM answers only from verifiable sources.

use crate::error::Result;
use crate::state::AppState;
use serde::Serialize;

/// Number of strong chunks required to call retrieval "grounded". Requiring two
/// independent corroborating sources is a deliberate anti-hallucination policy:
/// a lone strong chunk is surfaced as "weak", not "grounded". The strong/weak
/// similarity cutoffs themselves come from the active embedder via
/// [`ineru::Embedder::relevance_thresholds`].
const MIN_CORROBORATING_CHUNKS: usize = 2;

/// Fraction of a question's content words that must actually appear in the
/// retrieved text before the verdict may be "grounded".
///
/// # Why similarity alone is not enough
///
/// Cosine similarity answers "is this text like the question", which is not the
/// same as "does this text answer the question". With the sentence embedders in
/// use here the scores are compressed — unrelated prose from the same corpus
/// lands around 0.83, a direct hit around 0.86 — so an absolute cutoff sits
/// inside the noise and admits almost anything.
///
/// Measured on a 24-question labelled set: of the seven questions where
/// retrieval returned nothing useful at all, the verdict was "grounded" **seven
/// times out of seven**. Raising the cutoff does not fix it — it removes true
/// verdicts at nearly the same rate as false ones. Requiring lexical
/// corroboration as a SECOND signal removes six of those seven while keeping
/// most of the true ones, and the rest degrade to "weak", which still shows the
/// passages and says the evidence is thin rather than asserting a confidence
/// nobody earned.
const MIN_QUESTION_TERM_COVERAGE: f32 = 0.6;

/// Content words of a question: lowercased, three characters or more, deduped,
/// minus the function words that carry no topic.
///
/// Deliberately multilingual and deliberately crude. It splits on Unicode
/// alphanumerics rather than ASCII, so accented and non-Latin queries keep their
/// words instead of being shredded; the stop list covers the languages the
/// interface ships in. This is a corroboration signal, not a parser: being
/// approximately right in many languages matters more than being exact in one.
fn question_terms(question: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        // English
        "the", "and", "for", "are", "was", "were", "what", "which", "who", "whom", "how", "why",
        "when", "where", "does", "did", "can", "could", "with", "from", "this", "that", "these",
        "those", "you", "your", "our", "their", "his", "her", "its", "have", "has", "had", "not",
        "but", "all", "any", "about", "into", "than", "then", "them", "they", "there", "here",
        // Spanish
        "que", "qué", "los", "las", "del", "una", "unos", "unas", "por", "con", "para", "como",
        "cómo", "cuando", "cuándo", "donde", "dónde", "quien", "quién", "cual", "cuál", "cuales",
        "esta", "está", "este", "estos", "estas", "están", "eso", "esa", "ese", "son", "era",
        "eran", "hay", "sus", "sobre", "desde", "entre", "hasta", "muy", "mas", "más", "porque",
        // French / Portuguese / Italian / German
        "les", "des", "une", "dans", "pour", "avec", "est", "sont", "qui", "quoi", "comment", "não",
        "uma", "dos", "das", "der", "die", "und", "ist", "sind", "mit", "für", "wie", "wer",
        "nicht", "che", "per", "non", "sono",
    ];
    let mut out: Vec<String> = Vec::new();
    for raw in question.split(|c: char| !c.is_alphanumeric()) {
        if raw.chars().count() < 3 {
            continue;
        }
        let w = raw.to_lowercase();
        if STOP.contains(&w.as_str()) || out.contains(&w) {
            continue;
        }
        out.push(w);
    }
    out
}

/// Fraction of `terms` that appear anywhere in `body`.
///
/// Substring rather than whole-word matching, on purpose: it lets a query term
/// corroborate against an inflected form ("cita" in "citas", "sign" in "signed")
/// without carrying a stemmer for every language.
fn term_coverage(terms: &[String], body: &str) -> f32 {
    if terms.is_empty() {
        // A question made only of function words gives this signal nothing to
        // work with. Abstaining leaves the decision to similarity alone — the
        // behaviour that existed before — rather than refusing the question.
        return 1.0;
    }
    let body = body.to_lowercase();
    let hits = terms.iter().filter(|t| body.contains(t.as_str())).count();
    hits as f32 / terms.len() as f32
}

/// A cited chunk of source context.
#[derive(Debug, Clone, Serialize)]
pub struct ContextChunk {
    pub text: String,
    pub source: String,
    pub lines: String,
    pub relevance: f32,
    /// Hex hash of the DAG action that recorded this source.
    ///
    /// This is a **pointer, not a proof**: its presence only means this server
    /// found a signed action for the source. To obtain evidence, fetch the action
    /// (`GET /api/v1/dag/action/{hash}`, MCP `aingle_dag_action`) and run the
    /// verification procedure it returns — that response carries the signature,
    /// the public key and the exact signed bytes. `None` when the source has no
    /// signed action.
    pub provenance_anchor: Option<String>,
    pub ingested_at: Option<String>,
}

/// The grounded answer context returned to the model.
#[derive(Debug, Clone, Serialize)]
pub struct GroundedContext {
    pub groundedness: String, // "grounded" | "weak" | "ungrounded"
    pub answer_context: Vec<ContextChunk>,
    pub gaps: Vec<String>,
    /// Instruction echoed to the model to keep it on the cited path.
    pub instruction: String,
    /// `true` when the index holds chunks but every stored embedding in the
    /// candidate pool is a placeholder (missing or all-zero), so no query can
    /// ground against it. This is the honest signal for a stale index that must
    /// be re-embedded — distinct from "ungrounded" (index is fine, topic absent).
    /// Never `true` for a healthy or genuinely empty index.
    #[serde(default)]
    pub index_stale: bool,
}

use ineru::MemoryQuery;

/// Retrieve grounded context for `question`. Pulls the top-`k` semantically
/// similar chunks from Ineru, attaches each chunk's signed provenance from the
/// DAG (latest signed action affecting its source path), and computes a
/// groundedness signal from the best similarity.
pub async fn ground(state: &AppState, question: &str, k: usize) -> Result<GroundedContext> {
    let k = k.max(1);
    let (ground_high, ground_low) = state.embedder.relevance_thresholds();

    let query_vec = state.embedder.embed_query(question);
    // Fetch a broad candidate pool: Ineru's composite recall score is keyword-
    // and importance-weighted (embedding is only a minor term), so we over-fetch
    // and re-rank by pure embedding cosine below. That makes grounding a true
    // semantic search whose scores match the embedder's `relevance_thresholds`.
    let fetch_limit = k.max(24);
    let results = {
        let mem = state.memory.read().await;
        mem.recall(
            &MemoryQuery::text(question)
                .with_limit(fetch_limit)
                .with_embedding(query_vec.clone()),
        )
        .map_err(|e| crate::error::Error::Internal(e.to_string()))?
    };

    let mut answer_context = Vec::new();
    // Track the health of the candidate pool so a placeholder/stale index is
    // reported honestly instead of masquerading as a plain "ungrounded" miss.
    let mut chunk_total = 0usize;
    let mut chunk_degenerate = 0usize;
    for r in &results {
        // Only consider chunk memories produced by ingestion.
        if r.entry.entry_type != crate::service::ingest::CHUNK_ENTRY_TYPE {
            continue;
        }
        chunk_total += 1;
        // Semantic relevance = cosine(query, chunk) from the active embedder,
        // not Ineru's composite recall score. A stored embedding that is missing
        // or all-zero is a placeholder (pending model persisted, or a poisoned
        // legacy index): it scores 0 against every query, so it can never ground
        // an answer. Skip it AND count it — a pool that is entirely degenerate is
        // the fingerprint of a stale index that needs re-embedding.
        let relevance = match &r.entry.embedding {
            Some(emb) if emb.0.iter().any(|x| *x != 0.0) => query_vec.cosine_similarity(emb),
            _ => {
                chunk_degenerate += 1;
                continue;
            }
        };
        let d = &r.entry.data;
        let source = d
            .get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ls = d.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0);
        let le = d.get("line_end").and_then(|v| v.as_u64()).unwrap_or(0);
        let text = d
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let (sig, ingested_at) = signed_provenance(state, &source).await;

        answer_context.push(ContextChunk {
            text,
            source,
            lines: format!("{ls}-{le}"),
            relevance,
            provenance_anchor: sig,
            ingested_at,
        });
    }

    // Re-rank by semantic relevance and keep the top-k.
    answer_context.sort_by(|a, b| {
        b.relevance
            .partial_cmp(&a.relevance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    answer_context.truncate(k);
    let best: f32 = answer_context.first().map(|c| c.relevance).unwrap_or(0.0);

    // Require at least MIN_CORROBORATING_CHUNKS strong matches for "grounded";
    // a single strong chunk is only "weak" (independent corroboration guard).
    let strong = answer_context
        .iter()
        .filter(|c| c.relevance >= ground_high)
        .count();
    // Second signal: do the question's own words appear in what came back?
    // Similarity says "this resembles the question"; this says "this is about
    // what was asked". Only the strong chunks are examined — a weak chunk is not
    // evidence of anything, and letting it corroborate would hand the check back
    // the noise it exists to filter.
    let strong_body: String = answer_context
        .iter()
        .filter(|c| c.relevance >= ground_high)
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let coverage = term_coverage(&question_terms(question), &strong_body);

    let groundedness = if best >= ground_high
        && strong >= MIN_CORROBORATING_CHUNKS
        && coverage >= MIN_QUESTION_TERM_COVERAGE
    {
        "grounded"
    } else if best >= ground_low && !answer_context.is_empty() {
        // Everything that fails the corroboration check but retrieved something
        // lands here rather than in "ungrounded": the passages are still shown,
        // and the caller is told the evidence is thin instead of being told
        // there is none.
        "weak"
    } else {
        "ungrounded"
    };

    // A candidate pool that held chunks but whose every stored embedding was a
    // placeholder means the index is stale, NOT that the topic is absent. This is
    // the guard against the silent-retrieval failure: chunks exist, the engine
    // reports Ready, yet nothing can ever ground because the vectors are zeros.
    let index_stale = chunk_total > 0 && chunk_degenerate == chunk_total;

    let mut gaps = Vec::new();
    if index_stale {
        gaps.push(
            "The semantic index is stale: stored embeddings are placeholders, so no query \
             can be grounded. Re-index the vault to rebuild the embeddings."
                .to_string(),
        );
    } else if answer_context.is_empty() {
        gaps.push(format!("No ingested source matches: {question:?}."));
    } else if groundedness == "weak" {
        if best >= ground_high && strong < MIN_CORROBORATING_CHUNKS {
            gaps.push(
                "Only one source corroborates this; a second is needed to be grounded.".to_string(),
            );
        } else {
            gaps.push("Retrieved context is only weakly related to the question.".to_string());
        }
    } else if groundedness == "ungrounded" {
        // Chunks were retrieved but none are relevant enough to ground an answer.
        // Surface the gap so the engine stays honest rather than silently empty.
        gaps.push(
            "Retrieved context is not relevant enough to ground an answer on this topic."
                .to_string(),
        );
    }

    Ok(GroundedContext {
        groundedness: groundedness.to_string(),
        answer_context,
        gaps,
        instruction: "Answer ONLY from answer_context and cite each claim as \
            source:lines. If groundedness is not \"grounded\", say so explicitly \
            and do not invent facts."
            .to_string(),
        index_stale,
    })
}

/// Look up the latest signed DAG action affecting `source_path` and return its
/// action hash (the provenance anchor) and timestamp, if any.
///
/// The anchor is the action's hash rather than the signature itself: it is a
/// handle a client resolves against the single-action lookup, which serves the
/// signature, the public key and the canonical signed bytes. Inlining the proof
/// into every retrieved chunk would repeat a whole signed payload per citation;
/// inlining the raw signature without the canonical bytes would look like proof
/// while being unverifiable, which is worse than a handle.
async fn signed_provenance(
    state: &AppState,
    source_path: &str,
) -> (Option<String>, Option<String>) {
    #[cfg(feature = "dag")]
    {
        if source_path.is_empty() {
            return (None, None);
        }
        if let Ok(actions) = crate::service::dag::history_by_subject(state, source_path, 1).await {
            if let Some(a) = actions.first() {
                // Only anchor to an action that actually carries a signature: an
                // unsigned action (including the by-design genesis) attests to
                // nothing, so surfacing its hash as an anchor would be a claim
                // with no signature behind it.
                let sig = if a.signed { Some(a.hash.clone()) } else { None };
                return (sig, Some(a.timestamp.clone()));
            }
        }
        (None, None)
    }
    #[cfg(not(feature = "dag"))]
    {
        let _ = (state, source_path);
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn enabled_state() -> AppState {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        state
    }

    #[tokio::test]
    async fn empty_memory_is_ungrounded() {
        let state = enabled_state().await;
        let g = ground(&state, "anything at all", 5).await.unwrap();
        assert_eq!(g.groundedness, "ungrounded");
        assert!(g.answer_context.is_empty());
        assert!(!g.gaps.is_empty());
        assert!(
            !g.index_stale,
            "a genuinely empty index is not stale — there are no chunks to be placeholders"
        );
    }

    /// A 384-dim embedder that emits ONLY zero vectors — reproduces the poisoned
    /// index (a placeholder model that got persisted, or a same-dim swap that
    /// left every stored vector at zero).
    struct Zero384;
    impl ineru::Embedder for Zero384 {
        fn embed_passage(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.0; 384])
        }
        fn embed_query(&self, _t: &str) -> ineru::Embedding {
            ineru::Embedding::new(vec![0.0; 384])
        }
        fn dimensions(&self) -> usize {
            384
        }
    }

    #[tokio::test]
    async fn stale_index_is_reported_not_silently_ungrounded() {
        // The regression: chunks EXIST and the engine reports Ready, yet every
        // stored embedding is a placeholder so nothing can ever ground. Before the
        // fix this returned a plain "ungrounded" and looked like an empty vault.
        // Now it must raise `index_stale` and say a re-index is required.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("n.md"),
            "# N\n\nsled has exclusive lock semantics.\n",
        )
        .unwrap();
        let state =
            AppState::with_db_path_and_embedder(":memory:", None, std::sync::Arc::new(Zero384))
                .unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let g = ground(&state, "exclusive lock semantics", 5).await.unwrap();
        assert!(
            g.index_stale,
            "an all-placeholder candidate pool must be reported as a stale index"
        );
        assert_eq!(
            g.groundedness, "ungrounded",
            "a stale index cannot ground anything"
        );
        assert!(
            g.gaps.iter().any(|s| s.to_lowercase().contains("stale")),
            "the gap must tell the user to re-index; got {:?}",
            g.gaps
        );
    }

    #[tokio::test]
    async fn single_corroborating_chunk_is_weak_not_grounded() {
        // One source, one chunk: even a strong similarity match must not be called
        // "grounded" — with the placeholder embedder a lone high score can be
        // spurious, so a single corroborating chunk is downgraded to "weak".
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("note.md"),
            "# Note\n\nWe chose sled for its exclusive lock semantics.\n",
        )
        .unwrap();
        let state = enabled_state().await;
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        // Query the chunk almost verbatim so the lone chunk scores well above HIGH.
        let g = ground(&state, "We chose sled for its exclusive lock semantics.", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.is_empty(),
            "should retrieve the one chunk"
        );
        assert_eq!(
            g.groundedness, "weak",
            "a single corroborating chunk must be weak, not grounded; ctx: {:?}",
            g.answer_context
        );
    }

    #[tokio::test]
    async fn two_corroborating_sources_are_grounded() {
        // The same fact stated in two separate files yields two strong chunks for a
        // matching query — that independent corroboration is what makes it grounded.
        let dir = tempfile::tempdir().unwrap();
        let fact = "# Doc\n\nThe quorum read requires a valid leader lease.\n";
        std::fs::write(dir.path().join("a.md"), fact).unwrap();
        std::fs::write(dir.path().join("b.md"), fact).unwrap();
        let state = enabled_state().await;
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let g = ground(&state, "The quorum read requires a valid leader lease.", 5)
            .await
            .unwrap();
        let strong = g
            .answer_context
            .iter()
            .filter(|c| c.relevance >= 0.55)
            .count();
        assert!(
            strong >= 2,
            "two sources should both score strongly; ctx: {:?}",
            g.answer_context
        );
        assert_eq!(
            g.groundedness, "grounded",
            "two corroborating strong chunks must be grounded; ctx: {:?}",
            g.answer_context
        );
    }

    #[tokio::test]
    async fn grounds_after_ingest_with_source() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("adr.md"),
            "# Storage\n\nWe chose sled because of its exclusive lock semantics.\n",
        )
        .unwrap();
        let state = enabled_state().await;
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let g = ground(&state, "exclusive lock semantics sled", 5)
            .await
            .unwrap();
        assert!(
            !g.answer_context.is_empty(),
            "should retrieve the ingested chunk"
        );
        assert_eq!(g.answer_context[0].source, "adr.md");
        assert_ne!(g.groundedness, "ungrounded");
    }

    /// End-to-end acceptance test for the real neural embedder: a topical query
    /// must be grounded while an off-topic query is ungrounded. Gated on the
    /// `neural-embeddings` feature and skips if the model files are absent.
    /// Requires `ORT_DYLIB_PATH` to point at an onnxruntime dynamic library.
    #[cfg(feature = "neural-embeddings")]
    #[tokio::test]
    async fn neural_grounding_is_topical() {
        let model_dir = std::env::var("INERU_E5_MODEL_DIR").unwrap_or_else(|_| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../ineru/test-models/multilingual-e5-small"
            )
            .to_string()
        });
        if !std::path::Path::new(&model_dir)
            .join("onnx/model.onnx")
            .exists()
        {
            eprintln!("skipping: e5 model not found at {model_dir}");
            return;
        }

        let embedder = crate::embedder::build_embedder(Some(&model_dir));
        assert_eq!(embedder.dimensions(), 384, "neural embedder must be active");

        let state = AppState::with_db_path_and_embedder(":memory:", None, embedder).unwrap();
        {
            let mut graph = state.graph.write().await;
            graph.enable_dag();
        }

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("dogs.md"),
            "# Cuidado de perros\n\nLos perros necesitan paseos diarios, agua fresca y una dieta equilibrada para estar sanos.\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("dogs2.md"),
            "# Mascotas\n\nUn perro sano requiere ejercicio diario, hidratación constante y alimentación balanceada.\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

        let topical = ground(&state, "¿Cómo debo cuidar a mi perro?", 5)
            .await
            .unwrap();
        assert_ne!(
            topical.groundedness, "ungrounded",
            "a dog-care question must find the dog-care notes; ctx: {:?}",
            topical.answer_context
        );

        let off_topic = ground(
            &state,
            "¿Cuál fue el resultado de las elecciones presidenciales?",
            5,
        )
        .await
        .unwrap();
        assert_eq!(
            off_topic.groundedness, "ungrounded",
            "an unrelated question must be ungrounded; ctx: {:?}",
            off_topic.answer_context
        );
    }

    // ── Lexical corroboration ─────────────────────────────────────────────────
    //
    // The signal that stops "grounded" being asserted over passages that merely
    // resemble the question without answering it.

    #[test]
    fn question_terms_keeps_topic_words_and_drops_function_words() {
        let t =
            question_terms("¿Cómo se protege el prompt para que una nota no falsifique una cita?");
        assert!(t.contains(&"prompt".to_string()));
        assert!(t.contains(&"nota".to_string()));
        assert!(t.contains(&"cita".to_string()));
        assert!(
            !t.contains(&"cómo".to_string()),
            "stop word survived: {t:?}"
        );
        assert!(
            !t.contains(&"para".to_string()),
            "stop word survived: {t:?}"
        );
    }

    #[test]
    fn question_terms_keeps_accented_and_non_latin_words_whole() {
        // Splitting on ASCII would shred these into fragments and the coverage
        // check would then never corroborate a non-English question.
        let t = question_terms("¿Qué decisión tomamos sobre la migración?");
        assert!(t.contains(&"decisión".to_string()), "{t:?}");
        assert!(t.contains(&"migración".to_string()), "{t:?}");
        let jp = question_terms("カルシファー とは 何ですか");
        assert!(jp.iter().any(|w| w.contains('カ')), "{jp:?}");
    }

    #[test]
    fn question_terms_dedupes() {
        let t = question_terms("cita cita CITA");
        assert_eq!(t, vec!["cita".to_string()]);
    }

    #[test]
    fn coverage_is_full_when_the_body_discusses_the_question() {
        let t = question_terms("vault passage sha256 defang");
        assert_eq!(
            term_coverage(
                &t,
                "the vault passage carries a sha256 and we defang markers"
            ),
            1.0
        );
    }

    #[test]
    fn coverage_collapses_when_the_body_is_about_something_else() {
        // The real failure this was built for: passages retrieved for a question
        // about citation forgery that were actually about autosave tests.
        let t =
            question_terms("¿Cómo se protege el prompt para que una nota no falsifique una cita?");
        let body = "flush-on-unmount parks the version that landed underneath,                     switching notes flushes the pending save for the previous note";
        assert!(
            term_coverage(&t, body) < MIN_QUESTION_TERM_COVERAGE,
            "coverage was {}, expected below the bar",
            term_coverage(&t, body)
        );
    }

    #[test]
    fn coverage_matches_an_inflected_form() {
        let t = question_terms("firma sign");
        assert_eq!(term_coverage(&t, "las firmas quedan signed en el DAG"), 1.0);
    }

    #[test]
    fn a_question_of_only_function_words_abstains_rather_than_refusing() {
        // Nothing to corroborate against: fall back to similarity alone, which
        // is the behaviour that existed before this check.
        let t = question_terms("what is that");
        assert_eq!(term_coverage(&t, "cualquier cosa"), 1.0);
    }
}
