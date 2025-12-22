use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents an architectural decision record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitecturalDecision {
    /// Unique identifier (e.g., ADR-001)
    pub id: String,

    /// Short, descriptive title
    pub title: String,

    /// The situation that led to this decision
    pub context: String,

    /// What was decided
    pub decision: String,

    /// Why this decision was made (reasoning and justification)
    pub rationale: String,

    /// Alternative approaches that were considered
    pub alternatives: Vec<Alternative>,

    /// Expected impact and trade-offs
    pub consequences: String,

    /// Files affected by this decision
    pub related_files: Vec<String>,

    /// Related decisions (IDs)
    pub related_decisions: Vec<String>,

    /// Current status of the decision
    pub status: DecisionStatus,

    /// Author email or identifier
    pub author: String,

    /// When the decision was made
    pub timestamp: DateTime<Utc>,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl ArchitecturalDecision {
    /// Create a new architectural decision
    pub fn new(
        id: String,
        title: String,
        context: String,
        decision: String,
        rationale: String,
        author: String,
    ) -> Self {
        Self {
            id,
            title,
            context,
            decision,
            rationale,
            alternatives: Vec::new(),
            consequences: String::new(),
            related_files: Vec::new(),
            related_decisions: Vec::new(),
            status: DecisionStatus::Proposed,
            author,
            timestamp: Utc::now(),
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add an alternative that was considered
    pub fn add_alternative(&mut self, alternative: Alternative) {
        self.alternatives.push(alternative);
    }

    /// Add a related file path
    pub fn add_file(&mut self, file_path: String) {
        if !self.related_files.contains(&file_path) {
            self.related_files.push(file_path);
        }
    }

    /// Add a tag
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// Link to another decision
    pub fn link_decision(&mut self, decision_id: String) {
        if !self.related_decisions.contains(&decision_id) {
            self.related_decisions.push(decision_id);
        }
    }

    /// Accept the decision
    pub fn accept(&mut self) {
        self.status = DecisionStatus::Accepted;
    }

    /// Supersede this decision with another
    pub fn supersede(&mut self, by_decision_id: String) {
        self.status = DecisionStatus::Superseded(by_decision_id);
    }

    /// Mark as deprecated
    pub fn deprecate(&mut self, reason: String) {
        self.status = DecisionStatus::Deprecated(reason);
    }
}

/// An alternative approach that was considered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// Description of the alternative
    pub description: String,

    /// Advantages of this approach
    pub pros: Vec<String>,

    /// Disadvantages of this approach
    pub cons: Vec<String>,

    /// Why this alternative was rejected
    pub rejected_reason: String,
}

impl Alternative {
    pub fn new(description: String) -> Self {
        Self {
            description,
            pros: Vec::new(),
            cons: Vec::new(),
            rejected_reason: String::new(),
        }
    }

    pub fn with_pros(mut self, pros: Vec<String>) -> Self {
        self.pros = pros;
        self
    }

    pub fn with_cons(mut self, cons: Vec<String>) -> Self {
        self.cons = cons;
        self
    }

    pub fn with_rejection_reason(mut self, reason: String) -> Self {
        self.rejected_reason = reason;
        self
    }
}

/// Status of an architectural decision
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DecisionStatus {
    /// Decision has been proposed but not yet accepted
    Proposed,

    /// Decision has been accepted and is in effect
    Accepted,

    /// Decision has been superseded by another decision
    Superseded(String),

    /// Decision has been deprecated
    Deprecated(String),
}

impl DecisionStatus {
    pub fn as_str(&self) -> &str {
        match self {
            DecisionStatus::Proposed => "Proposed",
            DecisionStatus::Accepted => "Accepted",
            DecisionStatus::Superseded(_) => "Superseded",
            DecisionStatus::Deprecated(_) => "Deprecated",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, DecisionStatus::Proposed | DecisionStatus::Accepted)
    }
}

/// Context about specific code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContext {
    /// Path to the file
    pub file_path: String,

    /// Optional function or module name
    pub function_name: Option<String>,

    /// Explanation of why this code exists or how it works
    pub explanation: String,

    /// Linked decision IDs
    pub linked_decisions: Vec<String>,

    /// When this context was added
    pub timestamp: DateTime<Utc>,

    /// Author who added this context
    pub author: String,
}

impl CodeContext {
    pub fn new(
        file_path: String,
        explanation: String,
        author: String,
    ) -> Self {
        Self {
            file_path,
            function_name: None,
            explanation,
            linked_decisions: Vec::new(),
            timestamp: Utc::now(),
            author,
        }
    }

    pub fn with_function(mut self, function_name: String) -> Self {
        self.function_name = Some(function_name);
        self
    }

    pub fn link_decision(&mut self, decision_id: String) {
        if !self.linked_decisions.contains(&decision_id) {
            self.linked_decisions.push(decision_id);
        }
    }
}

/// A Git commit linked to architectural decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedCommit {
    /// Git commit hash
    pub commit_hash: String,

    /// Commit message
    pub message: String,

    /// Author of the commit
    pub author: String,

    /// Commit timestamp
    pub timestamp: DateTime<Utc>,

    /// Decisions implemented/referenced in this commit
    pub decision_ids: Vec<String>,

    /// Files changed in this commit
    pub files_changed: Vec<String>,
}

impl LinkedCommit {
    pub fn new(
        commit_hash: String,
        message: String,
        author: String,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            commit_hash,
            message,
            author,
            timestamp,
            decision_ids: Vec::new(),
            files_changed: Vec::new(),
        }
    }
}

/// Query filters for searching decisions
#[derive(Debug, Clone, Default)]
pub struct DecisionQuery {
    /// Free-text search
    pub text: Option<String>,

    /// Filter by tags
    pub tags: Vec<String>,

    /// Filter by files
    pub files: Vec<String>,

    /// Filter by author
    pub author: Option<String>,

    /// Filter by status
    pub status: Option<DecisionStatus>,

    /// Date range - start
    pub since: Option<DateTime<Utc>>,

    /// Date range - end
    pub until: Option<DateTime<Utc>>,

    /// Maximum number of results
    pub limit: Option<usize>,
}

impl DecisionQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn with_file(mut self, file: String) -> Self {
        self.files.push(file);
        self
    }

    pub fn with_author(mut self, author: String) -> Self {
        self.author = Some(author);
        self
    }

    pub fn with_status(mut self, status: DecisionStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_date_range(
        mut self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Self {
        self.since = since;
        self.until = until;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_decision() {
        let decision = ArchitecturalDecision::new(
            "ADR-001".to_string(),
            "Test Decision".to_string(),
            "Test context".to_string(),
            "Test decision".to_string(),
            "Test rationale".to_string(),
            "test@example.com".to_string(),
        );

        assert_eq!(decision.id, "ADR-001");
        assert_eq!(decision.status, DecisionStatus::Proposed);
        assert!(decision.alternatives.is_empty());
    }

    #[test]
    fn test_add_alternative() {
        let mut decision = ArchitecturalDecision::new(
            "ADR-001".to_string(),
            "Test".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            "author".to_string(),
        );

        let alt = Alternative::new("Alternative approach".to_string())
            .with_pros(vec!["Pro 1".to_string()])
            .with_cons(vec!["Con 1".to_string()])
            .with_rejection_reason("Not suitable".to_string());

        decision.add_alternative(alt);
        assert_eq!(decision.alternatives.len(), 1);
    }

    #[test]
    fn test_decision_status() {
        let mut decision = ArchitecturalDecision::new(
            "ADR-001".to_string(),
            "Test".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            "author".to_string(),
        );

        assert!(decision.status.is_active());

        decision.accept();
        assert_eq!(decision.status, DecisionStatus::Accepted);
        assert!(decision.status.is_active());

        decision.supersede("ADR-002".to_string());
        assert!(!decision.status.is_active());
    }
}
