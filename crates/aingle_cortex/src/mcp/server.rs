// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! The `AingleMcp` MCP server handler and its tool router.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};

use crate::state::AppState;

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
        let resp = crate::service::ground::ground(&self.state, &p.question, p.k)
            .await
            .map_err(super::convert::to_mcp_error)?;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
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
        let resp = crate::service::backlinks::backlinks(&self.state, &p.note).await;
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
        let resp = crate::service::context::note_context_cached(
            &self.state,
            &p.note,
            p.limit.unwrap_or(8),
        )
        .await;
        Ok(CallToolResult::success(vec![Content::json(resp)?]))
    }

    /// List ingested sources and their signed content hashes.
    #[tool(
        description = "List ingested source files with their content hashes (the \
            signed provenance registry).",
        annotations(read_only_hint = true)
    )]
    async fn aingle_sources(&self) -> Result<CallToolResult, ErrorData> {
        let resp = crate::service::ingest::list_sources(&self.state)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
        let resp = crate::service::vault_map::vault_map_cached(&self.state).await;
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
        let resp = crate::service::query::query_pattern(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
        let resp = crate::service::query::list_subjects(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
        let Parameters(req) = params;
        let dto = crate::service::triples::create_triple(&self.state, req, None)
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
        let Parameters(req) = params;
        crate::service::triples::delete_triple(&self.state, &req.id, None)
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
        let resp = crate::service::triples::list_triples(&self.state, req, None)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
        let h = crate::service::dag::history_by_subject(&self.state, &p.subject, p.limit)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
        let resp = crate::service::dag::chain(&self.state, &p.author, p.limit)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
        let resp = crate::service::sparql::execute(&self.state, req)
            .await
            .map_err(super::convert::to_mcp_error)?;
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
