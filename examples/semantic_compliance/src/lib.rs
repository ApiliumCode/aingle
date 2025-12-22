//! Semantic Compliance Library
//!
//! A comprehensive AML/KYC compliance system built on AIngle, providing:
//! - Real-time sanctions list monitoring
//! - Semantic entity matching
//! - Risk scoring and assessment
//! - Graph-based relationship analysis
//! - Immutable audit trails
//!
//! ## Example Usage
//!
//! ```no_run
//! use semantic_compliance::*;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create compliance system
//! let mut system = ComplianceSystem::new(ComplianceConfig::default());
//!
//! // Add an entity to monitor
//! let entity = Entity {
//!     id: "CUST-001".to_string(),
//!     name: "Acme Corp".to_string(),
//!     // ... other fields
//!     # entity_type: EntityType::Company,
//!     # aliases: vec![],
//!     # identifiers: vec![],
//!     # relationships: vec![],
//!     # risk_score: 0.0,
//!     # risk_level: RiskLevel::Low,
//!     # last_checked: chrono::Utc::now(),
//!     # created_at: chrono::Utc::now(),
//!     # metadata: std::collections::HashMap::new(),
//! };
//!
//! system.add_entity(entity).await?;
//!
//! // Check against sanctions lists
//! let matches = system.check_entity("CUST-001").await?;
//!
//! // Calculate risk score
//! let assessment = system.assess_risk("CUST-001").await?;
//!
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod audit_trail;
pub mod graph_analysis;
pub mod models;
pub mod risk_scoring;
pub mod sanctions_monitor;

// Re-export main types for convenience
pub use audit_trail::{AuditTrail, CheckResult, ExportFormat, VerificationResult};
pub use graph_analysis::{
    ClusterAlgorithm, EntityCluster, GraphAnalyzer, GraphStatistics, OwnershipTree, Path,
};
pub use models::*;
pub use risk_scoring::{RiskEngine, RiskExplanation, RiskWeights};
pub use sanctions_monitor::{SanctionMatch, SanctionsMonitor, SanctionsStatistics, SemanticMatcher};

use anyhow::Result;
use std::collections::HashMap;
use tracing::info;

// ============================================================================
// Compliance System
// ============================================================================

/// Main compliance system that integrates all components
pub struct ComplianceSystem {
    /// Configuration
    config: ComplianceConfig,

    /// Sanctions monitoring
    sanctions_monitor: SanctionsMonitor,

    /// Risk scoring engine
    risk_engine: RiskEngine,

    /// Graph analyzer
    graph_analyzer: GraphAnalyzer,

    /// Audit trail
    audit_trail: AuditTrail,

    /// Entity storage
    entities: HashMap<String, Entity>,

    /// Active alerts
    alerts: HashMap<String, ComplianceAlert>,
}

impl ComplianceSystem {
    /// Create a new compliance system
    pub fn new(config: ComplianceConfig) -> Self {
        info!("Initializing compliance system");

        let sanctions_monitor = SanctionsMonitor::new(config.matching.clone());
        let risk_engine = RiskEngine::new(config.risk_scoring.clone());
        let graph_analyzer = GraphAnalyzer::new();
        let audit_trail = AuditTrail::new();

        Self {
            config,
            sanctions_monitor,
            risk_engine,
            graph_analyzer,
            audit_trail,
            entities: HashMap::new(),
            alerts: HashMap::new(),
        }
    }

    /// Add an entity to the system
    pub async fn add_entity(&mut self, entity: Entity) -> Result<()> {
        info!("Adding entity: {} ({})", entity.name, entity.id);

        // Add to graph
        self.graph_analyzer.add_entity(entity.clone())?;

        // Store entity
        self.entities.insert(entity.id.clone(), entity);

        Ok(())
    }

    /// Get an entity by ID
    pub fn get_entity(&self, entity_id: &str) -> Option<&Entity> {
        self.entities.get(entity_id)
    }

    /// Check an entity against sanctions lists
    pub async fn check_entity(&mut self, entity_id: &str) -> Result<Vec<SanctionMatch>> {
        let entity = self.entities.get(entity_id)
            .ok_or_else(|| anyhow::anyhow!("Entity not found"))?.clone();

        let matches = self.sanctions_monitor.check_entity(&entity).await?;

        // Record in audit trail
        let result = audit_trail::CheckResult {
            matches: matches.iter().map(|m| m.entry.id.clone()).collect(),
            lists_checked: matches.iter()
                .map(|m| m.source.as_str().to_string())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect(),
            timestamp: chrono::Utc::now(),
        };

        self.audit_trail.record_check(entity_id, "system", result)?;

        // Create alerts for high-confidence matches
        for m in &matches {
            if m.confidence >= self.config.matching.critical_threshold {
                self.create_alert(&entity, m)?;
            }
        }

        Ok(matches)
    }

    /// Assess risk for an entity
    pub async fn assess_risk(&mut self, entity_id: &str) -> Result<RiskAssessment> {
        let entity = self.entities.get(entity_id)
            .ok_or_else(|| anyhow::anyhow!("Entity not found"))?;

        let assessment = self.risk_engine.calculate_risk(entity)?;

        // Record in audit trail
        self.audit_trail.record_risk_assessment(&assessment, "system")?;

        // Update entity risk score
        if let Some(entity) = self.entities.get_mut(entity_id) {
            entity.risk_score = assessment.overall_score;
            entity.risk_level = assessment.risk_level.clone();
        }

        Ok(assessment)
    }

    /// Find connections between entities
    pub fn find_connections(&self, entity_a: &str, entity_b: &str) -> Result<Vec<Path>> {
        self.graph_analyzer.find_connections(entity_a, entity_b)
    }

    /// Trace ownership structure
    pub fn trace_ownership(&self, entity_id: &str, max_depth: usize) -> Result<OwnershipTree> {
        self.graph_analyzer.trace_ownership(entity_id, max_depth)
    }

    /// Detect entity clusters
    pub fn detect_clusters(&self, algorithm: ClusterAlgorithm) -> Result<Vec<EntityCluster>> {
        self.graph_analyzer.detect_clusters(algorithm)
    }

    /// Get all active alerts
    pub fn get_alerts(&self, severity: Option<AlertSeverity>) -> Vec<&ComplianceAlert> {
        self.alerts.values()
            .filter(|alert| {
                if let Some(sev) = &severity {
                    &alert.severity == sev
                } else {
                    true
                }
            })
            .collect()
    }

    /// Resolve an alert
    pub fn resolve_alert(
        &mut self,
        alert_id: &str,
        resolution: AlertStatus,
        notes: &str,
        user_id: &str,
    ) -> Result<()> {
        let alert = self.alerts.get_mut(alert_id)
            .ok_or_else(|| anyhow::anyhow!("Alert not found"))?;

        alert.status = resolution.clone();
        alert.resolution_notes = Some(notes.to_string());
        alert.resolved_at = Some(chrono::Utc::now());

        // Record in audit trail
        self.audit_trail.record_alert_resolved(
            alert_id,
            &alert.entity_id,
            user_id,
            resolution,
            notes,
        )?;

        Ok(())
    }

    /// Generate compliance report
    pub fn generate_report(&self, period: ReportingPeriod) -> Result<AuditReport> {
        self.audit_trail.generate_report(period)
    }

    /// Verify audit trail integrity
    pub fn verify_audit_integrity(&self) -> VerificationResult {
        self.audit_trail.verify_integrity()
    }

    /// Get system statistics
    pub async fn get_statistics(&self) -> ComplianceStatistics {
        let sanctions_stats = self.sanctions_monitor.get_statistics().await;
        let graph_stats = self.graph_analyzer.get_statistics();

        let active_alerts = self.alerts.values()
            .filter(|a| !matches!(a.status, AlertStatus::Cleared | AlertStatus::FalsePositive))
            .count();

        ComplianceStatistics {
            total_entities: self.entities.len(),
            high_risk_entities: self.entities.values()
                .filter(|e| matches!(e.risk_level, RiskLevel::High | RiskLevel::Critical))
                .count(),
            active_alerts,
            sanctions_lists_loaded: sanctions_stats.total_lists,
            total_sanctions_entries: sanctions_stats.total_entries,
            graph_connections: graph_stats.total_relationships,
        }
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    fn create_alert(&mut self, entity: &Entity, match_info: &SanctionMatch) -> Result<()> {
        let alert_id = format!("ALERT-{}-{}", chrono::Utc::now().timestamp(), uuid::Uuid::new_v4());

        let severity = if match_info.confidence >= 0.95 {
            AlertSeverity::Critical
        } else if match_info.confidence >= 0.85 {
            AlertSeverity::High
        } else {
            AlertSeverity::Medium
        };

        let alert = ComplianceAlert {
            id: alert_id.clone(),
            severity,
            entity_id: entity.id.clone(),
            entity_name: entity.name.clone(),
            reason: format!(
                "Sanctions match detected: {} (confidence: {:.2})",
                match_info.entry.names.first().unwrap_or(&"Unknown".to_string()),
                match_info.confidence
            ),
            matched_list: match_info.source.clone(),
            matched_entry: match_info.entry.clone(),
            confidence: match_info.confidence,
            match_details: MatchDetails {
                matched_field: match_info.matched_field.clone(),
                entity_value: match_info.entity_value.clone(),
                list_value: match_info.list_value.clone(),
                algorithm: match_info.algorithm.clone(),
                edit_distance: None,
                context: HashMap::new(),
            },
            created_at: chrono::Utc::now(),
            status: AlertStatus::New,
            assigned_to: None,
            resolution_notes: None,
            resolved_at: None,
        };

        // Record in audit trail
        self.audit_trail.record_alert_created(&alert, "system")?;

        // Store alert
        self.alerts.insert(alert_id, alert);

        Ok(())
    }
}

impl Default for ComplianceSystem {
    fn default() -> Self {
        Self::new(ComplianceConfig::default())
    }
}

// ============================================================================
// Default Implementations
// ============================================================================

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            sources: vec![],
            matching: MatchingConfig::default(),
            risk_scoring: RiskScoringConfig::default(),
            alerts: AlertConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            default_threshold: 0.85,
            critical_threshold: 0.95,
            phonetic_matching: true,
            transliteration: false,
            max_edit_distance: 3,
        }
    }
}

impl Default for RiskScoringConfig {
    fn default() -> Self {
        let mut weights = HashMap::new();
        weights.insert("sanctions_match".to_string(), 10.0);
        weights.insert("pep_status".to_string(), 7.0);
        weights.insert("adverse_media".to_string(), 5.0);
        weights.insert("high_risk_jurisdiction".to_string(), 6.0);
        weights.insert("unusual_transactions".to_string(), 8.0);
        weights.insert("hidden_ownership".to_string(), 7.5);

        let mut thresholds = HashMap::new();
        thresholds.insert("critical".to_string(), 9.0);
        thresholds.insert("high".to_string(), 7.0);
        thresholds.insert("medium".to_string(), 5.0);
        thresholds.insert("low".to_string(), 3.0);

        Self {
            weights,
            thresholds,
            enable_ml: false,
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            email: vec![],
            slack_webhook: None,
            sms: vec![],
            escalation: EscalationConfig::default(),
        }
    }
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            critical_immediate: true,
            critical_notify: vec![],
            high_within_minutes: 15,
            medium_within_hours: 4,
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            retention_years: 7,
            allowed_formats: vec!["json".to_string(), "xml".to_string(), "pdf".to_string()],
            sign_reports: true,
            key_id: None,
        }
    }
}

// ============================================================================
// Statistics
// ============================================================================

/// Overall compliance system statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplianceStatistics {
    /// Total entities monitored
    pub total_entities: usize,

    /// High-risk entities
    pub high_risk_entities: usize,

    /// Active alerts
    pub active_alerts: usize,

    /// Sanctions lists loaded
    pub sanctions_lists_loaded: usize,

    /// Total sanctions entries
    pub total_sanctions_entries: usize,

    /// Graph connections
    pub graph_connections: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compliance_system_creation() {
        let system = ComplianceSystem::new(ComplianceConfig::default());
        let stats = system.get_statistics().await;

        assert_eq!(stats.total_entities, 0);
    }

    #[tokio::test]
    async fn test_add_entity() {
        let mut system = ComplianceSystem::new(ComplianceConfig::default());

        let entity = Entity {
            id: "TEST-001".to_string(),
            name: "Test Entity".to_string(),
            entity_type: EntityType::Company,
            aliases: vec![],
            identifiers: vec![],
            relationships: vec![],
            risk_score: 0.0,
            risk_level: RiskLevel::Low,
            last_checked: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        };

        system.add_entity(entity).await.unwrap();

        let stats = system.get_statistics().await;
        assert_eq!(stats.total_entities, 1);
    }
}
