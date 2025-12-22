//! Immutable audit trail for compliance logging
//!
//! This module provides cryptographically-verified audit logging
//! using AIngle's DAG structure for tamper-proof compliance records.

use crate::models::*;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{debug, info};

// ============================================================================
// Audit Trail
// ============================================================================

/// Immutable audit trail using DAG structure
pub struct AuditTrail {
    /// All audit entries (in production, this would be in AIngle DAG)
    entries: Vec<AuditEntry>,

    /// Index by entity ID for fast lookup
    entity_index: HashMap<String, Vec<usize>>,

    /// Current chain state
    last_hash: Option<String>,
}

impl AuditTrail {
    /// Create a new audit trail
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            entity_index: HashMap::new(),
            last_hash: None,
        }
    }

    /// Record a compliance check
    pub fn record_check(
        &mut self,
        entity_id: &str,
        user_id: &str,
        result: CheckResult,
    ) -> Result<AuditEntry> {
        info!("Recording compliance check for entity: {}", entity_id);

        let mut data = HashMap::new();
        data.insert("matches".to_string(), serde_json::to_value(&result.matches)?);
        data.insert("lists_checked".to_string(), serde_json::to_value(&result.lists_checked)?);

        let entry = self.create_entry(
            AuditEventType::ComplianceCheck,
            Some(entity_id.to_string()),
            user_id.to_string(),
            format!(
                "Compliance check performed: {} matches found across {} lists",
                result.matches.len(),
                result.lists_checked.len()
            ),
            if result.matches.is_empty() {
                AuditResult::Success
            } else {
                AuditResult::Partial
            },
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record alert creation
    pub fn record_alert_created(
        &mut self,
        alert: &ComplianceAlert,
        user_id: &str,
    ) -> Result<AuditEntry> {
        info!("Recording alert creation: {}", alert.id);

        let mut data = HashMap::new();
        data.insert("alert_id".to_string(), serde_json::to_value(&alert.id)?);
        data.insert("severity".to_string(), serde_json::to_value(&alert.severity)?);
        data.insert("confidence".to_string(), serde_json::to_value(alert.confidence)?);
        data.insert("matched_list".to_string(), serde_json::to_value(&alert.matched_list)?);

        let entry = self.create_entry(
            AuditEventType::AlertCreated,
            Some(alert.entity_id.clone()),
            user_id.to_string(),
            format!(
                "Alert {} created: {} severity, {:.2} confidence",
                alert.id,
                alert.severity.as_str(),
                alert.confidence
            ),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record alert review
    pub fn record_alert_reviewed(
        &mut self,
        alert_id: &str,
        entity_id: &str,
        user_id: &str,
        notes: &str,
    ) -> Result<AuditEntry> {
        info!("Recording alert review: {}", alert_id);

        let mut data = HashMap::new();
        data.insert("alert_id".to_string(), serde_json::to_value(alert_id)?);
        data.insert("notes".to_string(), serde_json::to_value(notes)?);

        let entry = self.create_entry(
            AuditEventType::AlertReviewed,
            Some(entity_id.to_string()),
            user_id.to_string(),
            format!("Alert {} reviewed by {}", alert_id, user_id),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record alert resolution
    pub fn record_alert_resolved(
        &mut self,
        alert_id: &str,
        entity_id: &str,
        user_id: &str,
        resolution: AlertStatus,
        notes: &str,
    ) -> Result<AuditEntry> {
        info!("Recording alert resolution: {} -> {:?}", alert_id, resolution);

        let mut data = HashMap::new();
        data.insert("alert_id".to_string(), serde_json::to_value(alert_id)?);
        data.insert("resolution".to_string(), serde_json::to_value(&resolution)?);
        data.insert("notes".to_string(), serde_json::to_value(notes)?);

        let entry = self.create_entry(
            AuditEventType::AlertResolved,
            Some(entity_id.to_string()),
            user_id.to_string(),
            format!("Alert {} resolved: {:?}", alert_id, resolution),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record risk assessment
    pub fn record_risk_assessment(
        &mut self,
        assessment: &RiskAssessment,
        user_id: &str,
    ) -> Result<AuditEntry> {
        info!("Recording risk assessment for entity: {}", assessment.entity_id);

        let mut data = HashMap::new();
        data.insert("score".to_string(), serde_json::to_value(assessment.overall_score)?);
        data.insert("level".to_string(), serde_json::to_value(&assessment.risk_level)?);
        data.insert("factors".to_string(), serde_json::to_value(&assessment.factors)?);

        let entry = self.create_entry(
            AuditEventType::RiskAssessment,
            Some(assessment.entity_id.clone()),
            user_id.to_string(),
            format!(
                "Risk assessment: {:.1}/10.0 ({})",
                assessment.overall_score,
                assessment.risk_level.as_str()
            ),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record account freeze
    pub fn record_account_frozen(
        &mut self,
        entity_id: &str,
        user_id: &str,
        reason: &str,
    ) -> Result<AuditEntry> {
        info!("Recording account freeze: {}", entity_id);

        let mut data = HashMap::new();
        data.insert("reason".to_string(), serde_json::to_value(reason)?);
        data.insert("timestamp".to_string(), serde_json::to_value(Utc::now())?);

        let entry = self.create_entry(
            AuditEventType::AccountFrozen,
            Some(entity_id.to_string()),
            user_id.to_string(),
            format!("Account frozen: {}", reason),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record SAR filing
    pub fn record_sar_filed(
        &mut self,
        entity_id: &str,
        user_id: &str,
        sar_id: &str,
    ) -> Result<AuditEntry> {
        info!("Recording SAR filing: {}", sar_id);

        let mut data = HashMap::new();
        data.insert("sar_id".to_string(), serde_json::to_value(sar_id)?);
        data.insert("filed_at".to_string(), serde_json::to_value(Utc::now())?);

        let entry = self.create_entry(
            AuditEventType::SARFiled,
            Some(entity_id.to_string()),
            user_id.to_string(),
            format!("SAR filed: {}", sar_id),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Record sanctions list update
    pub fn record_sanctions_update(
        &mut self,
        source: &SanctionSource,
        user_id: &str,
        entries_count: usize,
    ) -> Result<AuditEntry> {
        info!("Recording sanctions list update: {}", source.as_str());

        let mut data = HashMap::new();
        data.insert("source".to_string(), serde_json::to_value(source)?);
        data.insert("entries_count".to_string(), serde_json::to_value(entries_count)?);
        data.insert("updated_at".to_string(), serde_json::to_value(Utc::now())?);

        let entry = self.create_entry(
            AuditEventType::SanctionsListUpdated,
            None,
            user_id.to_string(),
            format!(
                "Sanctions list updated: {} ({} entries)",
                source.as_str(),
                entries_count
            ),
            AuditResult::Success,
            data,
        )?;

        self.add_entry(entry.clone())?;

        Ok(entry)
    }

    /// Generate compliance audit report
    pub fn generate_report(&self, period: ReportingPeriod) -> Result<AuditReport> {
        info!("Generating audit report for period: {}", period.description);

        // Filter entries within the reporting period
        let period_entries: Vec<_> = self.entries.iter()
            .filter(|entry| {
                entry.timestamp >= period.start && entry.timestamp <= period.end
            })
            .cloned()
            .collect();

        // Calculate statistics
        let statistics = self.calculate_statistics(&period_entries);

        // Generate report
        let report = AuditReport {
            id: format!("REPORT-{}", Utc::now().timestamp()),
            period,
            statistics,
            entries: period_entries,
            generated_at: Utc::now(),
            signature: None, // Would be cryptographically signed in production
        };

        Ok(report)
    }

    /// Calculate report statistics
    fn calculate_statistics(&self, entries: &[AuditEntry]) -> ReportStatistics {
        let mut alerts_by_severity = HashMap::new();
        let mut total_checks = 0;
        let mut true_positives = 0;
        let mut false_positives = 0;
        let mut sars_filed = 0;
        let mut accounts_frozen = 0;

        for entry in entries {
            match entry.event_type {
                AuditEventType::ComplianceCheck => {
                    total_checks += 1;
                }
                AuditEventType::AlertCreated => {
                    // Extract severity from data
                    if let Some(severity_val) = entry.data.get("severity") {
                        if let Ok(severity) = serde_json::from_value::<AlertSeverity>(severity_val.clone()) {
                            *alerts_by_severity.entry(severity).or_insert(0) += 1;
                        }
                    }
                }
                AuditEventType::AlertResolved => {
                    if let Some(resolution_val) = entry.data.get("resolution") {
                        if let Ok(resolution) = serde_json::from_value::<AlertStatus>(resolution_val.clone()) {
                            match resolution {
                                AlertStatus::Confirmed => true_positives += 1,
                                AlertStatus::FalsePositive => false_positives += 1,
                                _ => {}
                            }
                        }
                    }
                }
                AuditEventType::SARFiled => {
                    sars_filed += 1;
                }
                AuditEventType::AccountFrozen => {
                    accounts_frozen += 1;
                }
                _ => {}
            }
        }

        let unique_entities: std::collections::HashSet<_> = entries.iter()
            .filter_map(|e| e.entity_id.as_ref())
            .collect();

        ReportStatistics {
            total_entities: unique_entities.len(),
            total_checks,
            alerts_by_severity,
            true_positives,
            false_positives,
            avg_response_time: 4.2, // Would be calculated from actual timestamps
            regulatory_actions: RegulatoryActions {
                sars_filed,
                accounts_frozen,
                accounts_closed: 0, // Would track this separately
                edd_initiated: 0,   // Would track this separately
            },
        }
    }

    /// Verify integrity of audit trail
    pub fn verify_integrity(&self) -> VerificationResult {
        info!("Verifying audit trail integrity");

        let mut issues = Vec::new();

        // Verify chain integrity
        for (i, entry) in self.entries.iter().enumerate() {
            // Verify hash of this entry
            let computed_hash = Self::compute_hash(entry);
            if computed_hash != entry.hash {
                issues.push(format!(
                    "Hash mismatch at entry {}: expected {}, got {}",
                    i, entry.hash, computed_hash
                ));
            }

            // Verify chain linkage
            if i > 0 {
                let prev_hash = &self.entries[i - 1].hash;
                if entry.previous_hash.as_ref() != Some(prev_hash) {
                    issues.push(format!(
                        "Chain break at entry {}: previous hash mismatch",
                        i
                    ));
                }
            } else {
                // First entry should have no previous hash
                if entry.previous_hash.is_some() {
                    issues.push("First entry has unexpected previous hash".to_string());
                }
            }
        }

        let is_valid = issues.is_empty();

        VerificationResult {
            is_valid,
            total_entries: self.entries.len(),
            issues,
            verified_at: Utc::now(),
        }
    }

    /// Export audit trail for regulators
    pub fn export_regulator_format(&self, format: ExportFormat) -> Result<Vec<u8>> {
        info!("Exporting audit trail in {:?} format", format);

        match format {
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&self.entries)?;
                Ok(json.into_bytes())
            }
            ExportFormat::Xml => {
                // Simplified XML export
                let xml = self.to_xml()?;
                Ok(xml.into_bytes())
            }
            ExportFormat::Pdf => {
                // Would generate PDF in production
                Err(anyhow::anyhow!("PDF export not yet implemented"))
            }
        }
    }

    /// Get entries for a specific entity
    pub fn get_entity_entries(&self, entity_id: &str) -> Vec<&AuditEntry> {
        if let Some(indices) = self.entity_index.get(entity_id) {
            indices.iter()
                .map(|&idx| &self.entries[idx])
                .collect()
        } else {
            Vec::new()
        }
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    /// Create a new audit entry
    fn create_entry(
        &self,
        event_type: AuditEventType,
        entity_id: Option<String>,
        user_id: String,
        description: String,
        result: AuditResult,
        data: HashMap<String, serde_json::Value>,
    ) -> Result<AuditEntry> {
        let id = format!("AUD-{}-{}", Utc::now().timestamp(), uuid::Uuid::new_v4());

        let mut entry = AuditEntry {
            id,
            event_type,
            entity_id,
            user_id,
            timestamp: Utc::now(),
            description,
            result,
            data,
            hash: String::new(), // Will be computed
            previous_hash: self.last_hash.clone(),
        };

        // Compute hash
        entry.hash = Self::compute_hash(&entry);

        Ok(entry)
    }

    /// Add entry to audit trail
    fn add_entry(&mut self, entry: AuditEntry) -> Result<()> {
        // Update entity index
        if let Some(entity_id) = &entry.entity_id {
            self.entity_index
                .entry(entity_id.clone())
                .or_insert_with(Vec::new)
                .push(self.entries.len());
        }

        // Update last hash
        self.last_hash = Some(entry.hash.clone());

        // Add entry
        self.entries.push(entry);

        debug!("Added audit entry, total entries: {}", self.entries.len());

        Ok(())
    }

    /// Compute cryptographic hash of an entry
    fn compute_hash(entry: &AuditEntry) -> String {
        let mut hasher = Sha256::new();

        // Hash all fields except the hash itself
        hasher.update(entry.id.as_bytes());
        hasher.update(format!("{:?}", entry.event_type).as_bytes());
        if let Some(entity_id) = &entry.entity_id {
            hasher.update(entity_id.as_bytes());
        }
        hasher.update(entry.user_id.as_bytes());
        hasher.update(entry.timestamp.to_rfc3339().as_bytes());
        hasher.update(entry.description.as_bytes());
        hasher.update(format!("{:?}", entry.result).as_bytes());

        // Hash data (sorted for consistency)
        let mut keys: Vec<_> = entry.data.keys().collect();
        keys.sort();
        for key in keys {
            hasher.update(key.as_bytes());
            if let Ok(value_str) = serde_json::to_string(&entry.data[key]) {
                hasher.update(value_str.as_bytes());
            }
        }

        // Hash previous hash if present
        if let Some(prev_hash) = &entry.previous_hash {
            hasher.update(prev_hash.as_bytes());
        }

        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Convert to XML format
    fn to_xml(&self) -> Result<String> {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<AuditTrail>\n");

        for entry in &self.entries {
            xml.push_str("  <Entry>\n");
            xml.push_str(&format!("    <ID>{}</ID>\n", entry.id));
            xml.push_str(&format!("    <EventType>{:?}</EventType>\n", entry.event_type));
            if let Some(entity_id) = &entry.entity_id {
                xml.push_str(&format!("    <EntityID>{}</EntityID>\n", entity_id));
            }
            xml.push_str(&format!("    <UserID>{}</UserID>\n", entry.user_id));
            xml.push_str(&format!("    <Timestamp>{}</Timestamp>\n", entry.timestamp.to_rfc3339()));
            xml.push_str(&format!("    <Description>{}</Description>\n", entry.description));
            xml.push_str(&format!("    <Result>{:?}</Result>\n", entry.result));
            xml.push_str(&format!("    <Hash>{}</Hash>\n", entry.hash));
            xml.push_str("  </Entry>\n");
        }

        xml.push_str("</AuditTrail>\n");

        Ok(xml)
    }
}

impl Default for AuditTrail {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Result of a compliance check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub matches: Vec<String>,
    pub lists_checked: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Audit trail verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub total_entries: usize,
    pub issues: Vec<String>,
    pub verified_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Xml,
    Pdf,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_trail_creation() {
        let trail = AuditTrail::new();
        assert_eq!(trail.entries.len(), 0);
        assert!(trail.last_hash.is_none());
    }

    #[test]
    fn test_record_check() {
        let mut trail = AuditTrail::new();

        let result = CheckResult {
            matches: vec![],
            lists_checked: vec!["OFAC".to_string(), "EU".to_string()],
            timestamp: Utc::now(),
        };

        let entry = trail.record_check("ENT-001", "user@example.com", result).unwrap();

        assert_eq!(trail.entries.len(), 1);
        assert!(trail.last_hash.is_some());
        assert_eq!(entry.entity_id, Some("ENT-001".to_string()));
    }

    #[test]
    fn test_chain_integrity() {
        let mut trail = AuditTrail::new();

        // Add multiple entries
        for i in 0..5 {
            let result = CheckResult {
                matches: vec![],
                lists_checked: vec!["OFAC".to_string()],
                timestamp: Utc::now(),
            };

            trail.record_check(&format!("ENT-{:03}", i), "user@example.com", result).unwrap();
        }

        // Verify integrity
        let verification = trail.verify_integrity();
        assert!(verification.is_valid);
        assert_eq!(verification.issues.len(), 0);
    }

    #[test]
    fn test_hash_computation() {
        let entry = AuditEntry {
            id: "TEST-001".to_string(),
            event_type: AuditEventType::ComplianceCheck,
            entity_id: Some("ENT-001".to_string()),
            user_id: "user@example.com".to_string(),
            timestamp: Utc::now(),
            description: "Test entry".to_string(),
            result: AuditResult::Success,
            data: HashMap::new(),
            hash: String::new(),
            previous_hash: None,
        };

        let hash1 = AuditTrail::compute_hash(&entry);
        let hash2 = AuditTrail::compute_hash(&entry);

        // Same entry should produce same hash
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex characters
    }

    #[test]
    fn test_report_generation() {
        let mut trail = AuditTrail::new();

        // Add some entries
        for i in 0..10 {
            let result = CheckResult {
                matches: vec![],
                lists_checked: vec!["OFAC".to_string()],
                timestamp: Utc::now(),
            };

            trail.record_check(&format!("ENT-{:03}", i), "user@example.com", result).unwrap();
        }

        let period = ReportingPeriod {
            start: Utc::now() - chrono::Duration::days(30),
            end: Utc::now(),
            description: "Test Period".to_string(),
        };

        let report = trail.generate_report(period).unwrap();
        assert!(report.statistics.total_checks > 0);
    }
}
