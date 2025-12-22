//! Data models for semantic compliance system
//!
//! This module defines all the core data structures used throughout
//! the AML/KYC compliance system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Entity Types
// ============================================================================

/// An entity being monitored for compliance (customer, company, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entity {
    /// Unique identifier
    pub id: String,

    /// Primary name
    pub name: String,

    /// Type of entity
    pub entity_type: EntityType,

    /// Alternative names, aliases, DBAs
    pub aliases: Vec<String>,

    /// Unique identifiers (tax ID, passport, etc.)
    pub identifiers: Vec<Identifier>,

    /// Relationships to other entities
    pub relationships: Vec<Relationship>,

    /// Current risk score (0.0 - 10.0)
    pub risk_score: f64,

    /// Risk level category
    pub risk_level: RiskLevel,

    /// When this entity was last checked
    pub last_checked: DateTime<Utc>,

    /// When this entity was created in the system
    pub created_at: DateTime<Utc>,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntityType {
    /// Individual person
    Person,

    /// Registered company
    Company,

    /// Non-profit organization
    Organization,

    /// Government entity
    Government,

    /// Trust or foundation
    Trust,

    /// Other entity type
    Other(String),
}

/// A unique identifier for an entity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identifier {
    /// Type of identifier
    pub id_type: IdentifierType,

    /// The identifier value
    pub value: String,

    /// Issuing country/authority
    pub issuer: Option<String>,

    /// Issue date
    pub issue_date: Option<DateTime<Utc>>,

    /// Expiration date
    pub expiry_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IdentifierType {
    /// Tax identification number
    TaxId,

    /// Passport number
    Passport,

    /// National ID card
    NationalId,

    /// Driver's license
    DriversLicense,

    /// Business registration number
    BusinessRegistration,

    /// LEI (Legal Entity Identifier)
    LEI,

    /// SWIFT/BIC code
    Swift,

    /// Custom identifier type
    Custom(String),
}

/// Relationship between entities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relationship {
    /// Entity being related to
    pub target_entity_id: String,

    /// Type of relationship
    pub relationship_type: RelationshipType,

    /// Ownership percentage (if applicable)
    pub ownership_percent: Option<f64>,

    /// When relationship was established
    pub established_date: Option<DateTime<Utc>>,

    /// Whether relationship is active
    pub is_active: bool,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationshipType {
    /// Direct owner
    Owner,

    /// Beneficial owner
    BeneficialOwner,

    /// Director or officer
    Director,

    /// Shareholder
    Shareholder,

    /// Subsidiary
    Subsidiary,

    /// Parent company
    Parent,

    /// Business partner
    Partner,

    /// Authorized signatory
    Signatory,

    /// Family member
    Family,

    /// Associate
    Associate,

    /// Custom relationship
    Custom(String),
}

// ============================================================================
// Sanctions and Watch Lists
// ============================================================================

/// A sanctions list from an official source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsList {
    /// List identifier
    pub id: String,

    /// Source name (OFAC, EU, UN, etc.)
    pub source: SanctionSource,

    /// All entries in this list
    pub entries: Vec<SanctionEntry>,

    /// When this list was last updated
    pub last_updated: DateTime<Utc>,

    /// Version or checksum
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SanctionSource {
    /// US Treasury OFAC
    OFAC,

    /// European Union
    EU,

    /// United Nations
    UN,

    /// UK HM Treasury
    HMTreasury,

    /// Custom watch list
    Custom(String),
}

impl SanctionSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::OFAC => "OFAC",
            Self::EU => "EU",
            Self::UN => "UN",
            Self::HMTreasury => "HM Treasury",
            Self::Custom(s) => s,
        }
    }
}

/// An entry in a sanctions list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionEntry {
    /// Unique entry ID from source
    pub id: String,

    /// Primary names
    pub names: Vec<String>,

    /// Aliases and alternative spellings
    pub aliases: Vec<String>,

    /// Type of entity
    pub entity_type: EntityType,

    /// Sanction programs this entry is part of
    pub programs: Vec<String>,

    /// Unique identifiers
    pub identifiers: Vec<Identifier>,

    /// Known addresses
    pub addresses: Vec<Address>,

    /// Dates of birth (for individuals)
    pub dates_of_birth: Vec<String>,

    /// Nationalities
    pub nationalities: Vec<String>,

    /// Additional remarks
    pub remarks: Option<String>,

    /// When added to list
    pub listed_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Address {
    /// Street address
    pub street: Option<String>,

    /// City
    pub city: Option<String>,

    /// State/Province
    pub state: Option<String>,

    /// Postal code
    pub postal_code: Option<String>,

    /// Country
    pub country: String,
}

// ============================================================================
// Compliance Alerts
// ============================================================================

/// A compliance alert for a potential match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAlert {
    /// Unique alert ID
    pub id: String,

    /// Alert severity
    pub severity: AlertSeverity,

    /// Entity that triggered the alert
    pub entity_id: String,

    /// Entity name
    pub entity_name: String,

    /// Reason for alert
    pub reason: String,

    /// Which list was matched
    pub matched_list: SanctionSource,

    /// The matched sanction entry
    pub matched_entry: SanctionEntry,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Match details
    pub match_details: MatchDetails,

    /// When alert was created
    pub created_at: DateTime<Utc>,

    /// Current status
    pub status: AlertStatus,

    /// Who is assigned to review
    pub assigned_to: Option<String>,

    /// Resolution notes
    pub resolution_notes: Option<String>,

    /// When alert was resolved
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlertSeverity {
    /// Requires immediate action
    Critical,

    /// High priority
    High,

    /// Medium priority
    Medium,

    /// Low priority
    Low,

    /// Informational only
    Info,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
            Self::Info => "INFO",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertStatus {
    /// New alert, not yet reviewed
    New,

    /// Under investigation
    InvestigationPending,

    /// Being reviewed
    UnderReview,

    /// Escalated to management
    Escalated,

    /// Confirmed as true positive
    Confirmed,

    /// Determined to be false positive
    FalsePositive,

    /// Cleared after review
    Cleared,

    /// Requires enhanced due diligence
    RequiresEDD,

    /// Account frozen
    Frozen,

    /// SAR filed
    SARFiled,
}

/// Details about what was matched
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetails {
    /// What field was matched (name, alias, identifier, etc.)
    pub matched_field: MatchedField,

    /// Our entity's value that was matched
    pub entity_value: String,

    /// Sanctions list value that was matched
    pub list_value: String,

    /// Matching algorithm used
    pub algorithm: MatchAlgorithm,

    /// Edit distance (if applicable)
    pub edit_distance: Option<usize>,

    /// Additional context
    pub context: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchedField {
    Name,
    Alias,
    TaxId,
    Passport,
    NationalId,
    BusinessRegistration,
    Address,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchAlgorithm {
    /// Exact string match
    Exact,

    /// Fuzzy/approximate matching
    Fuzzy,

    /// Phonetic matching (Soundex, Metaphone, etc.)
    Phonetic,

    /// Transliteration matching
    Transliteration,

    /// Semantic/ML-based matching
    Semantic,
}

// ============================================================================
// Risk Assessment
// ============================================================================

/// Risk assessment for an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// Entity being assessed
    pub entity_id: String,

    /// Overall risk score (0.0 - 10.0)
    pub overall_score: f64,

    /// Risk level category
    pub risk_level: RiskLevel,

    /// Individual risk factors
    pub factors: Vec<RiskFactor>,

    /// Detailed explanation
    pub explanation: String,

    /// Recommended actions
    pub recommendations: Vec<String>,

    /// When assessment was performed
    pub assessed_at: DateTime<Utc>,

    /// Next review date
    pub next_review: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy)]
pub enum RiskLevel {
    /// Critical risk - immediate action required
    Critical,

    /// High risk - enhanced due diligence
    High,

    /// Medium risk - standard monitoring
    Medium,

    /// Low risk - standard procedures
    Low,

    /// Minimal risk
    Minimal,
}

impl RiskLevel {
    pub fn from_score(score: f64) -> Self {
        if score >= 9.0 {
            Self::Critical
        } else if score >= 7.0 {
            Self::High
        } else if score >= 5.0 {
            Self::Medium
        } else if score >= 3.0 {
            Self::Low
        } else {
            Self::Minimal
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
            Self::Minimal => "MINIMAL",
        }
    }
}

/// Individual risk factor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Factor type
    pub factor_type: RiskFactorType,

    /// Score contribution (0.0 - 10.0)
    pub score: f64,

    /// Weight of this factor
    pub weight: f64,

    /// Description
    pub description: String,

    /// Supporting evidence
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskFactorType {
    /// Sanctions list match
    SanctionsMatch,

    /// Politically exposed person
    PEP,

    /// Adverse media
    AdverseMedia,

    /// High-risk jurisdiction
    HighRiskJurisdiction,

    /// Unusual transaction patterns
    UnusualTransactions,

    /// Hidden/complex ownership
    HiddenOwnership,

    /// Related to high-risk entities
    HighRiskAssociates,

    /// Inconsistent information
    InconsistentData,

    /// Rapid account activity
    RapidActivity,

    /// Cash-intensive business
    CashIntensive,

    /// Custom factor
    Custom(String),
}

// ============================================================================
// Audit Trail
// ============================================================================

/// An entry in the compliance audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique audit entry ID
    pub id: String,

    /// Type of audit event
    pub event_type: AuditEventType,

    /// Entity involved (if applicable)
    pub entity_id: Option<String>,

    /// User who performed the action
    pub user_id: String,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Detailed description
    pub description: String,

    /// Result of the action
    pub result: AuditResult,

    /// Additional data
    pub data: HashMap<String, serde_json::Value>,

    /// Cryptographic hash of this entry
    pub hash: String,

    /// Hash of previous entry (for chain integrity)
    pub previous_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditEventType {
    /// Entity compliance check
    ComplianceCheck,

    /// Alert created
    AlertCreated,

    /// Alert reviewed
    AlertReviewed,

    /// Alert resolved
    AlertResolved,

    /// Risk assessment performed
    RiskAssessment,

    /// Account frozen
    AccountFrozen,

    /// Account unfrozen
    AccountUnfrozen,

    /// SAR filed
    SARFiled,

    /// Enhanced due diligence initiated
    EDDInitiated,

    /// Sanctions list updated
    SanctionsListUpdated,

    /// Configuration changed
    ConfigurationChanged,

    /// Custom event
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditResult {
    /// Action completed successfully
    Success,

    /// Action failed
    Failure,

    /// Action partially completed
    Partial,

    /// Action pending
    Pending,
}

// ============================================================================
// Configuration
// ============================================================================

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    /// Sanctions list sources
    pub sources: Vec<SourceConfig>,

    /// Matching configuration
    pub matching: MatchingConfig,

    /// Risk scoring configuration
    pub risk_scoring: RiskScoringConfig,

    /// Alert configuration
    pub alerts: AlertConfig,

    /// Audit configuration
    pub audit: AuditConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Source name
    pub name: String,

    /// Source type
    pub source_type: SanctionSource,

    /// Whether this source is enabled
    pub enabled: bool,

    /// Update URL
    pub url: String,

    /// Update interval in seconds
    pub update_interval: u64,

    /// Priority level
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchingConfig {
    /// Default matching threshold (0.0 - 1.0)
    pub default_threshold: f64,

    /// Threshold for critical alerts
    pub critical_threshold: f64,

    /// Enable phonetic matching
    pub phonetic_matching: bool,

    /// Enable transliteration
    pub transliteration: bool,

    /// Maximum edit distance for fuzzy matching
    pub max_edit_distance: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScoringConfig {
    /// Risk factor weights
    pub weights: HashMap<String, f64>,

    /// Risk level thresholds
    pub thresholds: HashMap<String, f64>,

    /// Enable ML-based scoring
    pub enable_ml: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Email addresses for notifications
    pub email: Vec<String>,

    /// Slack webhook URL
    pub slack_webhook: Option<String>,

    /// SMS phone numbers
    pub sms: Vec<String>,

    /// Escalation rules
    pub escalation: EscalationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    /// Immediately notify on critical alerts
    pub critical_immediate: bool,

    /// Who to notify for critical alerts
    pub critical_notify: Vec<String>,

    /// Minutes before escalating high severity alerts
    pub high_within_minutes: u64,

    /// Hours before escalating medium severity alerts
    pub medium_within_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Retention period in years
    pub retention_years: u32,

    /// Allowed export formats
    pub allowed_formats: Vec<String>,

    /// Sign reports with cryptographic signature
    pub sign_reports: bool,

    /// Key ID for signing
    pub key_id: Option<String>,
}

// ============================================================================
// Search and Query Types
// ============================================================================

/// Search query for entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySearchQuery {
    /// Search term
    pub query: String,

    /// Entity types to include
    pub entity_types: Option<Vec<EntityType>>,

    /// Risk level filter
    pub risk_levels: Option<Vec<RiskLevel>>,

    /// Date range
    pub date_range: Option<DateRange>,

    /// Maximum results
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

// ============================================================================
// Report Types
// ============================================================================

/// Compliance audit report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// Report ID
    pub id: String,

    /// Reporting period
    pub period: ReportingPeriod,

    /// Statistics
    pub statistics: ReportStatistics,

    /// All audit entries in period
    pub entries: Vec<AuditEntry>,

    /// Generated at
    pub generated_at: DateTime<Utc>,

    /// Cryptographic signature
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportingPeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportStatistics {
    /// Total entities monitored
    pub total_entities: usize,

    /// Total checks performed
    pub total_checks: usize,

    /// Alerts by severity
    pub alerts_by_severity: HashMap<AlertSeverity, usize>,

    /// True positives
    pub true_positives: usize,

    /// False positives
    pub false_positives: usize,

    /// Average response time in minutes
    pub avg_response_time: f64,

    /// Regulatory actions
    pub regulatory_actions: RegulatoryActions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegulatoryActions {
    /// SARs filed
    pub sars_filed: usize,

    /// Accounts frozen
    pub accounts_frozen: usize,

    /// Accounts closed
    pub accounts_closed: usize,

    /// Enhanced due diligence initiated
    pub edd_initiated: usize,
}

// ============================================================================
// Test Utilities
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_serialization() {
        let entity = Entity {
            id: "ENT-001".to_string(),
            name: "John Doe".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["J. Doe".to_string()],
            identifiers: vec![],
            relationships: vec![],
            risk_score: 3.5,
            risk_level: RiskLevel::Low,
            last_checked: Utc::now(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&entity).unwrap();
        let deserialized: Entity = serde_json::from_str(&json).unwrap();
        assert_eq!(entity.id, deserialized.id);
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
    fn test_sanction_source_as_str() {
        assert_eq!(SanctionSource::OFAC.as_str(), "OFAC");
        assert_eq!(SanctionSource::EU.as_str(), "EU");
        assert_eq!(SanctionSource::UN.as_str(), "UN");
    }
}
