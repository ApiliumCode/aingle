pub mod git_integration;
pub mod models;
pub mod semantic_index;

use anyhow::Result;
use git_integration::GitIntegration;
use models::{Alternative, ArchitecturalDecision, DecisionQuery, DecisionStatus};
use semantic_index::SemanticIndex;
use std::fs;
use std::path::{Path, PathBuf};

/// Deep Context configuration
pub struct DeepContext {
    /// Root directory of the repository
    pub root_dir: PathBuf,

    /// Path to .deep-context directory
    pub context_dir: PathBuf,

    /// Semantic index
    pub index: SemanticIndex,

    /// Git integration
    pub git: GitIntegration,
}

impl DeepContext {
    /// Initialize Deep Context in a repository
    pub fn init(repo_path: PathBuf) -> Result<Self> {
        let context_dir = repo_path.join(".deep-context");

        if context_dir.exists() {
            anyhow::bail!("Deep Context already initialized in this repository");
        }

        // Create directory structure
        fs::create_dir_all(&context_dir)?;
        fs::create_dir_all(context_dir.join("db"))?;

        // Create config file
        let config_path = context_dir.join("config.toml");
        let default_config = r#"[deep_context]
version = "0.1.0"
initialized_at = ""

[storage]
backend = "sled"
db_path = ".deep-context/db"

[git]
auto_link_commits = true
install_hooks = true

[export]
default_format = "markdown"
output_dir = "docs/decisions"
"#;
        fs::write(&config_path, default_config)?;

        // Initialize semantic index
        let db_path = context_dir.join("db");
        let index = SemanticIndex::new(db_path)?;

        // Initialize git integration
        let git = GitIntegration::open(repo_path.clone())?;

        // Install git hooks
        git.install_hooks()?;

        let deep_context = Self {
            root_dir: repo_path,
            context_dir,
            index,
            git,
        };

        log::info!("Deep Context initialized successfully");

        Ok(deep_context)
    }

    /// Open an existing Deep Context repository
    pub fn open(repo_path: PathBuf) -> Result<Self> {
        let context_dir = repo_path.join(".deep-context");

        if !context_dir.exists() {
            anyhow::bail!(
                "Deep Context not initialized. Run 'deep-context init' first."
            );
        }

        let db_path = context_dir.join("db");
        let index = SemanticIndex::new(db_path)?;

        let git = GitIntegration::open(repo_path.clone())?;

        Ok(Self {
            root_dir: repo_path,
            context_dir,
            index,
            git,
        })
    }

    /// Generate a new decision ID
    pub fn next_decision_id(&self) -> Result<String> {
        let stats = self.index.statistics()?;
        Ok(format!("ADR-{:03}", stats.total_decisions + 1))
    }

    /// Capture a new decision
    pub fn capture_decision(
        &mut self,
        title: String,
        context: String,
        decision: String,
        rationale: String,
        alternatives: Vec<Alternative>,
        consequences: String,
        files: Vec<String>,
        tags: Vec<String>,
    ) -> Result<ArchitecturalDecision> {
        let (_, author_email) = self.git.current_user()?;

        let id = self.next_decision_id()?;

        let mut adr = ArchitecturalDecision::new(
            id,
            title,
            context,
            decision,
            rationale,
            author_email,
        );

        // Add alternatives
        for alt in alternatives {
            adr.add_alternative(alt);
        }

        // Set consequences
        adr.consequences = consequences;

        // Add files
        for file in files {
            adr.add_file(file);
        }

        // Add tags
        for tag in tags {
            adr.add_tag(tag);
        }

        // Accept by default
        adr.accept();

        // Store in index
        self.index.store_decision(adr.clone())?;

        log::info!("Captured decision {}: {}", adr.id, adr.title);

        Ok(adr)
    }

    /// Query decisions
    pub fn query_decisions(&self, query: &DecisionQuery) -> Result<Vec<ArchitecturalDecision>> {
        self.index.query(query)
    }

    /// Get a specific decision
    pub fn get_decision(&self, id: &str) -> Result<Option<ArchitecturalDecision>> {
        self.index.get_decision(id)
    }

    /// Get decisions for a file
    pub fn decisions_for_file(&self, file_path: &str) -> Result<Vec<ArchitecturalDecision>> {
        self.index.decisions_for_file(file_path)
    }

    /// Get all tags
    pub fn all_tags(&self) -> Result<Vec<String>> {
        self.index.all_tags()
    }

    /// Get decisions by tag
    pub fn decisions_by_tag(&self, tag: &str) -> Result<Vec<ArchitecturalDecision>> {
        self.index.decisions_by_tag(tag)
    }

    /// Export decisions to a directory
    pub fn export_markdown(&self, output_dir: &Path) -> Result<()> {
        fs::create_dir_all(output_dir)?;

        let all_decisions = self.index.query(&DecisionQuery::default())?;

        for decision in &all_decisions {
            let filename = format!("{}.md", decision.id);
            let file_path = output_dir.join(filename);

            let markdown = self.decision_to_markdown(decision);
            fs::write(&file_path, markdown)?;

            log::info!("Exported {} to {:?}", decision.id, file_path);
        }

        // Create index file
        let index_path = output_dir.join("README.md");
        let index_content = self.create_decisions_index(&all_decisions);
        fs::write(&index_path, index_content)?;

        Ok(())
    }

    /// Convert a decision to Markdown format
    fn decision_to_markdown(&self, decision: &ArchitecturalDecision) -> String {
        let mut md = String::new();

        md.push_str(&format!("# {}: {}\n\n", decision.id, decision.title));
        md.push_str(&format!("- **Status:** {}\n", decision.status.as_str()));
        md.push_str(&format!("- **Date:** {}\n", decision.timestamp.format("%Y-%m-%d")));
        md.push_str(&format!("- **Author:** {}\n", decision.author));

        if !decision.tags.is_empty() {
            md.push_str(&format!("- **Tags:** {}\n", decision.tags.join(", ")));
        }

        md.push_str("\n## Context\n\n");
        md.push_str(&decision.context);
        md.push_str("\n\n");

        md.push_str("## Decision\n\n");
        md.push_str(&decision.decision);
        md.push_str("\n\n");

        md.push_str("## Rationale\n\n");
        md.push_str(&decision.rationale);
        md.push_str("\n\n");

        if !decision.alternatives.is_empty() {
            md.push_str("## Alternatives Considered\n\n");
            for (i, alt) in decision.alternatives.iter().enumerate() {
                md.push_str(&format!("### Alternative {}: {}\n\n", i + 1, alt.description));

                if !alt.pros.is_empty() {
                    md.push_str("**Pros:**\n");
                    for pro in &alt.pros {
                        md.push_str(&format!("- {}\n", pro));
                    }
                    md.push('\n');
                }

                if !alt.cons.is_empty() {
                    md.push_str("**Cons:**\n");
                    for con in &alt.cons {
                        md.push_str(&format!("- {}\n", con));
                    }
                    md.push('\n');
                }

                if !alt.rejected_reason.is_empty() {
                    md.push_str(&format!("**Rejected because:** {}\n\n", alt.rejected_reason));
                }
            }
        }

        if !decision.consequences.is_empty() {
            md.push_str("## Consequences\n\n");
            md.push_str(&decision.consequences);
            md.push_str("\n\n");
        }

        if !decision.related_files.is_empty() {
            md.push_str("## Related Files\n\n");
            for file in &decision.related_files {
                md.push_str(&format!("- `{}`\n", file));
            }
            md.push_str("\n");
        }

        if !decision.related_decisions.is_empty() {
            md.push_str("## Related Decisions\n\n");
            for related_id in &decision.related_decisions {
                md.push_str(&format!("- [{}]({}.md)\n", related_id, related_id));
            }
            md.push_str("\n");
        }

        md
    }

    /// Create an index of all decisions
    fn create_decisions_index(&self, decisions: &[ArchitecturalDecision]) -> String {
        let mut md = String::new();

        md.push_str("# Architectural Decision Records\n\n");
        md.push_str(&format!(
            "This repository contains {} architectural decisions.\n\n",
            decisions.len()
        ));

        // Group by status
        let mut active = Vec::new();
        let mut proposed = Vec::new();
        let mut superseded = Vec::new();
        let mut deprecated = Vec::new();

        for decision in decisions {
            match decision.status {
                DecisionStatus::Accepted => active.push(decision),
                DecisionStatus::Proposed => proposed.push(decision),
                DecisionStatus::Superseded(_) => superseded.push(decision),
                DecisionStatus::Deprecated(_) => deprecated.push(decision),
            }
        }

        if !active.is_empty() {
            md.push_str("## Active Decisions\n\n");
            for decision in active {
                md.push_str(&format!(
                    "- [{}]({}.md): {} ({})\n",
                    decision.id,
                    decision.id,
                    decision.title,
                    decision.timestamp.format("%Y-%m-%d")
                ));
            }
            md.push_str("\n");
        }

        if !proposed.is_empty() {
            md.push_str("## Proposed Decisions\n\n");
            for decision in proposed {
                md.push_str(&format!(
                    "- [{}]({}.md): {} ({})\n",
                    decision.id,
                    decision.id,
                    decision.title,
                    decision.timestamp.format("%Y-%m-%d")
                ));
            }
            md.push_str("\n");
        }

        if !superseded.is_empty() {
            md.push_str("## Superseded Decisions\n\n");
            for decision in superseded {
                md.push_str(&format!(
                    "- [{}]({}.md): {} ({})\n",
                    decision.id,
                    decision.id,
                    decision.title,
                    decision.timestamp.format("%Y-%m-%d")
                ));
            }
            md.push_str("\n");
        }

        if !deprecated.is_empty() {
            md.push_str("## Deprecated Decisions\n\n");
            for decision in deprecated {
                md.push_str(&format!(
                    "- [{}]({}.md): {} ({})\n",
                    decision.id,
                    decision.id,
                    decision.title,
                    decision.timestamp.format("%Y-%m-%d")
                ));
            }
            md.push_str("\n");
        }

        md
    }

    /// Get statistics about the knowledge base
    pub fn statistics(&self) -> Result<semantic_index::IndexStats> {
        self.index.statistics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo_path = temp.path().to_path_buf();

        // Initialize a Git repo
        git2::Repository::init(&repo_path).unwrap();

        (temp, repo_path)
    }

    #[test]
    fn test_init_deep_context() {
        let (_temp, repo_path) = create_test_repo();

        let deep_context = DeepContext::init(repo_path.clone()).unwrap();

        assert!(deep_context.context_dir.exists());
        assert!(deep_context.context_dir.join("config.toml").exists());
    }

    #[test]
    fn test_capture_decision() {
        let (_temp, repo_path) = create_test_repo();

        let mut deep_context = DeepContext::init(repo_path).unwrap();

        let decision = deep_context
            .capture_decision(
                "Test Decision".to_string(),
                "Test context".to_string(),
                "Test decision".to_string(),
                "Test rationale".to_string(),
                vec![],
                "Test consequences".to_string(),
                vec![],
                vec!["test".to_string()],
            )
            .unwrap();

        assert_eq!(decision.id, "ADR-001");
        assert_eq!(decision.title, "Test Decision");
    }
}
