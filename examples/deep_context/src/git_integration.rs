use crate::models::{ArchitecturalDecision, LinkedCommit};
use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use git2::{Commit, Repository, Time};
use log::{debug, info};
use std::fs;
use std::path::{Path, PathBuf};

/// Git integration for Deep Context
pub struct GitIntegration {
    repo: Repository,
    repo_path: PathBuf,
}

impl GitIntegration {
    /// Open the Git repository at the given path
    pub fn open(path: PathBuf) -> Result<Self> {
        let repo = Repository::discover(&path)
            .context("Failed to find Git repository. Is this a Git repo?")?;

        let repo_path = repo
            .workdir()
            .context("Repository has no working directory")?
            .to_path_buf();

        Ok(Self { repo, repo_path })
    }

    /// Get the current Git user
    pub fn current_user(&self) -> Result<(String, String)> {
        let config = self.repo.config()?;

        let name = config
            .get_string("user.name")
            .unwrap_or_else(|_| "Unknown".to_string());

        let email = config
            .get_string("user.email")
            .unwrap_or_else(|_| "unknown@example.com".to_string());

        Ok((name, email))
    }

    /// Install Git hooks for Deep Context
    pub fn install_hooks(&self) -> Result<()> {
        let hooks_dir = self.repo_path.join(".git").join("hooks");
        fs::create_dir_all(&hooks_dir)?;

        // Install post-commit hook
        self.install_post_commit_hook(&hooks_dir)?;

        // Install prepare-commit-msg hook
        self.install_prepare_commit_msg_hook(&hooks_dir)?;

        info!("Git hooks installed successfully");
        Ok(())
    }

    /// Install post-commit hook
    fn install_post_commit_hook(&self, hooks_dir: &Path) -> Result<()> {
        let hook_path = hooks_dir.join("post-commit");
        let hook_content = r#"#!/bin/sh
# Deep Context post-commit hook
# Automatically links commits to architectural decisions

# Check if deep-context is available
if command -v deep-context >/dev/null 2>&1; then
    # Extract decision references from commit message
    COMMIT_MSG=$(git log -1 --pretty=%B)

    # Look for patterns like "ADR-001" or "Relates-To: ADR-001"
    echo "$COMMIT_MSG" | grep -oE "(ADR-[0-9]+)" | while read -r DECISION_ID; do
        echo "Linking commit to $DECISION_ID"
        deep-context link-commit "$DECISION_ID" HEAD 2>/dev/null || true
    done
else
    echo "deep-context not found in PATH. Skipping decision linking."
fi
"#;

        fs::write(&hook_path, hook_content)?;

        // Make executable (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms)?;
        }

        debug!("Installed post-commit hook at {:?}", hook_path);
        Ok(())
    }

    /// Install prepare-commit-msg hook
    fn install_prepare_commit_msg_hook(&self, hooks_dir: &Path) -> Result<()> {
        let hook_path = hooks_dir.join("prepare-commit-msg");
        let hook_content = r##"#!/bin/sh
# Deep Context prepare-commit-msg hook
# Suggests adding decision references to commit messages

COMMIT_MSG_FILE=$1
COMMIT_SOURCE=$2

# Only run for regular commits (not merges, amendments, etc.)
if [ -z "$COMMIT_SOURCE" ]; then
    if command -v deep-context >/dev/null 2>&1; then
        # Get list of changed files
        CHANGED_FILES=$(git diff --cached --name-only)

        # Query for relevant decisions
        DECISIONS=$(deep-context suggest-decisions $CHANGED_FILES 2>/dev/null || true)

        if [ -n "$DECISIONS" ]; then
            # Append suggestions to commit message template
            echo "" >> "$COMMIT_MSG_FILE"
            echo "# Suggested decision references:" >> "$COMMIT_MSG_FILE"
            echo "$DECISIONS" | while read -r line; do
                echo "# $line" >> "$COMMIT_MSG_FILE"
            done
            echo "# Use 'Relates-To: ADR-XXX' to link this commit" >> "$COMMIT_MSG_FILE"
        fi
    fi
fi
"##;

        fs::write(&hook_path, hook_content)?;

        // Make executable (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms)?;
        }

        debug!("Installed prepare-commit-msg hook at {:?}", hook_path);
        Ok(())
    }

    /// Link a commit to a decision
    pub fn link_commit_to_decision(
        &self,
        decision_id: &str,
        commit_ref: &str,
    ) -> Result<LinkedCommit> {
        let obj = self.repo.revparse_single(commit_ref)?;
        let commit = obj.peel_to_commit()?;

        let linked_commit = self.commit_to_linked(&commit, vec![decision_id.to_string()])?;

        info!(
            "Linked commit {} to decision {}",
            &linked_commit.commit_hash[..8],
            decision_id
        );

        Ok(linked_commit)
    }

    /// Get recent commits
    pub fn recent_commits(&self, limit: usize) -> Result<Vec<LinkedCommit>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;

        let mut commits = Vec::new();

        for oid in revwalk.take(limit) {
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;

            // Extract decision references from commit message
            let decision_ids = self.extract_decision_refs(commit.message().unwrap_or(""));

            let linked_commit = self.commit_to_linked(&commit, decision_ids)?;
            commits.push(linked_commit);
        }

        Ok(commits)
    }

    /// Extract decision references from text (e.g., "ADR-001")
    fn extract_decision_refs(&self, text: &str) -> Vec<String> {
        let re = regex::Regex::new(r"ADR-\d+").unwrap();
        re.find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Convert a git2::Commit to LinkedCommit
    fn commit_to_linked(
        &self,
        commit: &Commit,
        decision_ids: Vec<String>,
    ) -> Result<LinkedCommit> {
        let commit_hash = commit.id().to_string();
        let message = commit.message().unwrap_or("").to_string();
        let author = commit.author();
        let author_str = format!(
            "{} <{}>",
            author.name().unwrap_or("Unknown"),
            author.email().unwrap_or("unknown@example.com")
        );
        let timestamp = git_time_to_chrono(&author.when());

        // Get changed files
        let files_changed = self.get_changed_files(commit)?;

        let mut linked_commit = LinkedCommit::new(commit_hash, message, author_str, timestamp);
        linked_commit.decision_ids = decision_ids;
        linked_commit.files_changed = files_changed;

        Ok(linked_commit)
    }

    /// Get files changed in a commit
    fn get_changed_files(&self, commit: &Commit) -> Result<Vec<String>> {
        let mut files = Vec::new();

        let tree = commit.tree()?;

        if commit.parent_count() == 0 {
            // First commit - all files are new
            tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
                if let Some(name) = entry.name() {
                    files.push(name.to_string());
                }
                git2::TreeWalkResult::Ok
            })?;
        } else {
            // Compare with parent
            let parent = commit.parent(0)?;
            let parent_tree = parent.tree()?;

            let diff = self
                .repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;

            for delta in diff.deltas() {
                if let Some(path) = delta.new_file().path() {
                    if let Some(path_str) = path.to_str() {
                        files.push(path_str.to_string());
                    }
                }
            }
        }

        Ok(files)
    }

    /// Get commits that affected a specific file
    pub fn commits_for_file(&self, file_path: &str) -> Result<Vec<LinkedCommit>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;

        let mut commits = Vec::new();

        for oid in revwalk {
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;

            let files_changed = self.get_changed_files(&commit)?;

            if files_changed.iter().any(|f| f.contains(file_path)) {
                let decision_ids = self.extract_decision_refs(commit.message().unwrap_or(""));
                let linked_commit = self.commit_to_linked(&commit, decision_ids)?;
                commits.push(linked_commit);
            }
        }

        Ok(commits)
    }

    /// Get the timeline of decisions based on commits
    pub fn decision_timeline(
        &self,
        decisions: &[ArchitecturalDecision],
    ) -> Result<Vec<TimelineEvent>> {
        let mut events: Vec<TimelineEvent> = decisions
            .iter()
            .map(|d| TimelineEvent {
                timestamp: d.timestamp,
                event_type: TimelineEventType::DecisionMade,
                title: d.title.clone(),
                decision_id: Some(d.id.clone()),
                commit_hash: None,
                author: d.author.clone(),
            })
            .collect();

        // Add commit events
        for decision in decisions {
            let decision_id = &decision.id;

            // Find commits that reference this decision
            let mut revwalk = self.repo.revwalk()?;
            revwalk.push_head()?;

            for oid in revwalk {
                let oid = oid?;
                let commit = self.repo.find_commit(oid)?;
                let message = commit.message().unwrap_or("");

                if message.contains(decision_id) {
                    let author = commit.author();
                    let timestamp = git_time_to_chrono(&author.when());

                    events.push(TimelineEvent {
                        timestamp,
                        event_type: TimelineEventType::DecisionImplemented,
                        title: commit.summary().unwrap_or("").to_string(),
                        decision_id: Some(decision_id.clone()),
                        commit_hash: Some(commit.id().to_string()),
                        author: format!(
                            "{} <{}>",
                            author.name().unwrap_or("Unknown"),
                            author.email().unwrap_or("unknown@example.com")
                        ),
                    });
                }
            }
        }

        // Sort by timestamp
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(events)
    }
}

/// Convert git2::Time to chrono::DateTime
fn git_time_to_chrono(time: &Time) -> DateTime<Utc> {
    Utc.timestamp_opt(time.seconds(), 0)
        .single()
        .unwrap_or_else(Utc::now)
}

/// Timeline event
#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: TimelineEventType,
    pub title: String,
    pub decision_id: Option<String>,
    pub commit_hash: Option<String>,
    pub author: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEventType {
    DecisionMade,
    DecisionImplemented,
    DecisionSuperseded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_decision_refs() {
        let git = GitIntegration {
            repo: unsafe { std::mem::zeroed() }, // Mock
            repo_path: PathBuf::new(),
        };

        let text = "This commit implements ADR-001 and relates to ADR-042";
        let refs = git.extract_decision_refs(text);

        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"ADR-001".to_string()));
        assert!(refs.contains(&"ADR-042".to_string()));
    }
}
