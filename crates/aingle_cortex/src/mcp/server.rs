// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The `AingleMcp` MCP server handler and its tool router.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

use crate::state::AppState;

/// Error result returned by mutation tools when the runtime MCP policy is
/// read-only. Centralised so every mutation tool emits the same message.
fn read_only_denied() -> CallToolResult {
    CallToolResult::error(vec![Content::text(
        "MCP is read-only: enable write access in Akashi to allow this.",
    )])
}

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

/// A DAG action DTO is hidden if its human-readable payload summary embeds a
/// path under any excluded folder. Only single-triple insert/delete summaries
/// inline note paths verbatim (batch/count summaries carry no path and the
/// content hash is a digest, not a path), so a conservative substring scrub over
/// the summary is a sound filter that never under-matches a real exclusion.
#[cfg(feature = "dag")]
fn dag_dto_hidden(pol: &crate::mcp::policy::McpPolicy, d: &crate::rest::dag::DagActionDto) -> bool {
    pol.text_references_excluded(&d.payload_summary)
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
            files are skipped."
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
            (path:lines) with a signed-provenance anchor and a groundedness signal. \
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
            let refusal = serde_json::json!({
                "groundedness": g.groundedness,
                "answerable": false,
                "answer_context": [],
                "gaps": g.gaps,
                "instruction": "Insufficient grounded evidence in your notes; \
                    say you don't know and do not invent facts.",
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
            "instruction": g.instruction,
        });
        Ok(CallToolResult::success(vec![Content::json(payload)?]))
    }

    /// Verified backlinks + outgoing links + unlinked mentions for a note.
    #[tool(
        description = "Verified backlinks, outgoing links, and unlinked mentions for a note. \
            Each backlink includes the source's context line and a signed-provenance anchor \
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

    /// Verified context bundle for a note: semantically-related notes (by meaning,
    /// not just links) with the matching passage and signed provenance.
    #[tool(
        description = "Verified context bundle for a note: notes that are semantically \
            related by meaning (not just by explicit links), each with the matching \
            passage as evidence and a signed-provenance anchor when available. Use to \
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

    /// List ingested sources and their signed content hashes.
    #[tool(
        description = "List ingested source files with their content hashes (the \
            signed provenance registry).",
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

    /// Verify a stored proof by ID; returns {valid: bool, ...}.
    ///
    /// Read-only. Invalid/malformed proofs return `valid:false` (NOT an error);
    /// only a missing proof yields an error.
    #[tool(
        description = "Verify a cryptographic/ZK proof by ID. Returns valid:false for invalid proofs (not an error).",
        annotations(read_only_hint = true)
    )]
    async fn aingle_verify_proof(
        &self,
        params: Parameters<crate::rest::VerifyProofByIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::proof::verify_proof(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Fetch a stored proof by ID; returns its metadata.
    ///
    /// Read-only. A missing proof yields an error.
    #[tool(
        description = "Fetch a stored cryptographic/ZK proof by ID. Errors if the proof does not exist.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_get_proof(
        &self,
        params: Parameters<crate::rest::GetProofRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp = crate::service::proof::get_proof(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Validate a semantic skill manifest against PoL rules.
    ///
    /// Read-only: validation never mutates state. Returns `{valid, errors}`;
    /// a manifest with unsatisfiable proof requirements yields `valid:false`
    /// with per-assertion error messages (not a tool error).
    #[tool(
        description = "Validate a semantic skill manifest against PoL rules. Returns {valid, errors}; does not mutate.",
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
        description = "Compute an agent's assertion consistency score (fraction of its assertions that pass PoL validation). Unknown agent => score 0.0.",
        annotations(read_only_hint = true)
    )]
    async fn aingle_agent_consistency(
        &self,
        params: Parameters<crate::rest::AgentConsistencyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp =
            crate::service::reputation::agent_consistency(&self.state, &req.agent_id, None).await;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Batch-verify assertion proofs (subject+predicate references).
    ///
    /// Read-only: verification never mutates. Missing/unknown assertions report
    /// `verified:false` per entry rather than erroring.
    #[tool(
        description = "Batch-verify assertion proofs by (subject, predicate). Returns a per-assertion verified flag; unknown assertions => verified:false (not an error).",
        annotations(read_only_hint = true)
    )]
    async fn aingle_verify_assertions_batch(
        &self,
        params: Parameters<crate::rest::BatchVerifyAssertionsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let Parameters(req) = params;
        let resp =
            crate::service::reputation::batch_verify_assertions(&self.state, req, None).await;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// Validate triple(s) against the PoL logic engine.
    ///
    /// Read-only: validation never mutates the graph. Returns per-triple
    /// validity + messages and an overall `valid` flag; an invalid triple yields
    /// `valid:false` (not a tool error).
    #[tool(
        description = "Validate triple(s) against the PoL logic engine. Returns {valid, results, proof_hash}; invalid triples yield valid:false (not an error). Does not mutate.",
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
        description = "Return the signed DAG provenance history of a subject (newest first).",
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

    /// Fetch a single DAG action by its hex hash.
    #[tool(
        description = "Fetch a single DAG action by its hex hash.",
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
        description = "Return an author's DAG action chain (newest first), up to limit.",
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
        description = "Return DAG statistics: action count and tip count.",
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
    /// Absolute or relative path to the vault/repo root to ingest.
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

/// Parameters for the `aingle_note_context` tool.
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct NoteContextParams {
    /// Note path (vault-relative) to get the verified context bundle for.
    pub note: String,
    /// Max number of related neighbors to return (default 8).
    pub limit: Option<usize>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AingleMcp {
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
        let result = mcp
            .aingle_ground(Parameters(req))
            .await
            .expect("ground ok");
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
        let result = mcp
            .aingle_ground(Parameters(req))
            .await
            .expect("ground ok");
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
        let result = mcp
            .aingle_ground(Parameters(req))
            .await
            .expect("ground ok");
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
    /// action with `origin = mcp`, so Akashi can later attribute "what your AI
    /// did". A non-MCP caller would leave the author at its node default.
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
        let newest = actions.first().expect("one DAG action recorded for the insert");
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
}
