//! Risk scoring and assessment engine
//!
//! This module provides comprehensive risk scoring for entities
//! based on multiple factors including sanctions matches, PEP status,
//! transaction patterns, and relationship analysis.

use crate::models::*;
use anyhow::Result;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

// ============================================================================
// Risk Engine
// ============================================================================

/// Risk assessment engine
pub struct RiskEngine {
    /// Risk factor weights
    weights: RiskWeights,

    /// Configuration
    config: RiskScoringConfig,

    /// Historical risk assessments for trend analysis
    history: HashMap<String, Vec<RiskAssessment>>,
}

impl RiskEngine {
    /// Create a new risk engine with default weights
    pub fn new(config: RiskScoringConfig) -> Self {
        let weights = RiskWeights::from_config(&config);

        Self {
            weights,
            config,
            history: HashMap::new(),
        }
    }

    /// Calculate comprehensive risk score for an entity
    pub fn calculate_risk(&self, entity: &Entity) -> Result<RiskAssessment> {
        info!("Calculating risk for entity: {} ({})", entity.name, entity.id);

        let mut factors = Vec::new();
        let mut total_weighted_score = 0.0;
        let mut total_weight = 0.0;

        // 1. Check for sanctions matches
        if let Some(factor) = self.assess_sanctions_risk(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // 2. Check PEP status
        if let Some(factor) = self.assess_pep_risk(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // 3. Check jurisdiction risk
        if let Some(factor) = self.assess_jurisdiction_risk(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // 4. Check ownership complexity
        if let Some(factor) = self.assess_ownership_risk(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // 5. Check relationship risk
        if let Some(factor) = self.assess_relationship_risk(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // 6. Check data consistency
        if let Some(factor) = self.assess_data_consistency(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // 7. Check historical behavior
        if let Some(factor) = self.assess_historical_behavior(entity) {
            total_weighted_score += factor.score * factor.weight;
            total_weight += factor.weight;
            factors.push(factor);
        }

        // Calculate overall score
        let overall_score = if total_weight > 0.0 {
            total_weighted_score / total_weight
        } else {
            0.0
        };

        let risk_level = RiskLevel::from_score(overall_score);

        // Generate explanation
        let explanation = self.generate_explanation(&factors, overall_score);

        // Generate recommendations
        let recommendations = self.generate_recommendations(&risk_level, &factors);

        // Calculate next review date
        let next_review = self.calculate_next_review(&risk_level);

        let assessment = RiskAssessment {
            entity_id: entity.id.clone(),
            overall_score,
            risk_level,
            factors,
            explanation,
            recommendations,
            assessed_at: Utc::now(),
            next_review,
        };

        debug!("Risk assessment completed: score={}, level={:?}", overall_score, risk_level);

        Ok(assessment)
    }

    /// Update risk based on new information
    pub fn update_risk(
        &mut self,
        entity_id: &str,
        _new_factors: Vec<RiskFactor>,
    ) -> Result<()> {
        info!("Updating risk for entity: {}", entity_id);

        // Store in history for trend analysis
        if let Some(history) = self.history.get_mut(entity_id) {
            // Keep last 100 assessments
            if history.len() >= 100 {
                history.remove(0);
            }
        }

        Ok(())
    }

    /// Get risk explanation
    pub fn explain_risk(&self, assessment: &RiskAssessment) -> RiskExplanation {
        let mut contributing_factors = Vec::new();
        let mut mitigating_factors = Vec::new();

        for factor in &assessment.factors {
            if factor.score > 5.0 {
                contributing_factors.push(format!(
                    "{}: {} (score: {:.1})",
                    factor.factor_type.as_str(),
                    factor.description,
                    factor.score
                ));
            } else if factor.score < 2.0 {
                mitigating_factors.push(format!(
                    "{}: {} (score: {:.1})",
                    factor.factor_type.as_str(),
                    factor.description,
                    factor.score
                ));
            }
        }

        RiskExplanation {
            overall_score: assessment.overall_score,
            risk_level: assessment.risk_level.clone(),
            contributing_factors,
            mitigating_factors,
            summary: assessment.explanation.clone(),
        }
    }

    // ========================================================================
    // Individual Risk Factor Assessments
    // ========================================================================

    fn assess_sanctions_risk(&self, entity: &Entity) -> Option<RiskFactor> {
        // Check if entity has any sanctions-related metadata
        let has_sanctions = entity.metadata.contains_key("sanctions_match");

        if has_sanctions {
            Some(RiskFactor {
                factor_type: RiskFactorType::SanctionsMatch,
                score: 10.0, // Maximum risk
                weight: self.weights.sanctions_match,
                description: "Entity matches sanctions list".to_string(),
                evidence: vec!["Sanctions list match detected".to_string()],
            })
        } else {
            None
        }
    }

    fn assess_pep_risk(&self, entity: &Entity) -> Option<RiskFactor> {
        // Check if entity is a Politically Exposed Person
        let is_pep = entity.metadata.get("is_pep")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_pep {
            Some(RiskFactor {
                factor_type: RiskFactorType::PEP,
                score: 7.0,
                weight: self.weights.pep_status,
                description: "Politically Exposed Person".to_string(),
                evidence: vec!["PEP status confirmed".to_string()],
            })
        } else {
            None
        }
    }

    fn assess_jurisdiction_risk(&self, entity: &Entity) -> Option<RiskFactor> {
        // Check if entity is in a high-risk jurisdiction
        let jurisdiction = entity.metadata.get("jurisdiction")
            .and_then(|v| v.as_str());

        if let Some(country) = jurisdiction {
            let risk_score = Self::get_jurisdiction_risk_score(country);

            if risk_score > 5.0 {
                Some(RiskFactor {
                    factor_type: RiskFactorType::HighRiskJurisdiction,
                    score: risk_score,
                    weight: self.weights.high_risk_jurisdiction,
                    description: format!("Entity in high-risk jurisdiction: {}", country),
                    evidence: vec![format!("Country: {}", country)],
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn assess_ownership_risk(&self, entity: &Entity) -> Option<RiskFactor> {
        // Calculate ownership complexity
        let beneficial_owners: Vec<_> = entity.relationships.iter()
            .filter(|r| matches!(r.relationship_type, RelationshipType::BeneficialOwner))
            .collect();

        let total_ownership: f64 = beneficial_owners.iter()
            .filter_map(|r| r.ownership_percent)
            .sum();

        // Risk factors:
        // - No identified beneficial owners (score: 9.0)
        // - Ownership doesn't add up to 100% (score: 7.0)
        // - Complex ownership structure (score: 6.0)

        if beneficial_owners.is_empty() {
            Some(RiskFactor {
                factor_type: RiskFactorType::HiddenOwnership,
                score: 9.0,
                weight: self.weights.hidden_ownership,
                description: "No beneficial owners identified".to_string(),
                evidence: vec!["Beneficial ownership structure unclear".to_string()],
            })
        } else if (total_ownership - 100.0).abs() > 10.0 {
            Some(RiskFactor {
                factor_type: RiskFactorType::HiddenOwnership,
                score: 7.0,
                weight: self.weights.hidden_ownership,
                description: format!(
                    "Ownership percentages don't add up correctly ({:.1}%)",
                    total_ownership
                ),
                evidence: vec![format!("Total ownership: {:.1}%", total_ownership)],
            })
        } else if beneficial_owners.len() > 5 {
            Some(RiskFactor {
                factor_type: RiskFactorType::HiddenOwnership,
                score: 6.0,
                weight: self.weights.hidden_ownership,
                description: format!("Complex ownership structure ({} owners)", beneficial_owners.len()),
                evidence: vec![format!("{} beneficial owners", beneficial_owners.len())],
            })
        } else {
            None
        }
    }

    fn assess_relationship_risk(&self, entity: &Entity) -> Option<RiskFactor> {
        // Check if entity has relationships with high-risk entities
        // This would typically query the graph database
        // For now, check metadata

        let high_risk_associates = entity.metadata.get("high_risk_associates")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if high_risk_associates > 0 {
            let score = (5.0 + (high_risk_associates as f64 * 0.5)).min(10.0);

            Some(RiskFactor {
                factor_type: RiskFactorType::HighRiskAssociates,
                score,
                weight: self.weights.high_risk_associates,
                description: format!(
                    "Associated with {} high-risk entities",
                    high_risk_associates
                ),
                evidence: vec![format!("{} high-risk connections", high_risk_associates)],
            })
        } else {
            None
        }
    }

    fn assess_data_consistency(&self, entity: &Entity) -> Option<RiskFactor> {
        // Check for data consistency issues
        let mut issues = Vec::new();

        // Check if name and identifiers are consistent
        if entity.name.is_empty() {
            issues.push("Missing entity name".to_string());
        }

        if entity.identifiers.is_empty() {
            issues.push("No identification documents".to_string());
        }

        // Check for expired identifiers
        let expired_ids = entity.identifiers.iter()
            .filter(|id| {
                id.expiry_date.map(|exp| exp < Utc::now()).unwrap_or(false)
            })
            .count();

        if expired_ids > 0 {
            issues.push(format!("{} expired identification documents", expired_ids));
        }

        if !issues.is_empty() {
            Some(RiskFactor {
                factor_type: RiskFactorType::InconsistentData,
                score: 5.0 + (issues.len() as f64 * 1.0),
                weight: self.weights.inconsistent_data,
                description: "Data consistency issues detected".to_string(),
                evidence: issues,
            })
        } else {
            None
        }
    }

    fn assess_historical_behavior(&self, entity: &Entity) -> Option<RiskFactor> {
        // Check historical risk trends
        if let Some(history) = self.history.get(&entity.id) {
            if history.len() >= 3 {
                // Check if risk is increasing
                let recent_scores: Vec<f64> = history.iter()
                    .rev()
                    .take(3)
                    .map(|a| a.overall_score)
                    .collect();

                let is_increasing = recent_scores.windows(2)
                    .all(|w| w[0] < w[1]);

                if is_increasing {
                    return Some(RiskFactor {
                        factor_type: RiskFactorType::Custom("trend".to_string()),
                        score: 6.0,
                        weight: 1.0,
                        description: "Risk score trending upward".to_string(),
                        evidence: vec![format!("Scores: {:?}", recent_scores)],
                    });
                }
            }
        }

        None
    }

    // ========================================================================
    // Helper Functions
    // ========================================================================

    fn get_jurisdiction_risk_score(country: &str) -> f64 {
        // Simplified risk scoring based on country
        // In production, use official FATF high-risk jurisdictions list
        match country.to_uppercase().as_str() {
            // High risk
            "IRAN" | "NORTH KOREA" | "MYANMAR" => 10.0,

            // Elevated risk
            "AFGHANISTAN" | "SYRIA" | "YEMEN" => 8.0,

            // Some concerns
            "RUSSIA" | "BELARUS" => 7.0,

            // Standard risk
            _ => 3.0,
        }
    }

    fn generate_explanation(&self, factors: &[RiskFactor], overall_score: f64) -> String {
        if factors.is_empty() {
            return "No significant risk factors identified.".to_string();
        }

        let mut explanation = format!(
            "Overall risk score of {:.1}/10.0 based on {} factors:\n",
            overall_score,
            factors.len()
        );

        // Sort factors by contribution (score * weight)
        let mut sorted_factors = factors.to_vec();
        sorted_factors.sort_by(|a, b| {
            let contrib_a = a.score * a.weight;
            let contrib_b = b.score * b.weight;
            contrib_b.partial_cmp(&contrib_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        for (i, factor) in sorted_factors.iter().take(5).enumerate() {
            explanation.push_str(&format!(
                "{}. {} - {} (score: {:.1}, weight: {:.1})\n",
                i + 1,
                factor.factor_type.as_str(),
                factor.description,
                factor.score,
                factor.weight
            ));
        }

        explanation
    }

    fn generate_recommendations(
        &self,
        risk_level: &RiskLevel,
        factors: &[RiskFactor],
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        match risk_level {
            RiskLevel::Critical => {
                recommendations.push("IMMEDIATE ACTION REQUIRED".to_string());
                recommendations.push("Freeze account pending investigation".to_string());
                recommendations.push("Escalate to senior compliance officer".to_string());
                recommendations.push("Consider filing SAR".to_string());
            }
            RiskLevel::High => {
                recommendations.push("Enhanced Due Diligence required".to_string());
                recommendations.push("Increase monitoring frequency".to_string());
                recommendations.push("Senior management review".to_string());
            }
            RiskLevel::Medium => {
                recommendations.push("Standard Due Diligence procedures".to_string());
                recommendations.push("Regular monitoring".to_string());
            }
            RiskLevel::Low | RiskLevel::Minimal => {
                recommendations.push("Standard procedures apply".to_string());
                recommendations.push("Periodic review sufficient".to_string());
            }
        }

        // Add factor-specific recommendations
        for factor in factors {
            match factor.factor_type {
                RiskFactorType::SanctionsMatch => {
                    recommendations.push("Verify sanctions match accuracy".to_string());
                }
                RiskFactorType::HiddenOwnership => {
                    recommendations.push("Request beneficial ownership documentation".to_string());
                }
                RiskFactorType::InconsistentData => {
                    recommendations.push("Verify and update entity information".to_string());
                }
                _ => {}
            }
        }

        recommendations
    }

    fn calculate_next_review(&self, risk_level: &RiskLevel) -> chrono::DateTime<Utc> {
        let days = match risk_level {
            RiskLevel::Critical => 1,   // Daily review
            RiskLevel::High => 7,        // Weekly review
            RiskLevel::Medium => 30,     // Monthly review
            RiskLevel::Low => 90,        // Quarterly review
            RiskLevel::Minimal => 365,   // Annual review
        };

        Utc::now() + Duration::days(days)
    }
}

// ============================================================================
// Risk Weights
// ============================================================================

/// Weights for different risk factors
#[derive(Debug, Clone)]
pub struct RiskWeights {
    pub sanctions_match: f64,
    pub pep_status: f64,
    pub adverse_media: f64,
    pub high_risk_jurisdiction: f64,
    pub unusual_transactions: f64,
    pub hidden_ownership: f64,
    pub high_risk_associates: f64,
    pub inconsistent_data: f64,
}

impl RiskWeights {
    /// Create weights from configuration
    pub fn from_config(config: &RiskScoringConfig) -> Self {
        Self {
            sanctions_match: config.weights.get("sanctions_match").copied().unwrap_or(10.0),
            pep_status: config.weights.get("pep_status").copied().unwrap_or(7.0),
            adverse_media: config.weights.get("adverse_media").copied().unwrap_or(5.0),
            high_risk_jurisdiction: config.weights.get("high_risk_jurisdiction").copied().unwrap_or(6.0),
            unusual_transactions: config.weights.get("unusual_transactions").copied().unwrap_or(8.0),
            hidden_ownership: config.weights.get("hidden_ownership").copied().unwrap_or(7.5),
            high_risk_associates: config.weights.get("high_risk_associates").copied().unwrap_or(6.5),
            inconsistent_data: config.weights.get("inconsistent_data").copied().unwrap_or(4.0),
        }
    }

    /// Default weights
    pub fn default() -> Self {
        Self {
            sanctions_match: 10.0,
            pep_status: 7.0,
            adverse_media: 5.0,
            high_risk_jurisdiction: 6.0,
            unusual_transactions: 8.0,
            hidden_ownership: 7.5,
            high_risk_associates: 6.5,
            inconsistent_data: 4.0,
        }
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Detailed risk explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskExplanation {
    pub overall_score: f64,
    pub risk_level: RiskLevel,
    pub contributing_factors: Vec<String>,
    pub mitigating_factors: Vec<String>,
    pub summary: String,
}

impl RiskFactorType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SanctionsMatch => "Sanctions Match",
            Self::PEP => "Politically Exposed Person",
            Self::AdverseMedia => "Adverse Media",
            Self::HighRiskJurisdiction => "High-Risk Jurisdiction",
            Self::UnusualTransactions => "Unusual Transactions",
            Self::HiddenOwnership => "Hidden Ownership",
            Self::HighRiskAssociates => "High-Risk Associates",
            Self::InconsistentData => "Inconsistent Data",
            Self::RapidActivity => "Rapid Activity",
            Self::CashIntensive => "Cash-Intensive Business",
            Self::Custom(s) => s,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> RiskScoringConfig {
        RiskScoringConfig {
            weights: HashMap::new(),
            thresholds: HashMap::new(),
            enable_ml: false,
        }
    }

    #[test]
    fn test_risk_weights() {
        let weights = RiskWeights::default();
        assert_eq!(weights.sanctions_match, 10.0);
        assert_eq!(weights.pep_status, 7.0);
    }

    #[test]
    fn test_jurisdiction_risk() {
        assert_eq!(RiskEngine::get_jurisdiction_risk_score("IRAN"), 10.0);
        assert_eq!(RiskEngine::get_jurisdiction_risk_score("AFGHANISTAN"), 8.0);
        assert_eq!(RiskEngine::get_jurisdiction_risk_score("USA"), 3.0);
    }

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(RiskLevel::from_score(9.5), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(7.5), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(5.5), RiskLevel::Medium);
        assert_eq!(RiskLevel::from_score(3.5), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(1.5), RiskLevel::Minimal);
    }

    #[test]
    fn test_calculate_risk() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        let entity = Entity {
            id: "TEST-001".to_string(),
            name: "Test Entity".to_string(),
            entity_type: EntityType::Company,
            aliases: vec![],
            identifiers: vec![],
            relationships: vec![],
            risk_score: 0.0,
            risk_level: RiskLevel::Low,
            last_checked: Utc::now(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        };

        let assessment = engine.calculate_risk(&entity).unwrap();
        assert!(assessment.overall_score >= 0.0);
        assert!(assessment.overall_score <= 10.0);
    }
}
