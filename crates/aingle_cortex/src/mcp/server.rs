// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The `AingleMcp` MCP server handler and its tool router.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

use crate::state::AppState;

/// The refusal returned when the runtime policy is read-only. Taken from the
/// shared policy module so this surface and every other one refuse identically.
use crate::mcp::policy::read_only_denied;

/// This surface's tool classification, consumed by the single enforcement point
/// in [`crate::mcp::policy::gate_tool_call`].
///
/// A tool missing from BOTH lists is denied by default (treated as mutating);
/// `every_exposed_tool_is_classified` below turns that runtime denial into a
/// build-time failure, so a tool cannot ship unclassified.
pub const TOOL_ACCESS: crate::mcp::policy::ToolAccessTable =
    crate::mcp::policy::ToolAccessTable::new(
        &[
            "aingle_agenda",
            "aingle_agent_consistency",
            "aingle_backlinks",
            "aingle_cards",
            "aingle_dag_action",
            "aingle_dag_chain",
            "aingle_dag_history",
            "aingle_dag_stats",
            "aingle_dag_tips",
            "aingle_due_cards",
            "aingle_get_proof",
            "aingle_get_triple",
            "aingle_graph_stats",
            "aingle_ground",
            "aingle_list_folders",
            "aingle_list_predicates",
            "aingle_list_subjects",
            "aingle_list_tags",
            "aingle_list_triples",
            "aingle_note_context",
            "aingle_path",
            "aingle_ping",
            "aingle_query_pattern",
            "aingle_sources",
            "aingle_sparql",
            "aingle_tasks",
            "aingle_validate",
            "aingle_validate_skill",
            "aingle_vault_map",
            "aingle_verify_assertions_batch",
            "aingle_verify_proof",
        ],
        &[
            "aingle_batch_insert",
            "aingle_create_folder",
            "aingle_create_triple",
            "aingle_dag_prune",
            "aingle_delete_triple",
            "aingle_edit_note",
            "aingle_ingest",
            "aingle_propose_note",
            "aingle_sandbox_create",
            "aingle_sandbox_delete",
            "aingle_tag_add",
            "aingle_tag_remove",
        ],
    );

/// Drop every path-bearing entry of a vault map that the policy hides, so an
/// excluded folder never leaks through the map/navigation surface.
fn filter_vault_map(
    map: &mut crate::service::vault_map::VaultMap,
    pol: &crate::mcp::policy::McpPolicy,
) {
    map.entry_points.retain(|e| !pol.is_hidden(&e.path));
    map.orphans.retain(|p| !pol.is_hidden(p));
    map.skills.retain(|p| !pol.is_hidden(p));
    for g in &mut map.tag_clusters {
        g.notes.retain(|n| !pol.is_hidden(n));
    }
    map.tag_clusters.retain(|g| !g.notes.is_empty());
    map.topics.retain(|t| !pol.is_hidden(&t.representative));
    for t in &mut map.topics {
        t.notes.retain(|n| !pol.is_hidden(n));
        t.size = t.notes.len();
    }
    map.graph.nodes.retain(|n| !pol.is_hidden(&n.id));
    map.graph
        .edges
        .retain(|e| !pol.is_hidden(&e.source) && !pol.is_hidden(&e.target));
    if map
        .identity
        .as_deref()
        .map(|id| pol.is_hidden(id))
        .unwrap_or(false)
    {
        map.identity = None;
    }
    map.totals.orphans = map.orphans.len();
    map.totals.clusters = map.topics.len();
}

/// A stored triple is hidden if its subject or its (node) object resolves to an
/// excluded note path. Note paths are used as triple subjects, and `links_to`
/// targets are node objects — both are folder-scoped. Scalar/string objects are
/// never note paths, so they pass through.
fn triple_dto_hidden(pol: &crate::mcp::policy::McpPolicy, t: &crate::rest::TripleDto) -> bool {
    if pol.is_hidden(&t.subject) {
        return true;
    }
    matches!(&t.object, crate::rest::ValueDto::Node { node } if pol.is_hidden(node))
}

/// A SPARQL result row (a JSON object of bound values) is hidden if any bound
/// value string resolves to an excluded note path.
fn binding_hidden(pol: &crate::mcp::policy::McpPolicy, row: &serde_json::Value) -> bool {
    row.as_object().is_some_and(|m| {
        m.values()
            .filter_map(|v| v.as_str())
            .any(|s| pol.is_hidden(s))
    })
}

/// A DAG action DTO is hidden if any path-bearing field embeds a path under an
/// excluded folder.
///
/// Two fields can carry a path. The human-readable summary inlines note paths
/// verbatim for single-triple insert/delete actions (batch/count summaries carry
/// no path, and the content hash is a digest). The verification bundle carries
/// the *signed* payload JSON, which names every subject touched — including ones
/// a batch summary reduces to "N ops". Scrubbing the summary alone would let an
/// excluded note's path out through the payload, so both are scanned.
///
/// The scrub is a conservative substring match: it can hide an action it did not
/// have to, but it never under-matches a real exclusion.
#[cfg(feature = "dag")]
fn dag_dto_hidden(pol: &crate::mcp::policy::McpPolicy, d: &crate::rest::dag::DagActionDto) -> bool {
    if pol.text_references_excluded(&d.payload_summary) {
        return true;
    }
    d.verification
        .as_ref()
        .is_some_and(|v| payload_json_references_excluded(pol, &v.canonical.payload_json))
}

/// Substring-scrub a *serialized JSON* document for excluded folder paths.
///
/// JSON escapes a backslash as `\\`, so a Windows-style path inside a serialized
/// payload arrives with its separators doubled and a naive `\` → `/` rewrite
/// yields `Personal//Finanzas`, which no longer contains the excluded prefix.
/// Collapsing runs of separators first closes that escape hatch. The collapse is
/// only for matching (it also flattens `scheme://`), never for output.
///
/// Used for every field that publishes *material* rather than a summary: the
/// signed DAG payload, and — since a proof's bytes and metadata are now served
/// alongside its verdict — the stored proof JSON and its metadata. Whenever a
/// surface starts publishing the thing itself instead of a description of it,
/// this is the filter that has to follow.
fn payload_json_references_excluded(
    pol: &crate::mcp::policy::McpPolicy,
    payload_json: &str,
) -> bool {
    let mut normalized = String::with_capacity(payload_json.len());
    let mut prev_sep = false;
    for ch in payload_json.chars() {
        let sep = ch == '/' || ch == '\\';
        if sep && prev_sep {
            continue;
        }
        normalized.push(if sep { '/' } else { ch });
        prev_sep = sep;
    }
    pol.text_references_excluded(&normalized)
}

/// A replay bundle is hidden if the proof material it publishes names a path
/// under an excluded folder.
///
/// Public inputs are hex digests and curve points, so they cannot spell a path;
/// `proof_json` is the caller-supplied proof document verbatim and very much can.
/// A proof submitted *about* an excluded note carries that note's path in its
/// body, and a "verify" response now serves that body — so the same scrub the
/// signed DAG payload needed applies here, escaped separators included.
///
/// The statement-binding schemes add a second route: the bound statement is a
/// caller-supplied byte string that the bundle renders as text in
/// `statement_utf8`. A statement naming an excluded note would walk straight out
/// through a field the hex-only reasoning above does not cover, so it is scanned
/// too.
fn replay_references_excluded(
    pol: &crate::mcp::policy::McpPolicy,
    replay: &crate::proofs::ProofReplay,
) -> bool {
    if replay
        .proof_json
        .as_deref()
        .is_some_and(|j| payload_json_references_excluded(pol, j))
    {
        return true;
    }
    replay
        .statement_binding
        .statement_utf8
        .as_deref()
        .is_some_and(|s| pol.is_hidden(s) || payload_json_references_excluded(pol, s))
}

/// A verify response is hidden if its replay bundle names an excluded path.
fn verify_proof_hidden(
    pol: &crate::mcp::policy::McpPolicy,
    resp: &crate::rest::VerifyProofResponse,
) -> bool {
    resp.replay
        .as_ref()
        .is_some_and(|r| replay_references_excluded(pol, r))
}

/// A fetched proof is hidden if either its material or its metadata names an
/// excluded path. Metadata is free-form (`submitter`, `tags`, `extra`), so it is
/// scanned as serialized JSON rather than field by field — a new metadata field
/// must not silently become a new way out.
fn get_proof_hidden(
    pol: &crate::mcp::policy::McpPolicy,
    resp: &crate::rest::ProofResponse,
) -> bool {
    if resp
        .replay
        .as_ref()
        .is_some_and(|r| replay_references_excluded(pol, r))
    {
        return true;
    }
    serde_json::to_string(&resp.metadata)
        .map(|m| payload_json_references_excluded(pol, &m))
        .unwrap_or(true)
}

/// An assertion verdict is hidden if the assertion it is about — or the triple
/// the evidence now echoes back — resolves to an excluded note path.
///
/// The subject is a structured field, so `is_hidden` handles it; the evidence
/// carries the stored triple's display strings as well, which is a second way
/// out and is scanned as text.
fn assertion_result_hidden(
    pol: &crate::mcp::policy::McpPolicy,
    r: &crate::rest::AssertionVerifyResult,
) -> bool {
    if pol.is_hidden(&r.subject) {
        return true;
    }
    r.evidence
        .triple
        .as_ref()
        .is_some_and(|t| pol.is_hidden(&t.subject) || pol.text_references_excluded(&t.subject))
}

/// Drop the consistency units the policy hides **and recompute the score from
/// what remains**.
///
/// Filtering the list while leaving `total`/`verified`/`score` untouched would
/// publish a fraction whose parts do not add up — which both leaks the existence
/// of the hidden units and breaks the arithmetic the response invites the caller
/// to check. Recomputing keeps the published numbers exactly the numbers the
/// published list supports.
fn consistency_retain_visible(
    pol: &crate::mcp::policy::McpPolicy,
    resp: &mut crate::rest::ConsistencyResponse,
) {
    resp.assertions.retain(|u| {
        !pol.is_hidden(&u.subject)
            && !pol.text_references_excluded(&u.subject)
            && !u.triple.as_ref().is_some_and(|t| pol.is_hidden(&t.subject))
    });
    resp.total = resp.assertions.len();
    resp.evaluated = resp
        .assertions
        .iter()
        .filter(|u| u.verified.is_some())
        .count();
    resp.verified = resp
        .assertions
        .iter()
        .filter(|u| u.verified == Some(true))
        .count();
    // Recomputed through the same helper the service uses, so a unit that was
    // never evaluated stays out of the fraction here too.
    resp.score = crate::service::reputation::consistency_score(&resp.assertions);
}

/// Parameters for the `aingle_dag_history` tool.
#[cfg(feature = "dag")]
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DagHistoryParams {
    /// Subject IRI whose mutation history to fetch.
    pub subject: String,
    /// Max actions to return.
    #[serde(default = "default_hist_limit")]
    pub limit: usize,
}

#[cfg(feature = "dag")]
fn default_hist_limit() -> usize {
    crate::service::dag::DEFAULT_HISTORY_LIMIT
}

/// Parameters for the `aingle_dag_action` tool.
#[cfg(feature = "dag")]
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DagActionParams {
    /// Hex-encoded DAG action hash to fetch.
    pub hash: String,
}

/// Parameters for the `aingle_dag_chain` tool.
#[cfg(feature = "dag")]
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DagChainParams {
    /// Author identity whose action chain to fetch.
    pub author: String,
    /// Max actions to return.
    #[serde(default = "default_hist_limit")]
    pub limit: usize,
}

/// MCP server exposing AIngle Córtex capabilities as tools.
///
/// Wraps the shared [`AppState`] so tools can operate on the same graph,
/// proof store, and DAG as the REST/GraphQL surfaces.
#[derive(Clone)]
pub struct AingleMcp {
    pub(crate) state: AppState,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AingleMcp {
    /// Creates a new MCP handler bound to the given shared application state.
    pub fn new(state: AppState) -> Self {
        // Start from the core tool router. The dag-gated tools live in a
        // separate `#[tool_router(router = dag_tool_router)]` block so that the
        // macro never references them when the `dag` feature is off (keeping
        // `mcp` compilable standalone). Merge them in only when `dag` is on.
        #[allow(unused_mut)]
        let mut router = Self::tool_router();
        #[cfg(feature = "dag")]
        {
            router += Self::dag_tool_router();
        }
        // The sparql-gated tool likewise lives in its own
        // `#[tool_router(router = sparql_tool_router)]` block so the macro on the
        // core impl never references it when `sparql` is off. Merge it only when
        // `sparql` is on (it is in `default`, but `mcp` must compile without it).
        #[cfg(feature = "sparql")]
        {
            router += Self::sparql_tool_router();
        }
        Self {
            state,
            tool_router: router,
        }
    }

    /// Liveness probe tool.
    #[tool(description = "Liveness check; returns 'pong'.")]
    async fn aingle_ping(&self) -> String {
        "pong".to_string()
    }

    /// Ingest a markdown vault / code repo into the graph + memory with provenance.
    #[tool(
        description = "Ingest a markdown vault or code repo: auto-extracts triples \
            (frontmatter, wikilinks, headings, tags), indexes text chunks for \
            semantic recall, and records signed provenance. Incremental: unchanged \
            files are skipped. Confined to the configured workspace root: a path \
            outside it, or inside an excluded or hidden directory, is refused.",
        annotations(read_only_hint = false)
    )]
    async fn aingle_ingest(
        &self,
        params: Parameters<IngestParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(p) = params;
        let resp = crate::service::ingest::ingest_path(&self.state, &p.path, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Grounded retrieval: cited, provenance-backed context for a question.
    #[tool(
        description = "Answer-grounding for a question. Returns cited source chunks \
            (path:lines) with a groundedness signal and, per chunk, a \
            `provenance_anchor`: the hex hash of the DAG action that recorded that \
            source. The anchor is a POINTER, not a proof — it means a signed action \
            exists, as asserted by this server. To turn it into evidence, pass it to \
            `aingle_dag_action` and run the verification procedure that tool returns. \
            Do not describe a chunk as 'verified' unless you actually did that. \
            Answer ONLY from the returned context; if groundedness is not 'grounded', \
            say so and do not invent.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_ground(
        &self,
        params: Parameters<GroundParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut g = crate::service::ground::ground(&self.state, &p.question, p.k)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let pol = self.state.mcp_policy_snapshot();

        // Filter folder-excluded sources out of the answer context BEFORE deciding
        // answerability. Deciding first and filtering afterwards produced a
        // contradictory signal (`grounded`/`answerable:true` alongside an empty
        // context) whenever a question's only evidence lived in an excluded folder.
        g.answer_context.retain(|c| !pol.is_hidden(&c.source));

        // `answerable` is the authoritative flag and must never be `true` with an
        // empty context: an answer is only answerable when at least one visible
        // source remains AND (when the grounding gate is active) the retrieval is
        // grounded. Omitting the chunks on refusal leaves the model nothing
        // weakly-related to answer from, so it must say it doesn't know.
        let has_visible_source = !g.answer_context.is_empty();
        let grounding_ok = !pol.require_grounding || g.groundedness == "grounded";
        let answerable = has_visible_source && grounding_ok;

        if !answerable {
            // `index_stale` distinguishes "the vault has no evidence" from "the
            // vault's embeddings are placeholders and need a re-index" — without
            // it, a stale index looks identical to an empty one and the client
            // wrongly tells the user their notes are empty.
            let instruction = if g.index_stale {
                "The semantic index is stale (embeddings are placeholders): tell the \
                 user to re-index the vault, and do not claim their notes are empty."
            } else {
                "Insufficient grounded evidence in your notes; say you don't know and \
                 do not invent facts."
            };
            let refusal = serde_json::json!({
                "groundedness": g.groundedness,
                "answerable": false,
                "answer_context": [],
                "gaps": g.gaps,
                "index_stale": g.index_stale,
                "instruction": instruction,
            });
            return Ok(CallToolResult::success(vec![Content::json(refusal)?]));
        }

        // Normal branch: carry the visible grounded context plus an explicit
        // `answerable:true`. `groundedness` stays as computed (still informative),
        // but `answerable` is the flag clients should gate on.
        let payload = serde_json::json!({
            "groundedness": g.groundedness,
            "answerable": true,
            "answer_context": g.answer_context,
            "gaps": g.gaps,
            "index_stale": g.index_stale,
            "instruction": g.instruction,
        });
        Ok(CallToolResult::success(vec![Content::json(payload)?]))
    }

    /// Verified backlinks + outgoing links + unlinked mentions for a note.
    #[tool(
        description = "Verified backlinks, outgoing links, and unlinked mentions for a note. \
            Each backlink includes the source's context line and a `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) \
            when available. Use for accurate reverse navigation.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_backlinks(
        &self,
        params: Parameters<BacklinksParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut resp = crate::service::backlinks::backlinks(&self.state, &p.note).await;
        let pol = self.state.mcp_policy_snapshot();
        resp.backlinks.retain(|b| !pol.is_hidden(&b.path));
        resp.outgoing.retain(|path| !pol.is_hidden(path));
        resp.unlinked.retain(|path| !pol.is_hidden(path));
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// All task facts extracted from the vault, optionally filtered by status.
    #[tool(
        description = "All task facts extracted from the vault (open and closed), optionally \
            filtered by status (todo|doing|done|canceled). Each task carries its text, \
            status, priority, scheduled/deadline dates, effective due date, and a \
            `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) when available. Use to list or board a vault's tasks.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_tasks(
        &self,
        params: Parameters<TasksParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut rows = crate::service::tasks::list_tasks(&self.state, p.status.as_deref()).await;
        let pol = self.state.mcp_policy_snapshot();
        rows.retain(|r| r.note.as_deref().map(|n| !pol.is_hidden(n)).unwrap_or(true));
        Ok(CallToolResult::success(vec![Content::json(rows)?]))
    }

    /// Open, dated tasks bucketed against a reference day: overdue / today / upcoming.
    #[tool(
        description = "Open, dated tasks bucketed against a reference day (`today`, ISO \
            YYYY-MM-DD) into overdue, today, and upcoming (within `horizon_days`, default 7). \
            Each task carries its effective due date, priority, and `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) \
            when available. Use to plan or answer what is due.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_agenda(
        &self,
        params: Parameters<AgendaParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut resp =
            crate::service::tasks::agenda(&self.state, &p.today, p.horizon_days.unwrap_or(7)).await;
        let pol = self.state.mcp_policy_snapshot();
        let prune = |v: &mut Vec<crate::service::tasks::TaskRow>| {
            v.retain(|r| r.note.as_deref().map(|n| !pol.is_hidden(n)).unwrap_or(true));
        };
        prune(&mut resp.overdue);
        prune(&mut resp.today);
        prune(&mut resp.upcoming);
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// All spaced-repetition card facts extracted from the vault.
    #[tool(
        description = "All spaced-repetition card facts extracted from the vault. Each card \
            carries its front text, whether it is a cloze card, its scheduling state \
            (ease/interval/reps/due/last review/last grade when present), a status derived \
            against `today` (new|due|scheduled), and a `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) when \
            available. `today` is an ISO YYYY-MM-DD reference day. Use to browse or board a \
            vault's flashcards.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_cards(
        &self,
        params: Parameters<CardsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut rows = crate::service::cards::list_cards(&self.state, &p.today).await;
        let pol = self.state.mcp_policy_snapshot();
        rows.retain(|r| r.note.as_deref().map(|n| !pol.is_hidden(n)).unwrap_or(true));
        Ok(CallToolResult::success(vec![Content::json(rows)?]))
    }

    /// Cards bucketed for a review session against a reference day: due / new / scheduled.
    #[tool(
        description = "Cards bucketed for a review session against a reference day (`today`, \
            ISO YYYY-MM-DD): `due` (due on/before today), `new` (never scheduled), and \
            `scheduled` (due after today). Each card carries its front text, cloze flag, \
            scheduling state, and `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) when available. Use to drive or \
            answer what is due for review now (study `due` + `new`).",
        annotations(read_only_hint = true)
    )]
    async fn aingle_due_cards(
        &self,
        params: Parameters<DueCardsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut resp = crate::service::cards::due_cards(&self.state, &p.today).await;
        let pol = self.state.mcp_policy_snapshot();
        let prune = |v: &mut Vec<crate::service::cards::CardRow>| {
            v.retain(|r| r.note.as_deref().map(|n| !pol.is_hidden(n)).unwrap_or(true));
        };
        prune(&mut resp.due);
        prune(&mut resp.new);
        prune(&mut resp.scheduled);
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Verified context bundle for a note: semantically-related notes (by meaning,
    /// not just links) with the matching passage and signed provenance.
    #[tool(
        description = "Verified context bundle for a note: notes that are semantically \
            related by meaning (not just by explicit links), each with the matching \
            passage as evidence and a `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) when available. Use to \
            answer grounded in a note's verified neighborhood without hallucinating.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_note_context(
        &self,
        params: Parameters<NoteContextParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut resp = crate::service::context::note_context_cached(
            &self.state,
            &p.note,
            p.limit.unwrap_or(8),
        )
        .await;
        let pol = self.state.mcp_policy_snapshot();
        resp.neighbors.retain(|n| !pol.is_hidden(&n.path));
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Shortest verified connection between two notes: typed hops (link or
    /// semantic) with evidence for every step.
    #[tool(
        description = "Shortest verified connection between two notes in the vault. \
            Returns the chain of typed hops (link or semantic), each with its \
            similarity score and `provenance_anchor` \
            (a DAG action hash; a pointer, not a proof — verify it with \
            `aingle_dag_action`) when available, so every \
            step of the connection can be cited. Use when the user asks how two \
            topics, notes, or decisions relate.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_path(
        &self,
        params: Parameters<PathParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut resp =
            crate::service::path::find_path(&self.state, &p.from, &p.to, p.max_hops).await;
        // A chain is only as visible as its most hidden node: if policy hides
        // any node on the path, report no connection rather than leak the hop.
        let pol = self.state.mcp_policy_snapshot();
        if resp.found && resp.nodes.iter().any(|n| pol.is_hidden(n)) {
            resp.found = false;
            resp.nodes.clear();
            resp.hops.clear();
            resp.note = Some(format!("no connection within {} hops", resp.max_hops));
        }
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List ingested sources and their recorded content hashes.
    #[tool(
        description = "List ingested source files with their blake3 content hashes as \
            recorded at ingest time. A client holding the file can recompute the hash \
            and compare it; the record itself is attested by the DAG action carrying \
            that source's provenance, verifiable via `aingle_dag_action`.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_sources(&self) -> Result<CallToolResult, ErrorData> {
        let mut resp = crate::service::ingest::list_sources(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let pol = self.state.mcp_policy_snapshot();
        resp.retain(|r| !pol.is_hidden(&r.path));
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Vault Map & Navigation Manual: entry points, topics, orphans, indices,
    /// and guidance for navigating the vault accurately before answering.
    #[tool(
        description = "Vault map & navigation manual: hub entry-points, semantic topic \
            clusters, orphan notes, tag/type indices, and guidance. Call this FIRST to \
            navigate a vault accurately, then aingle_ground each claim.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_vault_map(&self) -> Result<CallToolResult, ErrorData> {
        let mut resp = crate::service::vault_map::vault_map_cached(&self.state).await;
        let pol = self.state.mcp_policy_snapshot();
        filter_vault_map(&mut resp, &pol);
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Query the semantic graph by triple pattern (any field omitted = wildcard).
    #[tool(
        description = "Query the semantic graph by triple pattern. Omit a field to wildcard it.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_query_pattern(
        &self,
        params: Parameters<crate::rest::PatternQueryRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let mut resp = crate::service::query::query_pattern(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let pol = self.state.mcp_policy_snapshot();
        resp.matches.retain(|t| !triple_dto_hidden(&pol, t));
        resp.total = resp.matches.len();
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List unique subjects in the graph, optionally filtered by predicate.
    #[tool(
        description = "List unique subjects in the semantic graph, optionally filtered by predicate.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_subjects(
        &self,
        params: Parameters<crate::rest::ListSubjectsQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let mut resp = crate::service::query::list_subjects(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let pol = self.state.mcp_policy_snapshot();
        resp.subjects.retain(|s| !pol.is_hidden(s));
        resp.total = resp.subjects.len();
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List unique predicates in the graph, optionally filtered by subject.
    #[tool(
        description = "List unique predicates in the semantic graph, optionally filtered by subject.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_predicates(
        &self,
        params: Parameters<crate::rest::ListPredicatesQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::query::list_predicates(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List every tag in the vault with the number of notes carrying it.
    #[tool(
        description = "List every tag in the vault with the number of notes carrying it. \
            Tags come from frontmatter `tags:` and inline `#tag`. Returns [{tag, count}], \
            sorted by tag.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_tags(&self) -> Result<CallToolResult, ErrorData> {
        let pol = self.state.mcp_policy_snapshot();
        let tags = crate::service::query::list_tags(&self.state, &pol)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let out: Vec<serde_json::Value> = tags
            .into_iter()
            .map(|(tag, count)| serde_json::json!({ "tag": tag, "count": count }))
            .collect();
        Ok(CallToolResult::success(vec![Content::json(out)?]))
    }

    /// List every folder (directory prefix) in the vault.
    #[tool(
        description = "List every folder (directory prefix) in the vault, derived from the \
            ingested source paths. Returns a sorted array of folder paths. Excluded folders \
            are omitted.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_folders(&self) -> Result<CallToolResult, ErrorData> {
        let pol = self.state.mcp_policy_snapshot();
        let folders = crate::service::query::list_folders(&self.state, &pol)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(folders)?]))
    }

    /// Edit a vault note (append/prepend/replace text), signed via the DAG.
    ///
    /// Mutation: not read-only. Destructive (it rewrites the note's file). NOT
    /// idempotent for append/prepend (each call adds another line); a
    /// `replace_text` whose `find` is already gone is a content no-op.
    #[tool(
        description = "Edit a vault note and sign the change into the DAG. `mode` is \
            'append' (add `text` as a trailing line), 'prepend' (leading line), or \
            'replace_text' (replace the first occurrence of `find` with `text`). Set \
            `dry_run` to preview the content-hash change and triple diff without writing. \
            The note path is vault-relative; paths escaping the vault or inside an excluded \
            folder are refused.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn aingle_edit_note(
        &self,
        params: Parameters<EditNoteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(p) = params;
        let mode = match p.mode.as_str() {
            "append" => crate::service::notes::EditMode::Append,
            "prepend" => crate::service::notes::EditMode::Prepend,
            "replace_text" => {
                let Some(find) = p.find else {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "replace_text mode requires a `find` string.",
                    )]));
                };
                crate::service::notes::EditMode::ReplaceText { find }
            }
            other => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "unknown mode '{other}': expected append|prepend|replace_text"
                ))]));
            }
        };
        let res = crate::service::notes::edit_note(&self.state, &p.note, mode, &p.text, p.dry_run)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(res)?]))
    }

    /// Add a tag to a vault note, signed via the DAG.
    #[tool(
        description = "Add a tag to a vault note (frontmatter `tags:` list when present, \
            else an inline `#tag`) and sign the change into the DAG. Idempotent: adding a \
            tag the note already has is a no-op. Set `dry_run` to preview.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn aingle_tag_add(
        &self,
        params: Parameters<TagParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(p) = params;
        let res = crate::service::notes::tag_add(&self.state, &p.note, &p.tag, p.dry_run)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(res)?]))
    }

    /// Remove a tag from a vault note, signed via the DAG.
    #[tool(
        description = "Remove a tag from a vault note (frontmatter `tags:` list or inline \
            `#tag`) and sign the change into the DAG. Idempotent: removing a tag the note \
            does not have is a no-op. Set `dry_run` to preview.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn aingle_tag_remove(
        &self,
        params: Parameters<TagParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(p) = params;
        let res = crate::service::notes::tag_remove(&self.state, &p.note, &p.tag, p.dry_run)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(res)?]))
    }

    /// Create a folder inside the vault.
    #[tool(
        description = "Create a folder (and any missing parents) inside the vault. The path \
            is vault-relative; paths escaping the vault or inside an excluded folder are \
            refused. Idempotent: an existing folder is fine.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn aingle_create_folder(
        &self,
        params: Parameters<CreateFolderParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(p) = params;
        let created = crate::service::notes::create_folder(&self.state, &p.path)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(
            serde_json::json!({ "created": created }),
        )?]))
    }

    /// Stage a proposed note into the vault's `_inbox/` for human review.
    ///
    /// Mutation: not read-only (it writes a staging file). Non-destructive: the
    /// file name is uniquified so an existing pending proposal is never
    /// overwritten. The staged note is NOT ingested or signed — the ingest walk
    /// skips top-level `_inbox/`, so it stays out of the graph until a human
    /// approves and moves it out (that approval flow lives in the app).
    #[tool(
        description = "Stage a PROPOSED note into the vault's `_inbox/` for human review. Use \
            this to add externally-sourced content (a web clip, an external AI's draft): the \
            note is written to `_inbox/<name>.md` with `status: pending` frontmatter and is \
            NOT indexed or signed until a human approves it and moves it out of `_inbox/`. \
            Provide `source` (URL/app/agent) and optional `tags`. Pass a stable \
            `idempotency_key` so a retried call returns the already-staged note instead of \
            writing a duplicate. The suggested `name` is sanitized into a safe filename.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false
        )
    )]
    async fn aingle_propose_note(
        &self,
        params: Parameters<ProposeNoteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(p) = params;
        let tags = p.tags.unwrap_or_default();
        let res = crate::service::notes::propose_note(
            &self.state,
            &p.name,
            &p.content,
            p.source.as_deref(),
            // `clipped` is not exposed as a tool param; the app/clipper that
            // pre-builds frontmatter can include it there instead.
            None,
            &tags,
            p.idempotency_key.as_deref(),
        )
        .await
        .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(res)?]))
    }

    /// Insert a triple (subject, predicate, object) into the graph.
    ///
    /// Mutation: not read-only. Non-destructive (it never removes or overwrites
    /// existing data). NOT idempotent: the graph keys triples by content hash,
    /// so inserting a triple that already exists (same content hash) returns an
    /// error rather than silently succeeding — a retried call may therefore fail.
    #[tool(
        description = "Insert a triple into the semantic graph. Mutates the graph.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false
        )
    )]
    async fn aingle_create_triple(
        &self,
        params: Parameters<crate::rest::CreateTripleRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(req) = params;
        let dto =
            crate::service::triples::create_triple(&self.state, req, None, Some(super::MCP_ORIGIN))
                .await
                .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(dto)?]))
    }

    /// Atomically bulk-insert triples into the graph.
    ///
    /// Mutation: not read-only. Non-destructive (only adds rows; never removes or
    /// overwrites). Idempotent: batch insert silently skips triples whose content
    /// hash already exists (see `GraphStore::insert_batch`), so retrying the same
    /// batch converges to the same state without error.
    #[tool(
        description = "Atomically bulk-insert triples into the semantic graph. Duplicates are skipped silently.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn aingle_batch_insert(
        &self,
        params: Parameters<crate::rest::BatchInsertRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(req) = params;
        let resp = crate::service::triples::batch_insert(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Fetch a single triple by its hex hash id.
    #[tool(
        description = "Fetch a single triple by its hex hash id.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_get_triple(
        &self,
        params: Parameters<crate::rest::TripleIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let dto = crate::service::triples::get_triple(&self.state, &req.id)
            .await
            .map_err(super::convert::to_mcp_error)?;
        // Do not reveal a triple whose subject/object lives in an excluded
        // folder; report it as absent (same shape as a genuinely missing id).
        let pol = self.state.mcp_policy_snapshot();
        if triple_dto_hidden(&pol, &dto) {
            return Err(super::convert::to_mcp_error(crate::error::Error::NotFound(
                format!("Triple {} not found", req.id),
            )));
        }
        Ok(CallToolResult::success(vec![Content::json(dto)?]))
    }

    /// Delete a triple by its hex hash id.
    ///
    /// Mutation: not read-only. Destructive (removes data). Idempotent: deleting
    /// an absent id is reported as not-found, but the resulting state (the triple
    /// no longer present) is the same on retry.
    #[tool(
        description = "Delete a triple from the semantic graph by its hex hash id.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn aingle_delete_triple(
        &self,
        params: Parameters<crate::rest::TripleIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(req) = params;
        crate::service::triples::delete_triple(&self.state, &req.id, None, Some(super::MCP_ORIGIN))
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(
            serde_json::json!({ "deleted": true, "id": req.id }),
        )?]))
    }

    /// List triples with optional subject/predicate filters and pagination.
    #[tool(
        description = "List triples with optional subject/predicate filters and pagination.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_list_triples(
        &self,
        params: Parameters<crate::rest::ListTriplesQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let mut resp = crate::service::triples::list_triples(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let pol = self.state.mcp_policy_snapshot();
        resp.triples.retain(|t| !triple_dto_hidden(&pol, t));
        resp.total = resp.triples.len();
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Return graph statistics (triple count and related metrics).
    #[tool(
        description = "Return graph statistics: triple count and related metrics.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_graph_stats(&self) -> Result<CallToolResult, ErrorData> {
        let resp = crate::service::stats::graph_stats(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Verify a stored proof by ID, publishing the material to replay the check.
    ///
    /// Read-only. Invalid/malformed proofs return `valid:false` (NOT an error);
    /// only a missing proof yields an error.
    #[tool(
        description = "Ask this node to check a stored cryptographic/ZK proof, and get \
            back what you need to check it yourself.\n\
            \n\
            `valid` is THIS SERVER'S ASSERTION about data this server stores — not \
            proof, however much the tool name suggests otherwise. Do not relay it as \
            'verified'.\n\
            \n\
            Read `replay.statement_binding.bound` SECOND, right after `valid`. When it \
            is false the challenge covers NO statement, so the same proof bytes verify \
            beside ANY assertion: a passing check tells you someone produced a valid \
            proof of that shape, never that it backs the claim it was served with. Say \
            that explicitly. When it is true, hex-decode \
            `replay.statement_binding.challenge_preimage_hex` — the EXACT bytes that \
            were hashed, never rebuild them yourself — sha256 them, confirm the result \
            equals `public_inputs.challenge`, and confirm the preimage contains \
            `statement_hex` and the R you recomputed. Then compare that statement with \
            the claim you were actually shown; if they differ, the proof is about \
            something else.\n\
            \n\
            Read `replay.check`; it names what was actually computed:\n\
            - `schnorr_discrete_log_statement_bound` (`aingle-zk-knowledge-v2`) — a real \
            verification, bound to a statement.\n\
            - `pedersen_commitment_equality_statement_bound` (`aingle-zk-equality-v2`) — \
            a real verification, bound to a statement.\n\
            - `schnorr_discrete_log` (`aingle-zk-knowledge-v1`) — a real verification but \
            NOT statement-binding. Replay: decode `public_parameters.generator_g` and \
            `public_inputs.commitment` as compressed ristretto255 points G and P, and \
            `challenge`/`response` as 32-byte scalars c and s; compute R' = s*G - c*P; \
            check sha256(compress(R') || commitment) == `challenge`.\n\
            - `pedersen_commitment_equality` (`aingle-zk-equality-v1`) — a real \
            verification, NOT statement-binding. Rebuild H from \
            `public_parameters.generator_h_derivation` and confirm it equals \
            `generator_h`; compute D = C1 - C2 and R' = s*H - c*D; check \
            sha256(compress(R') || compress(D)) == `challenge`.\n\
            - `well_formedness_only` — NOT a verification. The node only checked that \
            the commitment and salt are non-zero; opening the commitment needs the \
            committed data, which the node does not hold. `valid:true` here means \
            nothing about the committed value.\n\
            - `root_consistency_only` — NOT a verification. The node only compared the \
            proof's own root against the root it is filed under; membership needs the \
            member datum, which the node does not hold.\n\
            \n\
            `replay.establishes` / `does_not_establish` state the scope in words, and \
            `additional_input_required` names the input missing when the check cannot \
            settle the claim. Follow `replay.procedure`, then report WHICH STEPS YOU \
            RAN and whether the proof was statement-bound. If you did not run them, say \
            'the node reports this proof valid', not 'verified'. Invalid proofs return \
            valid:false, not an error.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_verify_proof(
        &self,
        params: Parameters<crate::rest::VerifyProofByIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let proof_id = req.proof_id.clone();
        let resp = crate::service::proof::verify_proof(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        // The published proof bytes can name an excluded note; report the proof
        // as absent rather than serve its material.
        let pol = self.state.mcp_policy_snapshot();
        if verify_proof_hidden(&pol, &resp) {
            return Err(super::convert::to_mcp_error(crate::error::Error::NotFound(
                format!("Proof {} not found", proof_id),
            )));
        }
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Fetch a stored proof by ID, with the material needed to check it.
    ///
    /// Read-only. A missing proof yields an error.
    #[tool(
        description = "Fetch a stored cryptographic/ZK proof by ID. Returns its record \
            plus `replay`: the proof bytes, the public parameters, the public inputs \
            and a step-by-step procedure for checking it yourself.\n\
            \n\
            `verified` is this server's cached verdict from a PAST call, not a check \
            performed now, and it is `false` for a proof that was never checked. It is \
            an assertion either way. Use `replay` — and read `replay.check`, which for \
            `well_formedness_only` and `root_consistency_only` means no claim was \
            verified at all. Read `replay.statement_binding.bound` too: when false, the \
            proof binds no statement and verifies beside any claim whatsoever, so it is \
            not evidence for the one it is filed with. Errors if the proof does not \
            exist.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_get_proof(
        &self,
        params: Parameters<crate::rest::GetProofRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let proof_id = req.proof_id.clone();
        let resp = crate::service::proof::get_proof(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        // Proof metadata is caller-supplied and can name an excluded note.
        let pol = self.state.mcp_policy_snapshot();
        if get_proof_hidden(&pol, &resp) {
            return Err(super::convert::to_mcp_error(crate::error::Error::NotFound(
                format!("Proof {} not found", proof_id),
            )));
        }
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Validate a semantic skill manifest against PoL rules.
    ///
    /// Read-only: validation never mutates state. Returns `{valid, errors}`;
    /// a manifest with unsatisfiable proof requirements yields `valid:false`
    /// with per-assertion error messages (not a tool error).
    #[tool(
        description = "Check a semantic skill manifest against this node's \
            proof-of-logic rules. Returns {valid, outcome, errors, checks, rule_set, \
            procedure, limitation}; does not mutate.\n\
            \n\
            `valid` is THIS NODE'S ASSERTION, and `limitation` says why it cannot be \
            more: reproducing it needs the node's rule set, which is configuration and \
            may include conditions that cannot be serialized at all. There is no \
            replay bundle here and there cannot be one.\n\
            \n\
            Read `outcome` first. `not_evaluated` (with `valid: null`) means NOTHING in \
            the manifest was examined — either no assertion asked for proof, or this \
            node has no rule that could check one (`rule_set.vacuous` is true, or \
            `rule_set.predicate_scoped_rule_count` is 0). That is a configuration gap on \
            THIS NODE, not a manifest defect: say so, and do not report the manifest as \
            bad. Read `checks` second: entries with `evaluated: false` were never \
            examined, and each evaluated check ran against a synthetic probe triple, not \
            the skill's real assertions. A matching rule means such a rule exists for \
            that predicate; it does not mean anything was validated.\n\
            \n\
            Report which checks ran and under which `rule_set.digest`. Never say a \
            skill is 'verified' on the strength of this tool.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_validate_skill(
        &self,
        params: Parameters<crate::rest::ValidateManifestRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::skill::validate_manifest(&self.state, req).await;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Create a temporary sandbox namespace for skill verification.
    ///
    /// Mutation: not read-only. Non-destructive (only registers new sandbox
    /// state; never removes or overwrites). Each call mints a fresh sandbox id,
    /// so it is not marked idempotent.
    #[tool(
        description = "Create a temporary sandbox namespace for skill testing. Returns {id, namespace}.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn aingle_sandbox_create(
        &self,
        params: Parameters<crate::rest::CreateSandboxRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(req) = params;
        let resp = crate::service::skill::create_sandbox(&self.state, req).await;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Delete a sandbox namespace by id, removing all triples under it.
    ///
    /// Mutation: not read-only. Destructive (removes the sandbox and its
    /// triples). Idempotent: deleting an absent id reports `deleted:false`, but
    /// the resulting state (sandbox gone) is the same on retry.
    #[tool(
        description = "Delete a sandbox namespace by id, removing all triples under it. Unknown id => deleted:false.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn aingle_sandbox_delete(
        &self,
        params: Parameters<crate::rest::DeleteSandboxRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(req) = params;
        let resp = crate::service::skill::delete_sandbox(&self.state, &req.id).await;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Compute an agent's assertion consistency score.
    ///
    /// Read-only: inspects the graph + logic engine; never mutates. An unknown
    /// agent returns a well-formed default ({score:0.0, total:0, verified:0}),
    /// not an error.
    #[tool(
        description = "Compute an agent's assertion consistency score: the fraction of \
            its assertions that pass this node's proof-of-logic validation. Returns \
            {score, total, evaluated, verified, assertions, rule_set, procedure}.\n\
            \n\
            The score is arithmetic over verdicts this server produced, so it is an \
            assertion too — and it is NOT a reputation or trust measurement. Check it \
            rather than repeat it: `total` must equal the length of `assertions`, \
            `evaluated` the number whose `verified` is not null, `verified` the number \
            marked true, and `score` = verified/evaluated.\n\
            \n\
            `score` is NULL when `evaluated` is 0 — either no assertions were found, or \
            no rule examined them (`rule_set.vacuous`). A null score means there is no \
            measurement; it is neither 0% nor 100%, and reporting either would invent a \
            result. Units with `outcome: not_evaluated` are excluded from both sides of \
            the fraction rather than counted as passes. Note also that `assertions` \
            mixes two units: `subject` entries count as verified when ANY triple on that \
            subject validates, `triple` entries are single assertions, and both count \
            as 1.\n\
            \n\
            Report the fraction, how many units were evaluated, and the rule-set state \
            you saw — not a bare percentage.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_agent_consistency(
        &self,
        params: Parameters<crate::rest::AgentConsistencyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let mut resp =
            crate::service::reputation::agent_consistency(&self.state, &req.agent_id, None).await;
        // The score used to be three numbers; it now enumerates the subjects
        // behind them, which is a new way for an excluded note path to leave.
        // Dropping hidden units keeps the published arithmetic self-consistent —
        // recompute rather than serve a fraction whose parts do not add up.
        let pol = self.state.mcp_policy_snapshot();
        consistency_retain_visible(&pol, &mut resp);
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Batch-verify assertion proofs (subject+predicate references).
    ///
    /// Read-only: verification never mutates. Missing/unknown assertions report
    /// `verified:false` per entry rather than erroring.
    #[tool(
        description = "Check assertions by (subject, predicate) against this node's \
            proof-of-logic rules. Returns per-assertion {verified, evidence} plus \
            `rule_set` and `procedure`.\n\
            \n\
            `verified` is THIS NODE'S ASSERTION, not proof, and the boolean alone is \
            lossy: `false` covers both 'no such triple here' and 'a rule rejected it'. \
            Read `evidence.outcome`, which separates every case into `accepted`, \
            `rejected`, `not_found` and `not_evaluated`. `not_found` means this node \
            does not hold the assertion and is NOT evidence it is false. \
            `not_evaluated` means the triple exists but no rule is enabled to examine \
            it — `verified` is null there, and calling it verified would claim a check \
            that never ran.\n\
            \n\
            Read `rule_set` before reporting anything: `vacuous: true` means no rules \
            are enabled and every found triple comes back `not_evaluated`; `rules` \
            enumerates what ran otherwise. `evidence.triple` publishes the \
            literal bytes of the evaluated triple's id, so you can confirm the verdict \
            is about the triple you meant (blake3-256 of subject_bytes || \
            predicate_bytes || object_bytes must equal triple_id).\n\
            \n\
            This verdict cannot be replayed from the response — reproducing it needs \
            the node's rule set. Report 'this node reports X under rule-set digest Y', \
            not 'verified'.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_verify_assertions_batch(
        &self,
        params: Parameters<crate::rest::BatchVerifyAssertionsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let mut resp =
            crate::service::reputation::batch_verify_assertions(&self.state, req, None).await;
        // The evidence now echoes the stored triple, including its subject, so
        // an excluded note can ride out on a verdict about it.
        let pol = self.state.mcp_policy_snapshot();
        resp.results.retain(|r| !assertion_result_hidden(&pol, r));
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Validate triple(s) against the PoL logic engine.
    ///
    /// Read-only: validation never mutates the graph. Returns per-triple
    /// validity + messages and an overall `valid` flag; an invalid triple yields
    /// `valid:false` (not a tool error).
    #[tool(
        description = "Run triple(s) through this node's proof-of-logic rule engine. \
            Returns {valid, outcome, results, proof_hash, proof, rule_set, procedure}; \
            invalid triples yield valid:false (not an error). Does not mutate.\n\
            \n\
            Read `outcome` BEFORE `valid`. It is one of `valid`, `invalid`, or \
            `not_evaluated`. `not_evaluated` means this node has NO enabled rules, so \
            nothing examined the triples and `valid` is null — report 'not evaluated', \
            never 'valid'. (`rule_set.vacuous` says the same about the configuration.) \
            Each entry of `results` carries its own `outcome` on the same three values.\n\
            \n\
            `valid` is otherwise THIS NODE'S ASSERTION — 'no enabled rule rejected these \
            triples'. `rule_set.rules` enumerates the rules that ran, with the effect of \
            each, so you can say what was actually checked instead of citing a count. \
            The verdict cannot be replayed from this response; reproducing it needs the \
            node's rule set, which is configuration and may include Rust closures \
            (`rule_set.rules[].opaque_conditions` counts them).\n\
            \n\
            `proof_hash` IS reproducible, and it is not what its name suggests. Check it: \
            (1) for each `proof.triples` entry, hex-decode subject_bytes, \
            predicate_bytes and object_bytes, concatenate the raw bytes with no \
            separators, blake3-256 them, and confirm the result equals `triple_id`; \
            (2) concatenate `proof.preimage_parts` (the ASCII triple-id hex strings, in \
            order, no separator) and blake3-256 those ASCII bytes — it must equal \
            `proof_hash`. What that digest commits to is WHICH triples were submitted. \
            It does not cover the verdict, the rule set or a timestamp, and it is not \
            signed, so it is never evidence that the triples are valid.\n\
            \n\
            Report which of these steps you ran.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_validate(
        &self,
        params: Parameters<crate::rest::ValidateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::validate::validate_triples(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }
}

/// Dag-gated tools, kept in a separate router so the `#[tool_router]` macro on
/// the core impl never references them when `dag` is off. The combined router
/// is assembled in [`AingleMcp::new`].
#[cfg(feature = "dag")]
#[tool_router(router = dag_tool_router)]
impl AingleMcp {
    /// Inspect the signed DAG provenance history of a subject (who changed what, newest first).
    #[tool(
        description = "Return the DAG provenance history of a subject (newest first). \
            Each entry carries `hash` and `signature_status` (`signed` / \
            `unsigned_by_design` / `unsigned`) but NOT the proof — the signed \
            payload is omitted here to keep list responses small. `signed` and \
            `signature_status` are claims by this server; to verify one, call \
            `aingle_dag_action` with that entry's `hash` and follow the procedure \
            it returns.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_dag_history(
        &self,
        params: Parameters<DagHistoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let pol = self.state.mcp_policy_snapshot();
        // The subject is an explicit input: never surface the history of a note
        // that lives in an excluded folder.
        if pol.is_hidden(&p.subject) {
            let empty: Vec<crate::rest::dag::DagActionDto> = Vec::new();
            return Ok(CallToolResult::success(vec![Content::json(empty)?]));
        }
        let mut h = crate::service::dag::history_by_subject(&self.state, &p.subject, p.limit)
            .await
            .map_err(super::convert::to_mcp_error)?;
        // Defense in depth: a batch action affecting this (public) subject could
        // still inline a co-edited hidden path in its summary; scrub those.
        h.retain(|a| !dag_dto_hidden(&pol, a));
        Ok(CallToolResult::success(vec![Content::json(h)?]))
    }

    /// Return the current DAG tip hashes and their count.
    #[tool(
        description = "Return the current DAG tip hashes (frontier) and their count.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_dag_tips(&self) -> Result<CallToolResult, ErrorData> {
        let resp = crate::service::dag::tips(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Fetch a single DAG action by its hex hash, with everything needed to
    /// verify its signature independently of this server.
    #[tool(
        description = "Fetch a single DAG action by its hex hash — the verifiable \
            lookup. Use it to turn a `provenance_anchor` or history entry into \
            evidence.\n\
            \n\
            `signature_status` is one of: `signed`; `unsigned_by_design` (the \
            genesis action, deliberately unsigned so every node computes the same \
            initial hash); or `unsigned` (no signature, no design reason). The \
            legacy `signed` boolean is this server's own claim about its own data — \
            never present it to the user as proof.\n\
            \n\
            For a signed action the result carries `verification` with the \
            signature bytes, the public key, and `canonical`: the literal values \
            that were hashed. Verify it yourself:\n\
            1. Concatenate, with no separators: parent count as u64 little-endian; \
            each `canonical.parents` entry as its 32 raw bytes; UTF-8 length of \
            `canonical.author_json` as u64 LE; `author_json`; `canonical.seq` as \
            u64 LE; `canonical.timestamp_rfc3339` as UTF-8 with NO length prefix; \
            UTF-8 length of `canonical.payload_json` as u64 LE; `payload_json`.\n\
            2. blake3-256 those bytes; the hex digest must equal `hash`. If not, \
            stop — the parts do not describe this action.\n\
            3. Ed25519-verify `verification.signature` over the 32 RAW digest bytes \
            using `verification.public_key`. A null key means this node does not \
            hold the author's key; say you could not verify.\n\
            4. `canonical.payload_json` is the signed record. `payload_summary`, \
            `payload_type` and `author` are display fields this server computed and \
            are NOT signed — check your citation against `payload_json`.\n\
            5. Pin `public_key` (also on `aingle_dag_stats` as \
            `signing_public_key`) and compare it every time; a signature only \
            proves the holder of THAT key signed those bytes.\n\
            \n\
            Then state what you did: which steps passed, and against which key. If \
            you did not run the check, say the action is 'reported as signed', not \
            'verified'.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_dag_action(
        &self,
        params: Parameters<DagActionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let resp = crate::service::dag::action(&self.state, &p.hash)
            .await
            .map_err(super::convert::to_mcp_error)?;
        // If the action's summary references an excluded path, report it as
        // absent rather than revealing the excluded note's mutation.
        let pol = self.state.mcp_policy_snapshot();
        if dag_dto_hidden(&pol, &resp) {
            return Err(super::convert::to_mcp_error(crate::error::Error::NotFound(
                format!("DAG action {} not found", p.hash),
            )));
        }
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Return an author's DAG action chain, newest first.
    #[tool(
        description = "Return an author's DAG action chain (newest first), up to limit. \
            Carries `signature_status` per action but not the proof; fetch an action \
            with `aingle_dag_action` to verify its signature.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_dag_chain(
        &self,
        params: Parameters<DagChainParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(p) = params;
        let mut resp = crate::service::dag::chain(&self.state, &p.author, p.limit)
            .await
            .map_err(super::convert::to_mcp_error)?;
        // Drop actions whose summary references an excluded note path.
        let pol = self.state.mcp_policy_snapshot();
        resp.retain(|a| !dag_dto_hidden(&pol, a));
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Return DAG statistics: action count and tip count.
    #[tool(
        description = "Return DAG statistics: action count, tip count, and this \
            node's Ed25519 `signing_public_key`. Pin that key on first use and \
            compare it against the key served with each signed action — a signature \
            only attests to the holder of that specific key.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_dag_stats(&self) -> Result<CallToolResult, ErrorData> {
        let resp = crate::service::dag::stats(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Prune the DAG according to a retention policy.
    ///
    /// Mutation: not read-only. Destructive (removes actions). NOT idempotent:
    /// a second call against an already-pruned DAG yields a different result.
    #[tool(
        description = "Prune the DAG per a retention policy (keep_all/keep_since/keep_last/keep_depth). Destructive.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn aingle_dag_prune(
        &self,
        params: Parameters<crate::rest::dag::PruneRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.state.mcp_policy_snapshot().allows_mutation() {
            return Ok(read_only_denied());
        }
        let Parameters(req) = params;
        let resp = crate::service::dag::prune(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }
}

/// Sparql-gated tools, kept in a separate router so the `#[tool_router]` macro
/// on the core impl never references them when `sparql` is off. The combined
/// router is assembled in [`AingleMcp::new`].
#[cfg(feature = "sparql")]
#[tool_router(router = sparql_tool_router)]
impl AingleMcp {
    /// Run a SPARQL query against the semantic graph.
    #[tool(
        description = "Execute a SPARQL query (SELECT/CONSTRUCT/ASK) against the semantic graph.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_sparql(
        &self,
        params: Parameters<crate::sparql::SparqlRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let query_text = req.query.clone();
        let mut resp = crate::service::sparql::execute(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        let pol = self.state.mcp_policy_snapshot();
        if !pol.excluded_folders.is_empty() {
            // SELECT / CONSTRUCT / DESCRIBE: drop any result row that binds a
            // value referencing an excluded note path.
            if let Some(rows) = resp.bindings.as_mut() {
                rows.retain(|row| !binding_hidden(&pol, row));
                if resp.triple_count.is_some() {
                    resp.triple_count = Some(rows.len());
                }
            }
            // ASK yields only a boolean, so there is no row to filter. Refuse the
            // query if its text names an excluded path — answering true/false
            // would itself leak the existence of a hidden note.
            if resp.boolean.is_some() && pol.text_references_excluded(&query_text) {
                return Ok(CallToolResult::error(vec![Content::text(
                    "SPARQL ASK over an excluded folder is not allowed while folder \
                     exclusions are active.",
                )]));
            }
        }
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }
}

/// Parameters for the `aingle_ingest` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct IngestParams {
    /// Path to ingest. Confined to the configured workspace root: a relative
    /// path is resolved against it, and a path outside it is refused.
    pub path: String,
}

/// Parameters for the `aingle_ground` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct GroundParams {
    /// The question to ground against ingested sources.
    pub question: String,
    /// Max chunks to retrieve.
    #[serde(default = "default_ground_k")]
    pub k: usize,
}

fn default_ground_k() -> usize {
    6
}

/// Parameters for the `aingle_backlinks` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct BacklinksParams {
    /// Note path (vault-relative) to get backlinks for, e.g. "ideas/sled.md".
    pub note: String,
}

/// Parameters for the `aingle_tasks` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct TasksParams {
    /// Optional status filter: `todo`, `doing`, `done`, or `canceled`.
    pub status: Option<String>,
}

/// Parameters for the `aingle_agenda` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct AgendaParams {
    /// Reference day as ISO `YYYY-MM-DD`; tasks bucket relative to it.
    pub today: String,
    /// Days ahead to include in the "upcoming" bucket (default 7).
    pub horizon_days: Option<i64>,
}

/// Parameters for the `aingle_cards` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct CardsParams {
    /// Reference day as ISO `YYYY-MM-DD`; each card's status is derived against it.
    pub today: String,
}

/// Parameters for the `aingle_due_cards` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DueCardsParams {
    /// Reference day as ISO `YYYY-MM-DD`; cards bucket relative to it.
    pub today: String,
}

/// Parameters for the `aingle_note_context` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct NoteContextParams {
    /// Note path (vault-relative) to get the verified context bundle for.
    pub note: String,
    /// Max number of related neighbors to return (default 8).
    pub limit: Option<usize>,
}

/// Parameters for the `aingle_edit_note` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct EditNoteParams {
    /// Vault-relative path of the note to edit, e.g. "ideas/sled.md".
    pub note: String,
    /// Edit mode: `append`, `prepend`, or `replace_text`.
    pub mode: String,
    /// Text to append/prepend, or the replacement for `replace_text`.
    pub text: String,
    /// For `replace_text`: the substring to find (first occurrence replaced).
    #[serde(default)]
    pub find: Option<String>,
    /// Preview only: compute the diff without writing or ingesting.
    #[serde(default)]
    pub dry_run: bool,
}

/// Parameters for the `aingle_tag_add` / `aingle_tag_remove` tools.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct TagParams {
    /// Vault-relative path of the note to tag/untag.
    pub note: String,
    /// The tag (without a leading `#`).
    pub tag: String,
    /// Preview only: compute the diff without writing or ingesting.
    #[serde(default)]
    pub dry_run: bool,
}

/// Parameters for the `aingle_create_folder` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct CreateFolderParams {
    /// Vault-relative folder path to create (parents are created as needed).
    pub path: String,
}

/// Parameters for the `aingle_propose_note` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ProposeNoteParams {
    /// Suggested note name/title; sanitized into a safe `_inbox/<name>.md` filename.
    pub name: String,
    /// The note body (markdown). If it does not already begin with a `---`
    /// frontmatter block, one is added (`source`, `status: pending`, `tags`).
    pub content: String,
    /// Where the content came from (URL, app, or agent) — recorded in frontmatter.
    #[serde(default)]
    pub source: Option<String>,
    /// Optional tags to record in the staged note's frontmatter.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Optional idempotency key: a repeated call with the same key returns the
    /// already-staged note instead of writing a duplicate.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// Parameters for the `aingle_path` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct PathParams {
    /// Start note: vault-relative path or bare name (wikilink-style resolution).
    pub from: String,
    /// Goal note: vault-relative path or bare name (wikilink-style resolution).
    pub to: String,
    /// Max hops to search (default 4, capped at 6).
    pub max_hops: Option<usize>,
}

impl AingleMcp {
    /// Apply the shared policy gate to a tool name. `Some(refusal)` means the
    /// call must not be dispatched.
    ///
    /// One call site — [`ServerHandler::call_tool`] below — so a tool added to
    /// this surface is covered whether or not anyone remembers to guard its
    /// body, and an unclassified tool is refused rather than allowed.
    pub(crate) fn gate(&self, tool: &str) -> Option<CallToolResult> {
        crate::mcp::policy::gate_tool_call(&self.state.mcp_policy_snapshot(), &TOOL_ACCESS, tool)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AingleMcp {
    /// Every tool call on this surface passes through here, so this is where the
    /// policy is enforced. The `#[tool_handler]` macro only synthesises
    /// `call_tool` when the impl does not already define one, so defining it
    /// here replaces the unguarded dispatch rather than sitting beside it.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(denied) = self.gate(&request.name) {
            return Ok(denied);
        }
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "AIngle Córtex MCP server: tools for querying and mutating \
             AIngle semantic graphs."
                .to_string(),
        );
        info
    }
}

#[cfg(test)]
mod tool_access_tests {
    use super::*;
    use crate::mcp::policy::{McpPolicy, Permission, ToolAccess};

    /// Every tool this surface actually exposes must be classified. An
    /// unclassified tool is denied at runtime (deny by default), which is the
    /// safe outcome but a silent one — this test makes it loud at build time.
    #[test]
    fn every_exposed_tool_is_classified() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let mcp = AingleMcp::new(state);
        for t in mcp.tool_router.list_all() {
            assert!(
                TOOL_ACCESS.is_declared(&t.name),
                "tool '{}' is exposed but not classified in TOOL_ACCESS",
                t.name
            );
        }
    }

    /// The mirror of `every_exposed_tool_is_classified`. That test proves the
    /// table covers the router (nothing escapes classification); this one proves
    /// the list [`crate::mcp::exposed_tools`] publishes is *exactly* the router
    /// and *exactly* the gate table, in both directions.
    ///
    /// Hosts render that list as "what the connected assistant can reach". A
    /// published list that drifts from the gate under-reports the surface, and a
    /// trust display that under-reports is worse than none because the user
    /// believes it. Building the list from a second, hand-maintained array is
    /// how that drift happens, so this test refuses to compile-and-pass unless
    /// the published list is derived from the table the gate consults.
    #[test]
    fn published_tool_list_matches_the_router_and_the_gate() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        state.set_mcp_policy(McpPolicy::default()); // read-only
        let mcp = AingleMcp::new(state);

        let published = crate::mcp::exposed_tools();

        let mut published_names: Vec<String> =
            published.iter().map(|d| d.name.to_string()).collect();
        published_names.sort();
        let mut router_names: Vec<String> = mcp
            .tool_router
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        router_names.sort();
        assert_eq!(
            published_names, router_names,
            "the published tool list and the router disagree: a host showing \
             this list would misreport what the assistant can reach"
        );

        for d in &published {
            assert_eq!(
                d.access,
                TOOL_ACCESS.access(d.name),
                "published access for '{}' disagrees with the gate table",
                d.name
            );
            // The published classification must predict the real verdict: under
            // a read-only policy, exactly the tools published as mutating are
            // the ones the gate refuses.
            assert_eq!(
                mcp.gate(d.name).is_some(),
                d.access == ToolAccess::Mutating,
                "published access for '{}' does not predict the gate verdict",
                d.name
            );
        }
    }

    /// The classification must agree with the `read_only_hint` each tool
    /// advertises to clients: a tool that tells the model "I only read" but is
    /// filed as mutating (or the reverse) is a lie in one direction or a hole in
    /// the other.
    #[test]
    fn classification_matches_the_advertised_read_only_hint() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let mcp = AingleMcp::new(state);
        for t in mcp.tool_router.list_all() {
            let Some(hint) = t.annotations.as_ref().and_then(|a| a.read_only_hint) else {
                continue; // no hint advertised; the table is the only authority
            };
            let expected = if hint {
                ToolAccess::ReadOnly
            } else {
                ToolAccess::Mutating
            };
            assert_eq!(
                TOOL_ACCESS.access(&t.name),
                expected,
                "tool '{}' advertises read_only_hint={hint} but is classified otherwise",
                t.name
            );
        }
    }

    /// A read-only policy refuses every mutating tool, and refuses a tool nobody
    /// classified rather than allowing it.
    #[test]
    fn read_only_policy_refuses_mutating_and_unknown_tools() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        state.set_mcp_policy(McpPolicy::default()); // read-only
        let mcp = AingleMcp::new(state.clone());

        for name in mcp
            .tool_router
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
        {
            let denied = mcp.gate(&name).is_some();
            assert_eq!(
                denied,
                TOOL_ACCESS.access(&name) == ToolAccess::Mutating,
                "read-only gate disagrees with the classification of '{name}'"
            );
        }

        // A tool that does not exist yet — the shape of tomorrow's addition.
        assert!(
            mcp.gate("aingle_delete_everything").is_some(),
            "an unclassified tool must be refused under a read-only policy"
        );

        // Granting write access lifts the refusal for declared mutating tools…
        state.set_mcp_policy(McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        });
        assert!(mcp.gate("aingle_edit_note").is_none());
    }
}

#[cfg(test)]
mod ingest_tools_tests {
    use super::*;

    #[test]
    fn router_exposes_ingest_ground_sources() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let mcp = AingleMcp::new(state);
        let names: Vec<String> = mcp
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for expected in [
            "aingle_ingest",
            "aingle_ground",
            "aingle_sources",
            "aingle_vault_map",
            "aingle_backlinks",
            "aingle_note_context",
            "aingle_path",
            "aingle_tasks",
            "aingle_agenda",
            "aingle_cards",
            "aingle_due_cards",
            "aingle_list_tags",
            "aingle_list_folders",
            "aingle_edit_note",
            "aingle_tag_add",
            "aingle_tag_remove",
            "aingle_create_folder",
            "aingle_propose_note",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "missing tool {expected}"
            );
        }
    }
}

#[cfg(test)]
mod policy_enforcement_tests {
    use super::*;
    use crate::mcp::policy::{McpPolicy, Permission};

    /// The JSON payload a tool serialises into its first (text) content block.
    fn json_of(result: &CallToolResult) -> serde_json::Value {
        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .expect("tool result must have a text content block")
            .text
            .clone();
        serde_json::from_str(&text).expect("tool content must be valid JSON")
    }

    /// A ready state whose graph has ingested two notes: one under an excluded
    /// folder and one public. Returns the state and the temp dir (kept alive).
    async fn state_with_vault() -> (AppState, tempfile::TempDir) {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Personal").join("Finanzas")).unwrap();
        std::fs::create_dir_all(dir.path().join("Public")).unwrap();
        std::fs::write(
            dir.path()
                .join("Personal")
                .join("Finanzas")
                .join("secret.md"),
            "# Secreto\n\nMi presupuesto privado y numeros de cuenta.\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Public").join("open.md"),
            "# Abierto\n\nContenido publico del roadmap del proyecto.\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        (state, dir)
    }

    /// A note inside an excluded folder must not appear in `aingle_sources`,
    /// while a note outside every excluded folder is still returned.
    #[tokio::test]
    async fn excluded_folder_hidden_from_sources() {
        let (state, _dir) = state_with_vault().await;
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        let mcp = AingleMcp::new(state);

        let result = mcp.aingle_sources().await.expect("aingle_sources ok");
        let paths: Vec<String> = json_of(&result)
            .as_array()
            .expect("sources is an array")
            .iter()
            .map(|r| {
                r.get("path")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .replace('\\', "/")
            })
            .collect();

        assert!(
            paths.iter().any(|p| p == "Public/open.md"),
            "public note must remain visible: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("Personal/Finanzas")),
            "excluded-folder note must be hidden: {paths:?}"
        );
    }

    /// Build an MCP handler over the shared vault with `Personal/Finanzas`
    /// excluded (ReadOnly). Returns the handler and the temp dir (kept alive).
    async fn excluded_mcp() -> (AingleMcp, tempfile::TempDir) {
        let (state, dir) = state_with_vault().await;
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        (AingleMcp::new(state), dir)
    }

    /// `aingle_list_subjects` must drop subjects under an excluded folder while
    /// keeping public ones. Note paths are triple subjects, so an unfiltered
    /// listing would leak the excluded note's very existence.
    #[tokio::test]
    async fn excluded_folder_hidden_from_list_subjects() {
        let (mcp, _dir) = excluded_mcp().await;
        let req: crate::rest::ListSubjectsQuery =
            serde_json::from_value(serde_json::json!({ "limit": 10_000 })).unwrap();

        let result = mcp
            .aingle_list_subjects(Parameters(req))
            .await
            .expect("list_subjects ok");
        let subjects: Vec<String> = json_of(&result)
            .get("subjects")
            .and_then(|s| s.as_array())
            .expect("subjects array")
            .iter()
            .map(|v| v.as_str().unwrap_or("").replace('\\', "/"))
            .collect();

        assert!(
            subjects.iter().any(|s| s.contains("Public/open.md")),
            "public subject must remain visible: {subjects:?}"
        );
        assert!(
            !subjects.iter().any(|s| s.contains("Personal/Finanzas")),
            "excluded subject must be hidden: {subjects:?}"
        );
    }

    /// `aingle_query_pattern` with a wildcard pattern must not return any triple
    /// whose subject/object lives under an excluded folder.
    #[tokio::test]
    async fn excluded_folder_hidden_from_query_pattern() {
        let (mcp, _dir) = excluded_mcp().await;
        let req: crate::rest::PatternQueryRequest =
            serde_json::from_value(serde_json::json!({ "limit": 10_000 })).unwrap();

        let result = mcp
            .aingle_query_pattern(Parameters(req))
            .await
            .expect("query_pattern ok");
        let payload = json_of(&result);
        let dump = payload.to_string().replace('\\', "/");
        assert!(
            dump.contains("Public/open.md"),
            "public triples must remain: {dump}"
        );
        assert!(
            !dump.contains("Personal/Finanzas"),
            "excluded-folder triples must be hidden: {dump}"
        );
    }

    /// `aingle_sparql` `SELECT ?s ?p ?o` must not bind any row that references
    /// an excluded note path.
    #[cfg(feature = "sparql")]
    #[tokio::test]
    async fn excluded_folder_hidden_from_sparql_select() {
        let (mcp, _dir) = excluded_mcp().await;
        let req: crate::sparql::SparqlRequest = serde_json::from_value(serde_json::json!({
            "query": "SELECT ?s ?p ?o WHERE { ?s ?p ?o }"
        }))
        .unwrap();

        let result = mcp.aingle_sparql(Parameters(req)).await.expect("sparql ok");
        let dump = json_of(&result).to_string().replace('\\', "/");
        assert!(
            !dump.contains("Personal/Finanzas"),
            "SPARQL rows must not reference excluded paths: {dump}"
        );
    }

    /// `aingle_dag_history` for a subject inside an excluded folder must surface
    /// nothing, and must never leak the excluded path.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn excluded_folder_hidden_from_dag_history() {
        let (mcp, _dir) = excluded_mcp().await;
        let params = DagHistoryParams {
            subject: "Personal/Finanzas/secret.md".to_string(),
            limit: 50,
        };

        let result = mcp
            .aingle_dag_history(Parameters(params))
            .await
            .expect("dag_history ok");
        let payload = json_of(&result);
        let rows = payload.as_array().expect("history is an array");
        assert!(
            rows.is_empty(),
            "history of an excluded subject must be empty: {payload}"
        );
        assert!(
            !payload
                .to_string()
                .replace('\\', "/")
                .contains("Personal/Finanzas"),
            "dag_history must not leak the excluded path: {payload}"
        );
    }

    /// A vault whose notes carry markdown tasks: a public note with an overdue,
    /// a due-today, an upcoming and a done task, plus a task in an excluded
    /// folder. Returns a ReadOnly MCP handler that hides `Private`, and the temp
    /// dir (kept alive). Reference day for the agenda tests is `2026-07-24`.
    async fn tasks_mcp() -> (AingleMcp, tempfile::TempDir) {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Notes")).unwrap();
        std::fs::create_dir_all(dir.path().join("Private")).unwrap();
        std::fs::write(
            dir.path().join("Notes").join("plan.md"),
            "# Plan\n\n\
             - [ ] [#A] Overdue thing \u{1F4C5} 2026-07-20\n\
             - [ ] Today thing \u{1F4C5} 2026-07-24\n\
             - [ ] Soon thing \u{1F4C5} 2026-07-28\n\
             - [x] Done thing \u{1F4C5} 2026-07-15\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Private").join("secret.md"),
            "# Secret\n\n- [ ] Secret task \u{1F4C5} 2026-07-25\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Private".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        (AingleMcp::new(state), dir)
    }

    /// `aingle_tasks` returns every task with its fields populated, and drops any
    /// task whose note lives under an excluded folder.
    #[tokio::test]
    async fn tasks_tool_returns_fields_and_hides_excluded() {
        let (mcp, _dir) = tasks_mcp().await;

        let result = mcp
            .aingle_tasks(Parameters(TasksParams { status: None }))
            .await
            .expect("aingle_tasks ok");
        let rows = json_of(&result);
        let rows = rows.as_array().expect("tasks is an array");

        let texts: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
            .collect();
        // The four public tasks are present; the excluded-folder task is not.
        assert_eq!(rows.len(), 4, "one task is folder-excluded: {texts:?}");
        assert!(texts.contains(&"Overdue thing"), "{texts:?}");
        assert!(texts.contains(&"Done thing"), "{texts:?}");
        assert!(
            !texts.contains(&"Secret task"),
            "excluded-folder task must be hidden: {texts:?}"
        );
        let dump = json_of(&result).to_string().replace('\\', "/");
        assert!(
            !dump.contains("Private"),
            "must not leak excluded path: {dump}"
        );

        // Field shape: the high-priority overdue task keeps its status/priority/due.
        let overdue = rows
            .iter()
            .find(|r| r.get("text").and_then(|t| t.as_str()) == Some("Overdue thing"))
            .expect("overdue task present");
        assert_eq!(overdue.get("status").and_then(|v| v.as_str()), Some("todo"));
        assert_eq!(
            overdue.get("priority").and_then(|v| v.as_str()),
            Some("high")
        );
        assert_eq!(
            overdue.get("deadline").and_then(|v| v.as_str()),
            Some("2026-07-20")
        );
        assert_eq!(
            overdue.get("due").and_then(|v| v.as_str()),
            Some("2026-07-20")
        );
    }

    /// `aingle_tasks` honours the status filter.
    #[tokio::test]
    async fn tasks_tool_filters_by_status() {
        let (mcp, _dir) = tasks_mcp().await;
        let result = mcp
            .aingle_tasks(Parameters(TasksParams {
                status: Some("done".into()),
            }))
            .await
            .expect("aingle_tasks ok");
        let rows = json_of(&result);
        let rows = rows.as_array().expect("tasks is an array");
        assert_eq!(rows.len(), 1, "only one done task: {rows:?}");
        assert_eq!(
            rows[0].get("text").and_then(|v| v.as_str()),
            Some("Done thing")
        );
    }

    /// `aingle_agenda` buckets open dated tasks by date relative to `today`, and
    /// excludes both closed tasks and tasks under an excluded folder.
    #[tokio::test]
    async fn agenda_tool_buckets_by_date_and_hides_excluded() {
        let (mcp, _dir) = tasks_mcp().await;
        let result = mcp
            .aingle_agenda(Parameters(AgendaParams {
                today: "2026-07-24".into(),
                horizon_days: Some(7),
            }))
            .await
            .expect("aingle_agenda ok");
        let payload = json_of(&result);

        let bucket = |name: &str| -> Vec<String> {
            payload
                .get(name)
                .and_then(|v| v.as_array())
                .expect("bucket array")
                .iter()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()).map(String::from))
                .collect()
        };
        assert_eq!(bucket("overdue"), ["Overdue thing"]);
        assert_eq!(bucket("today"), ["Today thing"]);
        assert_eq!(bucket("upcoming"), ["Soon thing"]);

        // The excluded-folder task (due 2026-07-25, would be upcoming) and the
        // done task never surface, and the excluded path never leaks.
        let dump = payload.to_string().replace('\\', "/");
        assert!(
            !dump.contains("Secret task"),
            "excluded task hidden: {dump}"
        );
        assert!(
            !dump.contains("Done thing"),
            "closed task not in agenda: {dump}"
        );
        assert!(
            !dump.contains("Private"),
            "excluded path must not leak: {dump}"
        );
    }

    /// A ready state whose graph has ingested a deck of cards: some in an
    /// excluded folder and some public. Returns the MCP handle and the temp dir.
    async fn cards_mcp() -> (AingleMcp, tempfile::TempDir) {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Notes")).unwrap();
        std::fs::create_dir_all(dir.path().join("Private")).unwrap();
        std::fs::write(
            dir.path().join("Notes").join("deck.md"),
            "# Deck\n\n\
             Due card #card <!-- srs id=aaaaaaaaaaaa ef=2.5 due=2026-07-20 -->\n\
             The capital is {{cloze Paris}}. #card <!-- srs id=bbbbbbbbbbbb ef=2.5 due=2026-08-01 -->\n\
             Fresh card #card\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Private").join("secret.md"),
            "# Secret\n\nSecret card #card <!-- srs id=cccccccccccc ef=2.5 due=2026-07-19 -->\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Private".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        (AingleMcp::new(state), dir)
    }

    /// `aingle_cards` lists every card with its fields, and drops any card whose
    /// note lives under an excluded folder.
    #[tokio::test]
    async fn cards_tool_returns_fields_and_hides_excluded() {
        let (mcp, _dir) = cards_mcp().await;
        let result = mcp
            .aingle_cards(Parameters(CardsParams {
                today: "2026-07-24".into(),
            }))
            .await
            .expect("aingle_cards ok");
        let payload = json_of(&result);
        let rows = payload.as_array().expect("cards is an array");
        assert_eq!(rows.len(), 3, "three public cards: {rows:?}");

        let cloze = rows
            .iter()
            .find(|r| r.get("cloze").and_then(|c| c.as_bool()) == Some(true))
            .expect("a cloze card");
        assert!(cloze
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap()
            .contains("{{cloze Paris}}"));

        let dump = payload.to_string().replace('\\', "/");
        assert!(
            !dump.contains("Secret card"),
            "excluded card hidden: {dump}"
        );
        assert!(
            !dump.contains("Private"),
            "excluded path must not leak: {dump}"
        );
    }

    /// `aingle_due_cards` buckets cards for review against `today`: due / new /
    /// scheduled, hiding excluded folders.
    #[tokio::test]
    async fn due_cards_tool_buckets_by_status() {
        let (mcp, _dir) = cards_mcp().await;
        let result = mcp
            .aingle_due_cards(Parameters(DueCardsParams {
                today: "2026-07-24".into(),
            }))
            .await
            .expect("aingle_due_cards ok");
        let payload = json_of(&result);
        let bucket = |name: &str| -> Vec<String> {
            payload
                .get(name)
                .and_then(|v| v.as_array())
                .expect("bucket array")
                .iter()
                .filter_map(|r| r.get("text").and_then(|t| t.as_str()).map(String::from))
                .collect()
        };
        assert_eq!(bucket("due"), ["Due card"], "due-on/before-today card");
        assert_eq!(bucket("new"), ["Fresh card"], "unscheduled card");
        assert_eq!(
            bucket("scheduled"),
            ["The capital is {{cloze Paris}}."],
            "future card"
        );
        let dump = payload.to_string().replace('\\', "/");
        assert!(
            !dump.contains("Secret card"),
            "excluded card hidden: {dump}"
        );
    }

    /// Under the default (ReadOnly) policy a mutation tool returns an error
    /// result instead of touching the graph.
    #[tokio::test]
    async fn mutation_denied_under_read_only_default() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let mcp = AingleMcp::new(state); // default policy = ReadOnly

        let req: crate::rest::CreateTripleRequest = serde_json::from_value(serde_json::json!({
            "subject": "http://example.org/a",
            "predicate": "http://example.org/knows",
            "object": "b",
        }))
        .unwrap();

        let result = mcp
            .aingle_create_triple(Parameters(req))
            .await
            .expect("tool returns a result (not a protocol error)");
        assert_eq!(
            result.is_error,
            Some(true),
            "read-only default must deny mutation: {result:?}"
        );
    }

    /// With `require_grounding` ON, an off-topic question the retrieval cannot
    /// ground must be refused: the tool signals `answerable:false`, omits the
    /// source chunks (so nothing weakly-related can be answered from), and reports
    /// a non-"grounded" groundedness. With the flag OFF (default) the SAME question
    /// returns the normal context shape (answerable not-false, sources present) —
    /// proving the gate only triggers under the flag.
    #[tokio::test]
    async fn require_grounding_declines_ungrounded_answers() {
        // Clearly off-topic w.r.t. the ingested finance/roadmap notes, so the
        // retrieval will not be "grounded".
        let off_topic = "¿Cuál es la mejor receta de pizza napolitana con mozzarella?";

        // Case A (refusal): gate ON.
        let (state, _dir) = state_with_vault().await;
        state.set_mcp_policy(McpPolicy {
            require_grounding: true,
            ..Default::default()
        });
        let mcp = AingleMcp::new(state);
        let req = GroundParams {
            question: off_topic.to_string(),
            k: 6,
        };
        let result = mcp.aingle_ground(Parameters(req)).await.expect("ground ok");
        let payload = json_of(&result);
        assert_eq!(
            payload.get("answerable").and_then(|v| v.as_bool()),
            Some(false),
            "gated refusal must signal answerable:false: {payload}"
        );
        let ctx = payload.get("answer_context").and_then(|v| v.as_array());
        assert!(
            ctx.map(|a| a.is_empty()).unwrap_or(true),
            "refusal must omit source chunks so nothing weak can be answered from: {payload}"
        );
        assert_ne!(
            payload.get("groundedness").and_then(|v| v.as_str()),
            Some("grounded"),
            "an off-topic question must not be grounded: {payload}"
        );

        // Case B (control): gate OFF (default) — normal context shape.
        let (state, _dir2) = state_with_vault().await;
        let mcp = AingleMcp::new(state); // default policy: require_grounding = false
        let req = GroundParams {
            question: off_topic.to_string(),
            k: 6,
        };
        let result = mcp.aingle_ground(Parameters(req)).await.expect("ground ok");
        let payload = json_of(&result);
        assert_ne!(
            payload.get("answerable").and_then(|v| v.as_bool()),
            Some(false),
            "with the gate off the tool must not refuse: {payload}"
        );
        assert!(
            payload.get("answer_context").is_some(),
            "normal shape must still carry answer_context: {payload}"
        );
    }

    /// Regression: when every grounded source for a question lives inside an
    /// excluded folder, the tool must NOT claim the answer is answerable while
    /// handing back an empty context. Before the fix, the normal branch hardcoded
    /// `answerable:true` and only afterwards filtered `answer_context` down to
    /// nothing — a contradictory signal (grounded/answerable but zero context)
    /// that invites hallucination. `answerable` must follow the visible context.
    #[tokio::test]
    async fn all_sources_excluded_makes_unanswerable() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Personal").join("Finanzas")).unwrap();
        std::fs::write(
            dir.path()
                .join("Personal")
                .join("Finanzas")
                .join("presupuesto.md"),
            "# Presupuesto\n\nEl presupuesto mensual de marketing es de 4200 euros.\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        let mcp = AingleMcp::new(state);

        let req = GroundParams {
            question: "¿Cuál es el presupuesto mensual de marketing?".to_string(),
            k: 6,
        };
        let result = mcp.aingle_ground(Parameters(req)).await.expect("ground ok");
        let payload = json_of(&result);

        let ctx = payload.get("answer_context").and_then(|v| v.as_array());
        assert!(
            ctx.map(|a| a.is_empty()).unwrap_or(true),
            "all evidence is folder-excluded, so answer_context must be empty: {payload}"
        );
        assert_eq!(
            payload.get("answerable").and_then(|v| v.as_bool()),
            Some(false),
            "answerable must be false when no visible source remains: {payload}"
        );
    }

    /// A create_triple issued through the MCP tool must tag the resulting DAG
    /// action with `origin = mcp`, so a host can later attribute "what the
    /// connected AI did". A non-MCP caller would leave the author at its node
    /// default.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn mcp_create_triple_tags_dag_origin_mcp() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        state.set_mcp_policy(McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        });
        let mcp = AingleMcp::new(state.clone());

        let req: crate::rest::CreateTripleRequest = serde_json::from_value(serde_json::json!({
            "subject": "note.md",
            "predicate": "links_to",
            "object": { "node": "other.md" },
        }))
        .unwrap();

        let result = mcp
            .aingle_create_triple(Parameters(req))
            .await
            .expect("create_triple ok");
        assert_ne!(
            result.is_error,
            Some(true),
            "read-write policy must allow the mutation: {result:?}"
        );

        // Read the subject's DAG history via the same graph accessor the
        // `aingle_dag_history` tool uses, and assert the newest action's author
        // is the MCP origin tag.
        let graph = state.graph.read().await;
        let actions = graph.dag_history_by_subject("note.md", 10).unwrap();
        let newest = actions
            .first()
            .expect("one DAG action recorded for the insert");
        assert_eq!(
            newest.author.as_name(),
            Some(crate::mcp::MCP_ORIGIN),
            "MCP-originated create must tag the DAG action author with origin=mcp, got {:?}",
            newest.author
        );
    }

    /// With ReadWrite enabled the same mutation succeeds — proving the gate is a
    /// real switch, not an unconditional denial.
    #[tokio::test]
    async fn mutation_allowed_under_read_write() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        state.set_mcp_policy(McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        });
        let mcp = AingleMcp::new(state);

        let req: crate::rest::CreateTripleRequest = serde_json::from_value(serde_json::json!({
            "subject": "http://example.org/a",
            "predicate": "http://example.org/knows",
            "object": "b",
        }))
        .unwrap();

        let result = mcp
            .aingle_create_triple(Parameters(req))
            .await
            .expect("tool returns a result");
        assert_ne!(
            result.is_error,
            Some(true),
            "read-write policy must allow mutation: {result:?}"
        );
    }

    /// Build an MCP handler over a vault with a public tagged note and an
    /// excluded-folder tagged note. Returns the handler + temp dir (kept alive).
    async fn tagged_vault_mcp() -> (AingleMcp, tempfile::TempDir) {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Public")).unwrap();
        std::fs::create_dir_all(dir.path().join("Personal").join("Finanzas")).unwrap();
        std::fs::write(
            dir.path().join("Public").join("roadmap.md"),
            "# Roadmap\n\nPlan del proyecto. #roadmap\n",
        )
        .unwrap();
        std::fs::write(
            dir.path()
                .join("Personal")
                .join("Finanzas")
                .join("secret.md"),
            "# Secreto\n\nNumeros privados. #money\n",
        )
        .unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        (AingleMcp::new(state), dir)
    }

    /// `aingle_list_tags` must surface a public note's tag but never a tag that
    /// only lives on a note inside an excluded folder.
    #[tokio::test]
    async fn list_tags_hides_excluded_folder_tags() {
        let (mcp, _dir) = tagged_vault_mcp().await;
        let result = mcp.aingle_list_tags().await.expect("list_tags ok");
        let tags: Vec<String> = json_of(&result)
            .as_array()
            .expect("tags is an array")
            .iter()
            .filter_map(|r| r.get("tag").and_then(|t| t.as_str()).map(String::from))
            .collect();
        assert!(
            tags.iter().any(|t| t == "roadmap"),
            "public tag visible: {tags:?}"
        );
        assert!(
            !tags.iter().any(|t| t == "money"),
            "excluded-folder tag must be hidden: {tags:?}"
        );
    }

    /// `aingle_list_folders` must surface a public folder but drop any folder at
    /// or under an excluded path.
    #[tokio::test]
    async fn list_folders_hides_excluded() {
        let (mcp, _dir) = tagged_vault_mcp().await;
        let result = mcp.aingle_list_folders().await.expect("list_folders ok");
        let folders: Vec<String> = json_of(&result)
            .as_array()
            .expect("folders is an array")
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.replace('\\', "/")))
            .collect();
        assert!(
            folders.iter().any(|f| f == "Public"),
            "public folder: {folders:?}"
        );
        assert!(
            !folders.iter().any(|f| f.starts_with("Personal/Finanzas")),
            "excluded folder must be hidden: {folders:?}"
        );
    }

    /// The note-edit tool must refuse to write under the read-only default
    /// policy (mirrors `mutation_denied_under_read_only_default`).
    #[tokio::test]
    async fn edit_note_denied_under_read_only_default() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "# N\n\nbody\n").unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        let mcp = AingleMcp::new(state); // default policy = ReadOnly

        let result = mcp
            .aingle_edit_note(Parameters(EditNoteParams {
                note: "note.md".into(),
                mode: "append".into(),
                text: "sneaky".into(),
                find: None,
                dry_run: false,
            }))
            .await
            .expect("tool returns a result (not a protocol error)");
        assert_eq!(
            result.is_error,
            Some(true),
            "read-only default must deny note edits: {result:?}"
        );
        // The file must be untouched by the denied edit.
        let on_disk = std::fs::read_to_string(dir.path().join("note.md")).unwrap();
        assert!(
            !on_disk.contains("sneaky"),
            "denied edit must not write: {on_disk}"
        );
    }

    /// The propose-note tool must refuse to stage under the read-only default
    /// policy, leaving `_inbox/` empty.
    #[tokio::test]
    async fn propose_note_denied_under_read_only_default() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        let mcp = AingleMcp::new(state); // default policy = ReadOnly

        let result = mcp
            .aingle_propose_note(Parameters(ProposeNoteParams {
                name: "clip".into(),
                content: "sneaky body".into(),
                source: Some("https://example.com".into()),
                tags: None,
                idempotency_key: None,
            }))
            .await
            .expect("tool returns a result (not a protocol error)");
        assert_eq!(
            result.is_error,
            Some(true),
            "read-only default must deny proposing a note: {result:?}"
        );
        // Nothing was staged.
        assert!(
            !dir.path().join("_inbox").exists(),
            "denied proposal must not create _inbox"
        );
    }

    /// End-to-end through the tool: with ReadWrite enabled, `aingle_propose_note`
    /// stages a pending note into `_inbox/` without indexing it.
    #[tokio::test]
    async fn propose_note_tool_stages_under_read_write() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        state.set_mcp_policy(McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        });
        let mcp = AingleMcp::new(state);

        let result = mcp
            .aingle_propose_note(Parameters(ProposeNoteParams {
                name: "web idea".into(),
                content: "Clipped content.".into(),
                source: Some("https://example.com/x".into()),
                tags: Some(vec!["research".into()]),
                idempotency_key: Some("k1".into()),
            }))
            .await
            .expect("propose_note ok");
        assert_ne!(
            result.is_error,
            Some(true),
            "read-write must allow: {result:?}"
        );
        let payload = json_of(&result);
        let rel = payload.get("rel_path").and_then(|v| v.as_str()).unwrap();
        assert!(rel.starts_with("_inbox/"), "staged under _inbox: {payload}");
        assert!(
            dir.path().join(rel).exists(),
            "file staged on disk: {payload}"
        );
    }

    /// End-to-end through the tool: with ReadWrite enabled, `aingle_tag_add`
    /// writes the tag and it shows up via `aingle_query_pattern(tagged)`.
    #[tokio::test]
    async fn tag_add_tool_surfaces_via_query_pattern() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "# N\n\nbody\n").unwrap();
        crate::service::ingest::ingest_path(&state, dir.path().to_str().unwrap(), None)
            .await
            .unwrap();
        state.set_vault_root(dir.path().to_path_buf());
        state.set_mcp_policy(McpPolicy {
            permission: Permission::ReadWrite,
            ..Default::default()
        });
        let mcp = AingleMcp::new(state);

        let res = mcp
            .aingle_tag_add(Parameters(TagParams {
                note: "note.md".into(),
                tag: "roadmap".into(),
                dry_run: false,
            }))
            .await
            .expect("tag_add ok");
        assert_ne!(res.is_error, Some(true), "read-write must allow: {res:?}");

        let req: crate::rest::PatternQueryRequest = serde_json::from_value(serde_json::json!({
            "predicate": "tagged",
            "limit": 1000,
        }))
        .unwrap();
        let q = mcp
            .aingle_query_pattern(Parameters(req))
            .await
            .expect("query ok");
        assert!(
            json_of(&q).to_string().contains("roadmap"),
            "tagged triple must be queryable after tag_add: {}",
            json_of(&q)
        );
    }

    // ========================================================================
    // Independent verifiability over the MCP surface
    // ========================================================================

    /// Write one signed action into `state`'s DAG and return its hex hash.
    #[cfg(feature = "dag")]
    async fn put_signed(state: &AppState, payload: aingle_graph::dag::DagPayload) -> String {
        let graph = state.graph.read().await;
        let store = graph.dag_store().unwrap();
        let parents = store.tips().unwrap();
        let mut a = aingle_graph::dag::DagAction {
            parents,
            author: aingle_graph::NodeId::named("node:1"),
            seq: 1,
            timestamp: chrono::Utc::now(),
            payload,
            signature: None,
        };
        state.dag_signing_key.as_ref().unwrap().sign(&mut a);
        store.put(&a).unwrap().to_hex()
    }

    #[cfg(feature = "dag")]
    fn insert_of(subject: &str, object: &str) -> aingle_graph::dag::DagPayload {
        aingle_graph::dag::DagPayload::TripleInsert {
            triples: vec![aingle_graph::dag::TripleInsertPayload {
                subject: subject.into(),
                predicate: "note:title".into(),
                object: serde_json::json!(object),
                provenance: None,
            }],
        }
    }

    /// The acceptance criterion, exercised through the tool an MCP client
    /// actually calls: from the tool's JSON alone — no server state, no
    /// `aingle_graph` types — rebuild the signed bytes, recompute the hash, and
    /// check the Ed25519 signature.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn dag_action_tool_output_alone_verifies_the_signature() {
        let mut state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        state.dag_signing_key = Some(std::sync::Arc::new(
            aingle_graph::dag::DagSigningKey::from_seed(&[3u8; 32]),
        ));
        let hash = put_signed(&state, insert_of("Public/open.md", "Roadmap")).await;
        let mcp = AingleMcp::new(state);

        let out = json_of(
            &mcp.aingle_dag_action(Parameters(DagActionParams { hash: hash.clone() }))
                .await
                .expect("dag_action ok"),
        );

        // ------------------------------------------------------------------
        // From here on: only `out`.
        // ------------------------------------------------------------------
        assert_eq!(out["signature_status"], "signed", "{out}");
        let v = &out["verification"];
        assert_eq!(v["spec"], "aingle-dag-action-v1", "{out}");
        assert!(
            !v["procedure"].as_array().expect("procedure").is_empty(),
            "the bundle must tell the caller how to verify it"
        );

        let unhex = |s: &str, n: usize| -> Vec<u8> {
            assert_eq!(s.len(), n * 2);
            (0..n)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
                .collect()
        };

        let c = &v["canonical"];
        let mut pre: Vec<u8> = Vec::new();
        let parents = c["parents"].as_array().unwrap();
        pre.extend_from_slice(&(parents.len() as u64).to_le_bytes());
        for p in parents {
            pre.extend_from_slice(&unhex(p.as_str().unwrap(), 32));
        }
        let author = c["author_json"].as_str().unwrap().as_bytes();
        pre.extend_from_slice(&(author.len() as u64).to_le_bytes());
        pre.extend_from_slice(author);
        pre.extend_from_slice(&c["seq"].as_u64().unwrap().to_le_bytes());
        pre.extend_from_slice(c["timestamp_rfc3339"].as_str().unwrap().as_bytes());
        let payload = c["payload_json"].as_str().unwrap().as_bytes();
        pre.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        pre.extend_from_slice(payload);

        let digest = blake3::hash(&pre);
        assert_eq!(
            digest.to_hex().to_string(),
            out["hash"].as_str().unwrap(),
            "the reconstructed preimage must hash to the advertised action hash"
        );

        let pk: [u8; 32] = unhex(v["public_key"].as_str().unwrap(), 32)
            .try_into()
            .unwrap();
        let sig: [u8; 64] = unhex(v["signature"].as_str().unwrap(), 64)
            .try_into()
            .unwrap();
        ed25519_dalek::Verifier::verify(
            &ed25519_dalek::VerifyingKey::from_bytes(&pk).unwrap(),
            digest.as_bytes(),
            &ed25519_dalek::Signature::from_bytes(&sig),
        )
        .expect("the signature published over MCP must verify");

        // The key offered here must be the same one the stats tool publishes for
        // out-of-band pinning, or pinning would be meaningless.
        let stats = json_of(&mcp.aingle_dag_stats().await.expect("stats ok"));
        assert_eq!(
            stats["signing_public_key"], v["public_key"],
            "the pinnable node key must match the key served with the action"
        );
    }

    /// Publishing the signed payload must not become a hole in the folder
    /// exclusion. A batch action summarises as "N ops" — no path in the summary —
    /// so a filter that only scrubs the summary would let an excluded note's path
    /// through inside `verification.canonical.payload_json`.
    #[cfg(feature = "dag")]
    #[tokio::test]
    async fn verification_payload_does_not_leak_excluded_paths() {
        let mut state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let mut g = state.graph.write().await;
            g.enable_dag();
        }
        state.dag_signing_key = Some(std::sync::Arc::new(
            aingle_graph::dag::DagSigningKey::from_seed(&[4u8; 32]),
        ));
        let hash = put_signed(
            &state,
            aingle_graph::dag::DagPayload::Batch {
                ops: vec![
                    insert_of("Public/open.md", "Roadmap"),
                    insert_of("Personal/Finanzas/secret.md", "Presupuesto"),
                ],
            },
        )
        .await;
        state.set_mcp_policy(McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        });
        let mcp = AingleMcp::new(state);

        let res = mcp
            .aingle_dag_action(Parameters(DagActionParams { hash }))
            .await;

        let dump = match res {
            Ok(ok) => format!("{ok:?}"),
            Err(e) => format!("{e:?}"),
        }
        .replace("\\\\", "/")
        .replace('\\', "/");
        assert!(
            !dump.contains("Personal/Finanzas"),
            "the signed payload must be scrubbed by the folder exclusion too: {dump}"
        );
    }

    /// The scrub must survive JSON escaping. A Windows-style path serialized into
    /// the signed payload arrives as `Personal\\Finanzas` (a doubled backslash),
    /// which a plain `\` → `/` rewrite turns into `Personal//Finanzas` — no longer
    /// a match for the excluded prefix.
    #[cfg(feature = "dag")]
    #[test]
    fn payload_scrub_sees_through_json_escaped_separators() {
        let pol = McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            ..Default::default()
        };
        assert!(super::payload_json_references_excluded(
            &pol,
            r#"{"subject":"Personal\\Finanzas\\secret.md"}"#
        ));
        assert!(super::payload_json_references_excluded(
            &pol,
            r#"{"subject":"Personal/Finanzas/secret.md"}"#
        ));
        assert!(!super::payload_json_references_excluded(
            &pol,
            r#"{"subject":"Public/open.md","object":"note://open"}"#
        ));
    }

    // ========================================================================
    // The exclusion hole that publishing proof material opens
    //
    // Before this work the proof tools returned a verdict and some counters, so
    // there was nothing for an excluded path to ride out on. Publishing the
    // proof bytes, the submitter and the tags changes that: a proof submitted
    // *about* an excluded note now carries that note's path in the response.
    // Same failure the DAG canonical payload had, same fix — scrub the material,
    // not only the summary, and see through JSON escaping while doing it.
    // ========================================================================

    /// Flatten a serialized tool result the way a reader would: every separator
    /// becomes `/`, and runs collapse. Without the collapse this helper would
    /// miss a Windows path that JSON escaped into `Personal\\Finanzas`, and the
    /// leak tests below would pass while leaking.
    fn flattened(dump: &str) -> String {
        let mut out = String::with_capacity(dump.len());
        let mut prev_sep = false;
        for ch in dump.chars() {
            let sep = ch == '/' || ch == '\\';
            if sep && prev_sep {
                continue;
            }
            out.push(if sep { '/' } else { ch });
            prev_sep = sep;
        }
        out
    }

    /// Submit a proof whose stored bytes name an excluded note, and return its id.
    async fn submit_proof_naming(state: &AppState, path_in_proof: &str) -> String {
        state
            .proof_store
            .submit(crate::proofs::SubmitProofRequest {
                proof_type: crate::proofs::ProofType::HashOpening,
                proof_data: serde_json::json!({
                    "type": "HashOpening",
                    "commitment": vec![1u8; 32],
                    "salt": vec![2u8; 32],
                    "about": path_in_proof,
                }),
                metadata: None,
            })
            .await
            .expect("submit")
    }

    fn excluding_finanzas() -> McpPolicy {
        McpPolicy {
            excluded_folders: vec!["Personal/Finanzas".into()],
            permission: Permission::ReadOnly,
            require_grounding: false,
        }
    }

    #[tokio::test]
    async fn verify_proof_does_not_carry_an_excluded_path_out_in_the_proof_bytes() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let id = submit_proof_naming(&state, "Personal/Finanzas/secret.md").await;
        state.set_mcp_policy(excluding_finanzas());
        let mcp = AingleMcp::new(state);

        let out = mcp
            .aingle_verify_proof(Parameters(crate::rest::VerifyProofByIdRequest {
                proof_id: id.clone(),
            }))
            .await;

        let dump = flattened(&match out {
            Ok(r) => serde_json::to_string(&r.content).unwrap_or_default(),
            Err(e) => format!("{e:?}"),
        });
        assert!(
            !dump.contains("Personal/Finanzas"),
            "publishing the proof bytes must not smuggle an excluded note's path \
             out through a 'verify' response: {dump}"
        );
    }

    #[tokio::test]
    async fn get_proof_does_not_carry_an_excluded_path_out_in_the_metadata() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let id = state
            .proof_store
            .submit(crate::proofs::SubmitProofRequest {
                proof_type: crate::proofs::ProofType::HashOpening,
                proof_data: serde_json::json!({
                    "type": "HashOpening",
                    "commitment": vec![3u8; 32],
                    "salt": vec![4u8; 32],
                }),
                // The path rides out on metadata rather than on the proof bytes.
                metadata: Some(crate::proofs::ProofMetadata {
                    submitter: Some("Personal/Finanzas/secret.md".into()),
                    tags: vec!["budget".into()],
                    extra: Default::default(),
                }),
            })
            .await
            .expect("submit");
        state.set_mcp_policy(excluding_finanzas());
        let mcp = AingleMcp::new(state);

        let out = mcp
            .aingle_get_proof(Parameters(crate::rest::GetProofRequest { proof_id: id }))
            .await;
        let dump = flattened(&match out {
            Ok(r) => serde_json::to_string(&r.content).unwrap_or_default(),
            Err(e) => format!("{e:?}"),
        });
        assert!(
            !dump.contains("Personal/Finanzas"),
            "proof metadata is published material too and must be scrubbed: {dump}"
        );
    }

    /// The same escape hatch the signed-payload scrub had: a Windows-style path
    /// inside the stored proof JSON arrives with doubled separators, which a naive
    /// `\` → `/` rewrite turns into `Personal//Finanzas`.
    #[tokio::test]
    async fn proof_scrub_sees_through_json_escaped_separators() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let id = submit_proof_naming(&state, r"Personal\Finanzas\secret.md").await;
        state.set_mcp_policy(excluding_finanzas());
        let mcp = AingleMcp::new(state);

        let out = mcp
            .aingle_verify_proof(Parameters(crate::rest::VerifyProofByIdRequest {
                proof_id: id,
            }))
            .await;
        let dump = flattened(&match out {
            Ok(r) => serde_json::to_string(&r.content).unwrap_or_default(),
            Err(e) => format!("{e:?}"),
        });
        assert!(
            !dump.contains("Personal/Finanzas"),
            "an escaped Windows path must not slip past the proof scrub: {dump}"
        );
    }

    /// The PoL surfaces publish the evaluated triple now, not just a boolean, so
    /// a verdict about an excluded note carries that note's path out.
    #[tokio::test]
    async fn assertion_verdicts_about_an_excluded_note_are_not_served() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let g = state.graph.write().await;
            g.insert(aingle_graph::Triple::new(
                aingle_graph::NodeId::named("Personal/Finanzas/secret.md"),
                aingle_graph::Predicate::named("note:title"),
                aingle_graph::Value::literal("Presupuesto"),
            ))
            .unwrap();
            g.insert(aingle_graph::Triple::new(
                aingle_graph::NodeId::named("Public/open.md"),
                aingle_graph::Predicate::named("note:title"),
                aingle_graph::Value::literal("Roadmap"),
            ))
            .unwrap();
        }
        state.set_mcp_policy(excluding_finanzas());
        let mcp = AingleMcp::new(state);

        let out = json_of(
            &mcp.aingle_verify_assertions_batch(Parameters(
                crate::rest::BatchVerifyAssertionsRequest {
                    assertions: vec![
                        crate::rest::AssertionRef {
                            subject: "Personal/Finanzas/secret.md".into(),
                            predicate: "note:title".into(),
                        },
                        crate::rest::AssertionRef {
                            subject: "Public/open.md".into(),
                            predicate: "note:title".into(),
                        },
                    ],
                },
            ))
            .await
            .expect("batch verify ok"),
        );

        let dump = flattened(&out.to_string());
        assert!(
            !dump.contains("Personal/Finanzas"),
            "a verdict about an excluded note must not be served: {dump}"
        );
        // But the unrelated assertion must still come back with its evidence.
        assert_eq!(out["results"].as_array().unwrap().len(), 1, "{out}");
        assert_eq!(out["results"][0]["evidence"]["found"], true, "{out}");
    }

    /// Hiding a scored unit must also correct the arithmetic; a fraction whose
    /// parts do not add up both leaks and defeats the checking it invites.
    #[tokio::test]
    async fn consistency_recomputes_its_score_after_hiding_units() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        {
            let g = state.graph.write().await;
            for subject in [
                "mayros:agent:a1:Personal/Finanzas/secret.md",
                "mayros:agent:a1:Public/open.md",
            ] {
                g.insert(aingle_graph::Triple::new(
                    aingle_graph::NodeId::named(subject),
                    aingle_graph::Predicate::named("ex:says"),
                    aingle_graph::Value::literal("yes"),
                ))
                .unwrap();
            }
        }
        state.set_mcp_policy(excluding_finanzas());
        let mcp = AingleMcp::new(state);

        let out = json_of(
            &mcp.aingle_agent_consistency(Parameters(crate::rest::AgentConsistencyRequest {
                agent_id: "a1".into(),
            }))
            .await
            .expect("consistency ok"),
        );

        let dump = flattened(&out.to_string());
        assert!(
            !dump.contains("Personal/Finanzas"),
            "an excluded subject must not appear in the score breakdown: {dump}"
        );
        let units = out["assertions"].as_array().expect("assertions");
        assert_eq!(
            out["total"].as_u64().unwrap() as usize,
            units.len(),
            "the denominator must match the list actually served: {out}"
        );
        let verified = units.iter().filter(|u| u["verified"] == true).count();
        assert_eq!(
            out["verified"].as_u64().unwrap() as usize,
            verified,
            "{out}"
        );
    }

    /// The scrub must not become a blanket refusal: a proof that names nothing
    /// excluded still has to come back, replay bundle and all.
    #[tokio::test]
    async fn an_unrelated_proof_still_verifies_with_its_replay_bundle() {
        let state = AppState::with_db_path(":memory:", None).unwrap();
        let id = submit_proof_naming(&state, "Public/open.md").await;
        state.set_mcp_policy(excluding_finanzas());
        let mcp = AingleMcp::new(state);

        let out = json_of(
            &mcp.aingle_verify_proof(Parameters(crate::rest::VerifyProofByIdRequest {
                proof_id: id,
            }))
            .await
            .expect("an unrelated proof must still be served"),
        );
        assert_eq!(
            out["replay"]["scheme"], "aingle-zk-hash-opening-v1",
            "{out}"
        );
    }
}
