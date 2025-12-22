//! GraphQL resolvers

use async_graphql::*;

use super::schema::*;
use crate::state::AppState;
use aingle_graph::{NodeId, Predicate, TriplePattern};

/// Query root
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get a triple by ID (hash)
    async fn triple(&self, ctx: &Context<'_>, id: ID) -> Result<Option<Triple>> {
        let state = ctx.data::<AppState>()?;
        let graph = state.graph.read().await;

        let triple_id = aingle_graph::TripleId::from_hex(&id.to_string())
            .ok_or_else(|| Error::new("Invalid triple ID"))?;

        match graph.get(&triple_id) {
            Ok(Some(t)) => Ok(Some(t.into())),
            Ok(None) => Ok(None),
            Err(e) => Err(Error::new(e.to_string())),
        }
    }

    /// Query triples with filters
    async fn triples(
        &self,
        ctx: &Context<'_>,
        filter: Option<TripleFilter>,
        #[graphql(default = 100)] limit: i32,
        #[graphql(default = 0)] offset: i32,
    ) -> Result<Vec<Triple>> {
        let state = ctx.data::<AppState>()?;
        let graph = state.graph.read().await;

        let mut pattern = TriplePattern::any();

        if let Some(f) = filter {
            if let Some(ref subject) = f.subject {
                pattern = pattern.with_subject(NodeId::named(subject));
            }
            if let Some(ref predicate) = f.predicate {
                pattern = pattern.with_predicate(Predicate::named(predicate));
            }
        }

        let triples = graph.find(pattern)?;

        Ok(triples
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|t| t.into())
            .collect())
    }

    /// Execute a pattern query
    async fn query(
        &self,
        ctx: &Context<'_>,
        pattern: PatternInput,
        #[graphql(default = 100)] limit: i32,
    ) -> Result<QueryResult> {
        let state = ctx.data::<AppState>()?;
        let graph = state.graph.read().await;

        let mut pat = TriplePattern::any();

        if let Some(ref s) = pattern.subject {
            pat = pat.with_subject(NodeId::named(s));
        }
        if let Some(ref p) = pattern.predicate {
            pat = pat.with_predicate(Predicate::named(p));
        }
        if let Some(ref o) = pattern.object {
            let obj: aingle_graph::Value = o.clone().into();
            pat = pat.with_object(obj);
        }

        let triples = graph.find(pat)?;

        let total = triples.len() as i32;
        let matches: Vec<Triple> = triples
            .into_iter()
            .take(limit as usize)
            .map(|t| t.into())
            .collect();

        Ok(QueryResult { matches, total })
    }

    /// Get graph statistics
    async fn stats(&self, ctx: &Context<'_>) -> Result<GraphStats> {
        let state = ctx.data::<AppState>()?;
        let stats = state.stats().await;

        Ok(GraphStats {
            triple_count: stats.triple_count as i32,
            subject_count: stats.subject_count as i32,
            predicate_count: stats.predicate_count as i32,
            object_count: stats.object_count as i32,
        })
    }

    /// List all unique subjects
    async fn subjects(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 100)] limit: i32,
    ) -> Result<Vec<String>> {
        let state = ctx.data::<AppState>()?;
        let graph = state.graph.read().await;

        let triples = graph.find(TriplePattern::any())?;
        let mut subjects: Vec<String> =
            triples.into_iter().map(|t| t.subject.to_string()).collect();
        subjects.sort();
        subjects.dedup();

        Ok(subjects.into_iter().take(limit as usize).collect())
    }

    /// List all unique predicates
    async fn predicates(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 100)] limit: i32,
    ) -> Result<Vec<String>> {
        let state = ctx.data::<AppState>()?;
        let graph = state.graph.read().await;

        let triples = graph.find(TriplePattern::any())?;
        let mut predicates: Vec<String> = triples
            .into_iter()
            .map(|t| t.predicate.to_string())
            .collect();
        predicates.sort();
        predicates.dedup();

        Ok(predicates.into_iter().take(limit as usize).collect())
    }
}

/// Mutation root
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Create a new triple
    async fn create_triple(&self, ctx: &Context<'_>, input: TripleInput) -> Result<Triple> {
        let state = ctx.data::<AppState>()?;

        let object: aingle_graph::Value = input.object.into();

        let triple = aingle_graph::Triple::new(
            NodeId::named(&input.subject),
            Predicate::named(&input.predicate),
            object,
        );

        {
            let graph = state.graph.read().await;
            graph.insert(triple.clone())?;
        }

        // Broadcast event
        state
            .broadcaster
            .broadcast(crate::state::Event::TripleAdded {
                hash: triple.id().to_hex(),
                subject: input.subject,
                predicate: input.predicate,
                object: serde_json::json!({}),
            });

        Ok(triple.into())
    }

    /// Delete a triple by ID
    async fn delete_triple(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let state = ctx.data::<AppState>()?;

        let triple_id = aingle_graph::TripleId::from_hex(&id.to_string())
            .ok_or_else(|| Error::new("Invalid triple ID"))?;

        let deleted = {
            let graph = state.graph.read().await;
            graph.delete(&triple_id)?
        };

        if deleted {
            state
                .broadcaster
                .broadcast(crate::state::Event::TripleDeleted {
                    hash: id.to_string(),
                });
        }

        Ok(deleted)
    }

    /// Validate triples
    async fn validate(
        &self,
        ctx: &Context<'_>,
        triples: Vec<TripleInput>,
    ) -> Result<TripleValidationResult> {
        let state = ctx.data::<AppState>()?;
        let logic = state.logic.read().await;

        let mut all_valid = true;
        let mut messages = Vec::new();

        for input in &triples {
            let object: aingle_graph::Value = input.object.clone().into();
            let triple = aingle_graph::Triple::new(
                NodeId::named(&input.subject),
                Predicate::named(&input.predicate),
                object,
            );

            let validation = logic.validate(&triple);

            if !validation.is_valid() {
                all_valid = false;
            }

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
        }

        // Generate proof hash if valid
        let proof_hash = if all_valid && !triples.is_empty() {
            let mut hasher = blake3::Hasher::new();
            for input in &triples {
                hasher.update(input.subject.as_bytes());
                hasher.update(input.predicate.as_bytes());
            }
            Some(hasher.finalize().to_hex().to_string())
        } else {
            None
        };

        Ok(TripleValidationResult {
            valid: all_valid,
            messages,
            proof_hash,
        })
    }
}
