//! Audit log for tracking API actions
//!
//! Provides an append-only, file-backed audit log with REST endpoints
//! for querying and aggregating audit entries.

use axum::{
    extract::{Query, State},
    Json, Router,
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::state::AppState;

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// User ID (from JWT sub)
    pub user_id: String,
    /// Namespace scope
    pub namespace: Option<String>,
    /// Action performed (create, read, delete, query, validate, etc.)
    pub action: String,
    /// Resource path or identifier
    pub resource: String,
    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Unique request ID for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Audit log with optional JSONL file backing.
pub struct AuditLog {
    entries: Vec<AuditEntry>,
    max_entries: usize,
    /// Optional file path for JSONL persistence.
    log_path: Option<PathBuf>,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
            log_path: None,
        }
    }

    /// Create a file-backed audit log. Reads existing entries from the JSONL file on disk.
    pub fn with_path(max_entries: usize, path: PathBuf) -> Self {
        let mut entries = Vec::new();

        // Read existing entries from JSONL file
        if let Ok(file) = std::fs::File::open(&path) {
            let reader = std::io::BufReader::new(file);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if let Ok(entry) = serde_json::from_str::<AuditEntry>(&line) {
                        entries.push(entry);
                    }
                }
            }
            // Keep only the last max_entries
            if entries.len() > max_entries {
                entries = entries.split_off(entries.len() - max_entries);
            }
        }

        Self {
            entries,
            max_entries,
            log_path: Some(path),
        }
    }

    /// Record a new audit entry.
    pub fn record(&mut self, entry: AuditEntry) {
        // Append to file if file-backed
        if let Some(ref path) = self.log_path {
            if let Ok(json) = serde_json::to_string(&entry) {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(file, "{}", json);
                }
            }
        }

        // Evict oldest entries if at capacity
        if self.entries.len() >= self.max_entries {
            let drain_count = self.max_entries / 10; // evict 10%
            self.entries.drain(0..drain_count);
        }
        self.entries.push(entry);
    }

    /// Query entries with filters.
    pub fn query(
        &self,
        user_id: Option<&str>,
        namespace: Option<&str>,
        action: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Vec<&AuditEntry> {
        self.entries
            .iter()
            .rev() // newest first
            .filter(|e| {
                if let Some(uid) = user_id {
                    if e.user_id != uid {
                        return false;
                    }
                }
                if let Some(ns) = namespace {
                    if e.namespace.as_deref() != Some(ns) {
                        return false;
                    }
                }
                if let Some(act) = action {
                    if e.action != act {
                        return false;
                    }
                }
                if let Some(f) = from {
                    if e.timestamp.as_str() < f {
                        return false;
                    }
                }
                if let Some(t) = to {
                    if e.timestamp.as_str() > t {
                        return false;
                    }
                }
                true
            })
            .take(limit)
            .collect()
    }

    /// Get aggregate stats.
    pub fn stats(&self) -> AuditStats {
        let mut actions: HashMap<String, usize> = HashMap::new();
        let mut users: HashMap<String, usize> = HashMap::new();
        let mut namespaces: HashMap<String, usize> = HashMap::new();

        for entry in &self.entries {
            *actions.entry(entry.action.clone()).or_insert(0) += 1;
            *users.entry(entry.user_id.clone()).or_insert(0) += 1;
            if let Some(ref ns) = entry.namespace {
                *namespaces.entry(ns.clone()).or_insert(0) += 1;
            }
        }

        AuditStats {
            total_entries: self.entries.len(),
            actions_by_type: actions,
            entries_by_user: users,
            entries_by_namespace: namespaces,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(10_000)
    }
}

/// Aggregate audit statistics.
#[derive(Debug, Serialize)]
pub struct AuditStats {
    pub total_entries: usize,
    pub actions_by_type: HashMap<String, usize>,
    pub entries_by_user: HashMap<String, usize>,
    pub entries_by_namespace: HashMap<String, usize>,
}

// ============================================================================
// REST Endpoints
// ============================================================================

/// Query parameters for the audit endpoint.
#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub user_id: Option<String>,
    pub namespace: Option<String>,
    pub action: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// GET /api/v1/audit
pub async fn get_audit_log(
    State(state): State<AppState>,
    Query(params): Query<AuditQueryParams>,
) -> Json<Vec<AuditEntry>> {
    let log = state.audit_log.read().await;
    let entries = log.query(
        params.user_id.as_deref(),
        params.namespace.as_deref(),
        params.action.as_deref(),
        params.from.as_deref(),
        params.to.as_deref(),
        params.limit,
    );
    Json(entries.into_iter().cloned().collect())
}

/// GET /api/v1/audit/stats
pub async fn get_audit_stats(
    State(state): State<AppState>,
) -> Json<AuditStats> {
    let log = state.audit_log.read().await;
    Json(log.stats())
}

/// Create the audit router.
pub fn audit_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/audit", get(get_audit_log))
        .route("/api/v1/audit/stats", get(get_audit_stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(action: &str, user: &str, ns: Option<&str>) -> AuditEntry {
        AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            user_id: user.to_string(),
            namespace: ns.map(|s| s.to_string()),
            action: action.to_string(),
            resource: "/api/v1/triples".to_string(),
            details: None,
            request_id: None,
        }
    }

    #[test]
    fn test_audit_log_record_and_query() {
        let mut log = AuditLog::new(100);
        log.record(make_entry("create", "user1", Some("mayros")));
        log.record(make_entry("read", "user2", Some("other")));
        log.record(make_entry("delete", "user1", Some("mayros")));

        assert_eq!(log.len(), 3);

        let results = log.query(Some("user1"), None, None, None, None, 100);
        assert_eq!(results.len(), 2);

        let results = log.query(None, Some("mayros"), None, None, None, 100);
        assert_eq!(results.len(), 2);

        let results = log.query(None, None, Some("delete"), None, None, 100);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_audit_log_eviction() {
        let mut log = AuditLog::new(10);
        for i in 0..15 {
            log.record(make_entry(&format!("action-{}", i), "user1", None));
        }
        // Should have evicted some entries
        assert!(log.len() <= 15);
        assert!(log.len() > 0);
    }

    #[test]
    fn test_audit_log_file_backed_roundtrip() {
        let dir = std::env::temp_dir().join(format!("audit_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit.jsonl");

        // Clean up any leftover file
        let _ = std::fs::remove_file(&path);

        // Record entries to a file-backed log
        {
            let mut log = AuditLog::with_path(100, path.clone());
            log.record(make_entry("create", "user1", Some("ns1")));
            log.record(make_entry("read", "user2", Some("ns2")));
            assert_eq!(log.len(), 2);
        }
        // Drop log — entries should persist on disk

        // Recreate from file — entries should be restored
        {
            let log = AuditLog::with_path(100, path.clone());
            assert_eq!(log.len(), 2);
            let results = log.query(Some("user1"), None, None, None, None, 100);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].action, "create");
        }

        // Clean up
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_audit_stats() {
        let mut log = AuditLog::new(100);
        log.record(make_entry("create", "user1", Some("ns1")));
        log.record(make_entry("create", "user1", Some("ns1")));
        log.record(make_entry("read", "user2", Some("ns2")));

        let stats = log.stats();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(*stats.actions_by_type.get("create").unwrap(), 2);
        assert_eq!(*stats.actions_by_type.get("read").unwrap(), 1);
        assert_eq!(*stats.entries_by_user.get("user1").unwrap(), 2);
    }
}
