//! Semantic Compliance CLI
//!
//! Command-line interface for the AML/KYC compliance system

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use semantic_compliance::*;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber;

// ============================================================================
// CLI Definition
// ============================================================================

#[derive(Parser)]
#[command(name = "semantic-compliance")]
#[command(about = "Real-time AML/KYC compliance system", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Configuration file path
    #[arg(short, long, global = true, default_value = "config.toml")]
    config: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the compliance system
    Init {
        /// Data directory
        #[arg(short, long, default_value = "data")]
        data_dir: PathBuf,
    },

    /// Start real-time sanctions monitoring
    Watch {
        /// Sanctions list sources to monitor
        #[arg(short, long, value_delimiter = ',')]
        sources: Vec<String>,

        /// Update interval in seconds
        #[arg(short, long, default_value = "3600")]
        interval: u64,

        /// Critical alert threshold
        #[arg(long, default_value = "0.95")]
        critical_threshold: f64,

        /// High alert threshold
        #[arg(long, default_value = "0.85")]
        high_threshold: f64,
    },

    /// Check an entity against sanctions lists
    Check {
        /// Entity name or ID to check
        entity: String,

        /// Perform deep check with relationship analysis
        #[arg(short, long)]
        deep: bool,

        /// Maximum depth for relationship traversal
        #[arg(long, default_value = "3")]
        max_depth: usize,

        /// Matching threshold
        #[arg(short, long, default_value = "0.85")]
        threshold: f64,
    },

    /// Generate compliance audit report
    Audit {
        /// Reporting period (e.g., "2024-Q4")
        #[arg(short, long)]
        period: Option<String>,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,

        /// End date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Include regulator-specific formatting
        #[arg(long)]
        regulator_format: bool,

        /// Include cryptographic proofs
        #[arg(long)]
        include_proofs: bool,
    },

    /// Manage compliance alerts
    Alerts {
        #[command(subcommand)]
        action: AlertAction,
    },

    /// Analyze entity relationships
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },

    /// Import entities from file
    Import {
        /// Input file (JSON, CSV)
        #[arg(short, long)]
        file: PathBuf,

        /// File format
        #[arg(short = 't', long, default_value = "json")]
        format: String,
    },

    /// Configure system settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show system statistics
    Stats,

    /// Assess risk for an entity
    Risk {
        /// Entity ID
        entity_id: String,

        /// Show detailed explanation
        #[arg(short, long)]
        explain: bool,
    },
}

#[derive(Subcommand)]
enum AlertAction {
    /// List alerts
    List {
        /// Filter by severity
        #[arg(short, long)]
        severity: Option<String>,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Limit number of results
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },

    /// Acknowledge an alert
    Ack {
        /// Alert ID
        alert_id: String,

        /// User ID
        #[arg(short, long)]
        user: String,

        /// Notes
        #[arg(short, long)]
        notes: String,
    },

    /// Resolve an alert
    Resolve {
        /// Alert ID
        alert_id: String,

        /// Resolution status
        #[arg(short, long)]
        resolution: String,

        /// User ID
        #[arg(short, long)]
        user: String,

        /// Notes
        #[arg(short, long)]
        notes: String,
    },

    /// Configure alert notifications
    ConfigureNotifications {
        /// Email addresses (comma-separated)
        #[arg(short, long)]
        email: Option<String>,

        /// Slack webhook URL
        #[arg(short, long)]
        slack: Option<String>,
    },
}

#[derive(Subcommand)]
enum GraphAction {
    /// Visualize entity relationships
    Visualize {
        /// Entity ID
        entity_id: String,

        /// Traversal depth
        #[arg(short, long, default_value = "2")]
        depth: usize,

        /// Output format (dot, svg, png)
        #[arg(short, long, default_value = "dot")]
        format: String,
    },

    /// Find connections between entities
    Connections {
        /// First entity
        entity_a: String,

        /// Second entity
        entity_b: String,
    },

    /// Detect suspicious clusters
    Clusters {
        /// Clustering algorithm
        #[arg(short, long, default_value = "community-detection")]
        algorithm: String,
    },

    /// Trace ownership structure
    Ownership {
        /// Entity ID
        entity_id: String,

        /// Maximum depth
        #[arg(short, long, default_value = "5")]
        max_depth: usize,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Add a sanctions list source
    AddSource {
        /// Source name
        #[arg(short, long)]
        name: String,

        /// Source URL
        #[arg(short, long)]
        url: String,

        /// Source type (OFAC, EU, UN, etc.)
        #[arg(short = 't', long)]
        source_type: String,
    },

    /// Remove a sanctions list source
    RemoveSource {
        /// Source name
        name: String,
    },

    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,

        /// Configuration value
        value: String,
    },
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let log_level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .init();

    // Load configuration
    let config = load_config(&cli.config)?;

    // Create compliance system
    let mut system = ComplianceSystem::new(config);

    // Execute command
    match cli.command {
        Commands::Init { data_dir } => {
            cmd_init(&data_dir).await?;
        }
        Commands::Watch {
            sources,
            interval,
            critical_threshold,
            high_threshold,
        } => {
            cmd_watch(&mut system, sources, interval, critical_threshold, high_threshold).await?;
        }
        Commands::Check {
            entity,
            deep,
            max_depth,
            threshold,
        } => {
            cmd_check(&mut system, &entity, deep, max_depth, threshold).await?;
        }
        Commands::Audit {
            period,
            from,
            to,
            format,
            output,
            regulator_format,
            include_proofs,
        } => {
            cmd_audit(
                &system,
                period,
                from,
                to,
                &format,
                output,
                regulator_format,
                include_proofs,
            ).await?;
        }
        Commands::Alerts { action } => {
            cmd_alerts(&mut system, action).await?;
        }
        Commands::Graph { action } => {
            cmd_graph(&system, action).await?;
        }
        Commands::Import { file, format } => {
            cmd_import(&mut system, &file, &format).await?;
        }
        Commands::Config { action } => {
            cmd_config(action).await?;
        }
        Commands::Stats => {
            cmd_stats(&system).await?;
        }
        Commands::Risk { entity_id, explain } => {
            cmd_risk(&mut system, &entity_id, explain).await?;
        }
    }

    Ok(())
}

// ============================================================================
// Command Implementations
// ============================================================================

async fn cmd_init(data_dir: &PathBuf) -> Result<()> {
    println!("{}", "Initializing Semantic Compliance System...".bold().green());

    // Create data directory
    tokio::fs::create_dir_all(data_dir).await
        .context("Failed to create data directory")?;

    println!("  {} Data directory: {}", "✓".green(), data_dir.display());

    // Create subdirectories
    for subdir in &["entities", "alerts", "audit", "cache"] {
        let path = data_dir.join(subdir);
        tokio::fs::create_dir_all(&path).await?;
        println!("  {} Created: {}", "✓".green(), path.display());
    }

    // Create default config
    let config_path = "config.toml";
    if !std::path::Path::new(config_path).exists() {
        let default_config = create_default_config();
        tokio::fs::write(config_path, default_config).await?;
        println!("  {} Created default config: {}", "✓".green(), config_path);
    }

    println!("\n{}", "Initialization complete!".bold().green());
    println!("\nNext steps:");
    println!("  1. Review and update config.toml");
    println!("  2. Import entities: semantic-compliance import --file entities.json");
    println!("  3. Start monitoring: semantic-compliance watch --sources OFAC,EU,UN");

    Ok(())
}

async fn cmd_watch(
    system: &mut ComplianceSystem,
    sources: Vec<String>,
    interval: u64,
    critical_threshold: f64,
    high_threshold: f64,
) -> Result<()> {
    println!("{}", "Starting real-time sanctions monitoring...".bold().cyan());
    println!("  Sources: {}", sources.join(", "));
    println!("  Update interval: {}s", interval);
    println!("  Critical threshold: {:.2}", critical_threshold);
    println!("  High threshold: {:.2}", high_threshold);
    println!();

    let stats = system.get_statistics().await;
    println!("Monitoring {} entities", stats.total_entities);
    println!("{} sanctions lists loaded ({} entries)",
        stats.sanctions_lists_loaded,
        stats.total_sanctions_entries
    );
    println!();

    // In a real implementation, this would start background monitoring
    println!("{}", "Press Ctrl+C to stop monitoring".dimmed());

    // Simulate monitoring
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
        println!("{} {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            "Checking for sanctions list updates...".dimmed()
        );
    }
}

async fn cmd_check(
    system: &mut ComplianceSystem,
    entity: &str,
    deep: bool,
    max_depth: usize,
    threshold: f64,
) -> Result<()> {
    println!("{}", format!("Checking entity: {}", entity).bold().cyan());
    println!();

    // For demo purposes, create a test entity if not found
    if system.get_entity(entity).is_none() {
        let test_entity = Entity {
            id: entity.to_string(),
            name: entity.to_string(),
            entity_type: EntityType::Company,
            aliases: vec![],
            identifiers: vec![],
            relationships: vec![],
            risk_score: 0.0,
            risk_level: RiskLevel::Low,
            last_checked: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        };
        system.add_entity(test_entity).await?;
    }

    // Check against sanctions lists
    println!("├─ {} Checking sanctions lists...", "●".cyan());
    let matches = system.check_entity(entity).await?;

    if matches.is_empty() {
        println!("│  {} No matches found", "✓".green());
    } else {
        println!("│  {} {} potential matches found", "⚠".yellow(), matches.len());
        for (i, m) in matches.iter().take(5).enumerate() {
            println!("│  {}. {} (confidence: {:.2})",
                i + 1,
                m.entry.names.first().unwrap_or(&"Unknown".to_string()),
                m.confidence
            );
        }
    }

    // Risk assessment
    println!("├─ {} Calculating risk score...", "●".cyan());
    let assessment = system.assess_risk(entity).await?;

    let risk_color = match assessment.risk_level {
        RiskLevel::Critical => "red",
        RiskLevel::High => "yellow",
        RiskLevel::Medium => "blue",
        RiskLevel::Low | RiskLevel::Minimal => "green",
    };

    println!("│  Score: {:.1}/10.0 ({})",
        assessment.overall_score,
        assessment.risk_level.as_str().color(risk_color).bold()
    );

    if deep {
        println!("├─ {} Performing deep analysis...", "●".cyan());

        // Ownership analysis
        if let Ok(ownership) = system.trace_ownership(entity, max_depth) {
            println!("│  Ownership structure:");
            print_ownership_tree(&ownership, "│  ", 0);
        }
    }

    println!("└─ {} Analysis complete", "✓".green());
    println!();

    // Show recommendations
    if !assessment.recommendations.is_empty() {
        println!("{}", "Recommendations:".bold());
        for rec in assessment.recommendations {
            println!("  • {}", rec);
        }
    }

    Ok(())
}

async fn cmd_audit(
    system: &ComplianceSystem,
    period: Option<String>,
    from: Option<String>,
    to: Option<String>,
    format: &str,
    output: Option<PathBuf>,
    _regulator_format: bool,
    _include_proofs: bool,
) -> Result<()> {
    println!("{}", "Generating Compliance Audit Report".bold().cyan());
    println!();

    // Parse dates
    let (start, end, description) = if let Some(period_str) = period {
        parse_period(&period_str)?
    } else if let (Some(from_str), Some(to_str)) = (from, to) {
        let start = chrono::NaiveDate::parse_from_str(&from_str, "%Y-%m-%d")?
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let end = chrono::NaiveDate::parse_from_str(&to_str, "%Y-%m-%d")?
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();
        (start, end, format!("{} to {}", from_str, to_str))
    } else {
        // Default to last 30 days
        let end = chrono::Utc::now();
        let start = end - chrono::Duration::days(30);
        (start, end, "Last 30 days".to_string())
    };

    let reporting_period = ReportingPeriod {
        start,
        end,
        description,
    };

    let report = system.generate_report(reporting_period)?;

    // Display statistics
    println!("Period: {}", report.period.description);
    println!();
    println!("{}", "Statistics:".bold());
    println!("├─ Total Entities Monitored: {}", report.statistics.total_entities);
    println!("├─ Total Checks Performed: {}", report.statistics.total_checks);
    println!("├─ Alerts Generated: {}",
        report.statistics.alerts_by_severity.values().sum::<usize>()
    );

    for (severity, count) in &report.statistics.alerts_by_severity {
        println!("│  ├─ {}: {}", severity.as_str(), count);
    }

    println!("├─ True Positives: {}", report.statistics.true_positives);
    println!("├─ False Positives: {}", report.statistics.false_positives);
    println!("├─ Average Response Time: {:.1} minutes", report.statistics.avg_response_time);
    println!("└─ Regulatory Actions:");
    println!("   ├─ SARs Filed: {}", report.statistics.regulatory_actions.sars_filed);
    println!("   ├─ Accounts Frozen: {}", report.statistics.regulatory_actions.accounts_frozen);
    println!("   ├─ Accounts Closed: {}", report.statistics.regulatory_actions.accounts_closed);
    println!("   └─ Enhanced Due Diligence: {}", report.statistics.regulatory_actions.edd_initiated);
    println!();

    // Save report
    let output_path = output.unwrap_or_else(|| {
        PathBuf::from(format!("audit-report-{}.{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            format
        ))
    });

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&report)?;
            tokio::fs::write(&output_path, json).await?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported format: {}", format));
        }
    }

    println!("{} Report saved to: {}", "✓".green(), output_path.display());

    Ok(())
}

async fn cmd_alerts(system: &mut ComplianceSystem, action: AlertAction) -> Result<()> {
    match action {
        AlertAction::List { severity, status: _, limit } => {
            let severity_filter = severity.as_ref().and_then(|s| match s.to_uppercase().as_str() {
                "CRITICAL" => Some(AlertSeverity::Critical),
                "HIGH" => Some(AlertSeverity::High),
                "MEDIUM" => Some(AlertSeverity::Medium),
                "LOW" => Some(AlertSeverity::Low),
                "INFO" => Some(AlertSeverity::Info),
                _ => None,
            });

            let alerts = system.get_alerts(severity_filter);

            println!("{}", format!("Active Alerts ({})", alerts.len()).bold().cyan());
            println!();

            for (i, alert) in alerts.iter().take(limit).enumerate() {
                let sev_color = match alert.severity {
                    AlertSeverity::Critical => "red",
                    AlertSeverity::High => "yellow",
                    AlertSeverity::Medium => "blue",
                    _ => "white",
                };

                println!("{}. [{}] {} - {}",
                    i + 1,
                    alert.severity.as_str().color(sev_color).bold(),
                    alert.entity_name,
                    alert.reason
                );
                println!("   ID: {} | Confidence: {:.2} | Status: {:?}",
                    alert.id, alert.confidence, alert.status
                );
                println!();
            }
        }
        AlertAction::Ack { alert_id, user, notes } => {
            system.resolve_alert(&alert_id, AlertStatus::UnderReview, &notes, &user)?;
            println!("{} Alert acknowledged: {}", "✓".green(), alert_id);
        }
        AlertAction::Resolve { alert_id, resolution, user, notes } => {
            let status = match resolution.to_uppercase().as_str() {
                "CONFIRMED" => AlertStatus::Confirmed,
                "FALSE_POSITIVE" => AlertStatus::FalsePositive,
                "CLEARED" => AlertStatus::Cleared,
                _ => return Err(anyhow::anyhow!("Invalid resolution status")),
            };

            system.resolve_alert(&alert_id, status, &notes, &user)?;
            println!("{} Alert resolved: {}", "✓".green(), alert_id);
        }
        AlertAction::ConfigureNotifications { email: _, slack: _ } => {
            println!("Alert notifications configured");
        }
    }

    Ok(())
}

async fn cmd_graph(system: &ComplianceSystem, action: GraphAction) -> Result<()> {
    match action {
        GraphAction::Visualize { entity_id, depth, format: _ } => {
            println!("{}", format!("Visualizing relationships for: {}", entity_id).bold().cyan());
            println!("Depth: {}", depth);
            println!();
            println!("(Visualization not yet implemented)");
        }
        GraphAction::Connections { entity_a, entity_b } => {
            println!("{}", format!("Finding connections: {} <-> {}", entity_a, entity_b).bold().cyan());
            println!();

            let paths = system.find_connections(&entity_a, &entity_b)?;

            if paths.is_empty() {
                println!("No connections found");
            } else {
                println!("Found {} paths:", paths.len());
                for (i, path) in paths.iter().take(5).enumerate() {
                    println!("\n{}. Path length: {} steps", i + 1, path.length);
                    if let Some(ownership) = path.total_ownership {
                        println!("   Effective ownership: {:.1}%", ownership);
                    }
                }
            }
        }
        GraphAction::Clusters { algorithm } => {
            let algo = match algorithm.as_str() {
                "community-detection" => ClusterAlgorithm::CommunityDetection,
                "strongly-connected" => ClusterAlgorithm::StronglyConnected,
                "high-risk-network" => ClusterAlgorithm::HighRiskNetwork,
                _ => return Err(anyhow::anyhow!("Unknown algorithm: {}", algorithm)),
            };

            println!("{}", format!("Detecting clusters using: {}", algorithm).bold().cyan());
            println!();

            let clusters = system.detect_clusters(algo)?;

            println!("Found {} clusters:", clusters.len());
            for (i, cluster) in clusters.iter().take(10).enumerate() {
                println!("\n{}. {} entities - Risk: {:.1}/10.0",
                    i + 1,
                    cluster.entities.len(),
                    cluster.risk_score
                );
                println!("   {}", cluster.description);
            }
        }
        GraphAction::Ownership { entity_id, max_depth } => {
            println!("{}", format!("Tracing ownership for: {}", entity_id).bold().cyan());
            println!();

            let tree = system.trace_ownership(&entity_id, max_depth)?;
            print_ownership_tree(&tree, "", 0);
        }
    }

    Ok(())
}

async fn cmd_import(_system: &mut ComplianceSystem, file: &PathBuf, format: &str) -> Result<()> {
    println!("{}", format!("Importing entities from: {}", file.display()).bold().cyan());
    println!("Format: {}", format);
    println!();

    // Read file
    let content = tokio::fs::read_to_string(file).await?;

    match format {
        "json" => {
            let _entities: Vec<Entity> = serde_json::from_str(&content)?;
            println!("(Import functionality not yet fully implemented)");
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported format: {}", format));
        }
    }

    Ok(())
}

async fn cmd_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::AddSource { name, url, source_type } => {
            println!("Adding sanctions list source:");
            println!("  Name: {}", name);
            println!("  URL: {}", url);
            println!("  Type: {}", source_type);
            println!();
            println!("(Configuration update not yet implemented)");
        }
        ConfigAction::RemoveSource { name } => {
            println!("Removing source: {}", name);
        }
        ConfigAction::Show => {
            println!("{}", "Current Configuration:".bold());
            println!("(Configuration display not yet implemented)");
        }
        ConfigAction::Set { key, value } => {
            println!("Setting {} = {}", key, value);
        }
    }

    Ok(())
}

async fn cmd_stats(system: &ComplianceSystem) -> Result<()> {
    let stats = system.get_statistics().await;

    println!("{}", "System Statistics".bold().cyan());
    println!();
    println!("├─ Total Entities: {}", stats.total_entities);
    println!("├─ High-Risk Entities: {}", stats.high_risk_entities);
    println!("├─ Active Alerts: {}", stats.active_alerts);
    println!("├─ Sanctions Lists Loaded: {}", stats.sanctions_lists_loaded);
    println!("├─ Total Sanctions Entries: {}", stats.total_sanctions_entries);
    println!("└─ Graph Connections: {}", stats.graph_connections);

    Ok(())
}

async fn cmd_risk(system: &mut ComplianceSystem, entity_id: &str, explain: bool) -> Result<()> {
    println!("{}", format!("Risk Assessment: {}", entity_id).bold().cyan());
    println!();

    let assessment = system.assess_risk(entity_id).await?;

    let risk_color = match assessment.risk_level {
        RiskLevel::Critical => "red",
        RiskLevel::High => "yellow",
        RiskLevel::Medium => "blue",
        RiskLevel::Low | RiskLevel::Minimal => "green",
    };

    println!("Overall Score: {:.1}/10.0", assessment.overall_score);
    println!("Risk Level: {}", assessment.risk_level.as_str().color(risk_color).bold());
    println!("Next Review: {}", assessment.next_review.format("%Y-%m-%d"));
    println!();

    if explain {
        println!("{}", "Risk Factors:".bold());
        for factor in &assessment.factors {
            println!("  • {} - {:.1}/10.0",
                factor.factor_type.as_str(),
                factor.score
            );
            println!("    {}", factor.description);
        }
        println!();

        println!("{}", "Recommendations:".bold());
        for rec in &assessment.recommendations {
            println!("  • {}", rec);
        }
    }

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn load_config(_path: &PathBuf) -> Result<ComplianceConfig> {
    // In production, load from TOML file
    // For now, return default config
    Ok(ComplianceConfig::default())
}

fn create_default_config() -> String {
    r#"# Semantic Compliance Configuration

[matching]
default_threshold = 0.85
critical_threshold = 0.95
phonetic_matching = true
transliteration = false
max_edit_distance = 3

[risk_scoring]
enable_ml = false

[risk_scoring.weights]
sanctions_match = 10.0
pep_status = 7.0
adverse_media = 5.0
high_risk_jurisdiction = 6.0
unusual_transactions = 8.0
hidden_ownership = 7.5

[risk_scoring.thresholds]
critical = 9.0
high = 7.0
medium = 5.0
low = 3.0

[alerts]
email = []
slack_webhook = ""
sms = []

[alerts.escalation]
critical_immediate = true
critical_notify = []
high_within_minutes = 15
medium_within_hours = 4

[audit]
retention_years = 7
allowed_formats = ["json", "xml", "pdf"]
sign_reports = true
"#.to_string()
}

fn parse_period(period: &str) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, String)> {
    // Parse period like "2024-Q4"
    if period.contains("-Q") {
        let parts: Vec<&str> = period.split("-Q").collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid period format"));
        }

        let year: i32 = parts[0].parse()?;
        let quarter: u32 = parts[1].parse()?;

        let (start_month, end_month) = match quarter {
            1 => (1, 3),
            2 => (4, 6),
            3 => (7, 9),
            4 => (10, 12),
            _ => return Err(anyhow::anyhow!("Invalid quarter")),
        };

        let start = chrono::NaiveDate::from_ymd_opt(year, start_month, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        let end = chrono::NaiveDate::from_ymd_opt(year, end_month,
            if end_month == 12 { 31 } else { 30 })
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();

        Ok((start, end, period.to_string()))
    } else {
        Err(anyhow::anyhow!("Unsupported period format"))
    }
}

fn print_ownership_tree(tree: &OwnershipTree, prefix: &str, depth: usize) {
    if depth > 5 {
        return;
    }

    let ownership_str = if let Some(percent) = tree.ownership_percent {
        format!(" ({:.1}%)", percent)
    } else {
        String::new()
    };

    let ultimate_marker = if tree.ultimate_owner {
        " [ULTIMATE OWNER]".green().bold()
    } else {
        "".normal()
    };

    println!("{}{}{}{}",
        prefix,
        tree.entity_name,
        ownership_str,
        ultimate_marker
    );

    for (i, owner) in tree.owners.iter().enumerate() {
        let is_last = i == tree.owners.len() - 1;
        let new_prefix = format!("{}{}  ",
            prefix,
            if is_last { "└─" } else { "├─" }
        );
        print_ownership_tree(owner, &new_prefix, depth + 1);
    }
}
