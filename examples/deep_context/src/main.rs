use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use colored::*;
use deep_context::models::{Alternative, DecisionQuery};
use deep_context::DeepContext;
use dialoguer::{Confirm, Input, MultiSelect};
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "deep-context")]
#[command(about = "Deep Context - Semantic Git that captures the 'why' behind code decisions", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Repository path (defaults to current directory)
    #[arg(short, long, global = true)]
    repo: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize deep-context in a repository
    Init,

    /// Capture an architectural decision
    Capture {
        /// Decision title
        #[arg(long)]
        title: Option<String>,

        /// Context that led to the decision
        #[arg(long)]
        context: Option<String>,

        /// What was decided
        #[arg(long)]
        decision: Option<String>,

        /// Why this decision was made
        #[arg(long)]
        rationale: Option<String>,

        /// Alternative approaches (can be specified multiple times)
        #[arg(long)]
        alternative: Vec<String>,

        /// Expected consequences
        #[arg(long)]
        consequence: Option<String>,

        /// Related files (can be specified multiple times)
        #[arg(long = "files")]
        files: Vec<String>,

        /// Tags (can be specified multiple times)
        #[arg(long = "tag")]
        tags: Vec<String>,

        /// Use interactive mode
        #[arg(short, long)]
        interactive: bool,
    },

    /// Query past decisions
    Query {
        /// Free-text search query
        query: Option<String>,

        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Filter by file
        #[arg(long)]
        file: Option<String>,

        /// Filter by author
        #[arg(long)]
        author: Option<String>,

        /// Date range - start
        #[arg(long)]
        since: Option<String>,

        /// Date range - end
        #[arg(long)]
        until: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Show decision timeline
    Timeline {
        /// File path to show timeline for
        file: Option<String>,

        /// Decision ID to show timeline for
        #[arg(long)]
        decision: Option<String>,

        /// Show visual ASCII timeline
        #[arg(long)]
        visual: bool,
    },

    /// Export knowledge base
    Export {
        /// Output format (markdown, json, rdf)
        #[arg(long, default_value = "markdown")]
        format: String,

        /// Output directory or file
        #[arg(long)]
        output: PathBuf,
    },

    /// Show statistics about the knowledge base
    Stats,

    /// List all tags
    Tags,

    /// Show details of a specific decision
    Show {
        /// Decision ID (e.g., ADR-001)
        id: String,
    },

    /// Link a commit to a decision (used by Git hooks)
    LinkCommit {
        /// Decision ID
        decision_id: String,

        /// Commit reference (default: HEAD)
        #[arg(default_value = "HEAD")]
        commit_ref: String,
    },

    /// Suggest decisions for changed files (used by Git hooks)
    SuggestDecisions {
        /// Files to check
        files: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logger
    if cli.verbose {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();
    }

    let repo_path = cli.repo.unwrap_or_else(|| env::current_dir().unwrap());

    match cli.command {
        Commands::Init => cmd_init(repo_path),
        Commands::Capture {
            title,
            context,
            decision,
            rationale,
            alternative,
            consequence,
            files,
            tags,
            interactive,
        } => {
            if interactive {
                cmd_capture_interactive(repo_path)
            } else {
                cmd_capture(
                    repo_path,
                    title,
                    context,
                    decision,
                    rationale,
                    alternative,
                    consequence,
                    files,
                    tags,
                )
            }
        }
        Commands::Query {
            query,
            tag,
            file,
            author,
            since,
            until,
            limit,
        } => cmd_query(repo_path, query, tag, file, author, since, until, limit),
        Commands::Timeline {
            file,
            decision,
            visual,
        } => cmd_timeline(repo_path, file, decision, visual),
        Commands::Export { format, output } => cmd_export(repo_path, format, output),
        Commands::Stats => cmd_stats(repo_path),
        Commands::Tags => cmd_tags(repo_path),
        Commands::Show { id } => cmd_show(repo_path, id),
        Commands::LinkCommit {
            decision_id,
            commit_ref,
        } => cmd_link_commit(repo_path, decision_id, commit_ref),
        Commands::SuggestDecisions { files } => cmd_suggest_decisions(repo_path, files),
    }
}

fn cmd_init(repo_path: PathBuf) -> Result<()> {
    println!("{}", "Initializing Deep Context...".cyan().bold());

    let _deep_context = DeepContext::init(repo_path)?;

    println!("{}", "✓ Deep Context initialized successfully!".green().bold());
    println!();
    println!("What's next:");
    println!("  - Capture your first decision: {}", "deep-context capture --interactive".yellow());
    println!("  - Query decisions: {}", "deep-context query \"your search\"".yellow());
    println!("  - View statistics: {}", "deep-context stats".yellow());

    Ok(())
}

fn cmd_capture(
    repo_path: PathBuf,
    title: Option<String>,
    context: Option<String>,
    decision: Option<String>,
    rationale: Option<String>,
    alternatives: Vec<String>,
    consequence: Option<String>,
    files: Vec<String>,
    tags: Vec<String>,
) -> Result<()> {
    let mut deep_context = DeepContext::open(repo_path)?;

    let title = title.context("--title is required (or use --interactive)")?;
    let context = context.context("--context is required (or use --interactive)")?;
    let decision = decision.context("--decision is required (or use --interactive)")?;
    let rationale = rationale.context("--rationale is required (or use --interactive)")?;

    let alternatives: Vec<Alternative> = alternatives
        .into_iter()
        .map(|desc| Alternative::new(desc))
        .collect();

    let consequences = consequence.unwrap_or_default();

    let adr = deep_context.capture_decision(
        title,
        context,
        decision,
        rationale,
        alternatives,
        consequences,
        files,
        tags,
    )?;

    println!("{}", "✓ Decision captured successfully!".green().bold());
    println!();
    print_decision(&adr);

    Ok(())
}

fn cmd_capture_interactive(repo_path: PathBuf) -> Result<()> {
    let mut deep_context = DeepContext::open(repo_path)?;

    println!("{}", "Capturing Architectural Decision".cyan().bold());
    println!();

    let title: String = Input::new()
        .with_prompt("Decision Title")
        .interact_text()?;

    let context: String = Input::new()
        .with_prompt("Context (What situation led to this decision?)")
        .interact_text()?;

    let decision: String = Input::new()
        .with_prompt("Decision (What was decided?)")
        .interact_text()?;

    let rationale: String = Input::new()
        .with_prompt("Rationale (Why was this decided?)")
        .interact_text()?;

    // Alternatives
    let mut alternatives = Vec::new();
    loop {
        if !alternatives.is_empty() {
            let add_more = Confirm::new()
                .with_prompt("Add another alternative?")
                .default(false)
                .interact()?;

            if !add_more {
                break;
            }
        } else {
            let has_alternatives = Confirm::new()
                .with_prompt("Were there alternative approaches?")
                .default(true)
                .interact()?;

            if !has_alternatives {
                break;
            }
        }

        let alt_desc: String = Input::new()
            .with_prompt("Alternative description")
            .interact_text()?;

        alternatives.push(Alternative::new(alt_desc));
    }

    let consequences: String = Input::new()
        .with_prompt("Consequences (Expected impact and trade-offs)")
        .allow_empty(true)
        .interact_text()?;

    // Tags
    let available_tags = deep_context.all_tags().unwrap_or_default();
    let mut tags = Vec::new();

    if !available_tags.is_empty() {
        println!("Select tags (use space to select, enter to confirm):");
        let selections = MultiSelect::new()
            .items(&available_tags)
            .interact()?;

        for idx in selections {
            tags.push(available_tags[idx].clone());
        }
    }

    let add_new_tag = Confirm::new()
        .with_prompt("Add new tags?")
        .default(false)
        .interact()?;

    if add_new_tag {
        let new_tags: String = Input::new()
            .with_prompt("Tags (comma-separated)")
            .allow_empty(true)
            .interact_text()?;

        for tag in new_tags.split(',') {
            let tag = tag.trim().to_string();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }

    // Files
    let files_input: String = Input::new()
        .with_prompt("Related files (comma-separated paths or patterns)")
        .allow_empty(true)
        .interact_text()?;

    let files: Vec<String> = files_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let adr = deep_context.capture_decision(
        title,
        context,
        decision,
        rationale,
        alternatives,
        consequences,
        files,
        tags,
    )?;

    println!();
    println!("{}", "✓ Decision captured successfully!".green().bold());
    println!();
    print_decision(&adr);

    Ok(())
}

fn cmd_query(
    repo_path: PathBuf,
    text: Option<String>,
    tag: Option<String>,
    file: Option<String>,
    author: Option<String>,
    since: Option<String>,
    until: Option<String>,
    limit: usize,
) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    let mut query = DecisionQuery::new().with_limit(limit);

    if let Some(text) = text {
        query = query.with_text(text);
    }

    if let Some(tag) = tag {
        query = query.with_tag(tag);
    }

    if let Some(file) = file {
        query = query.with_file(file);
    }

    if let Some(author) = author {
        query = query.with_author(author);
    }

    // Parse dates
    let since_date = since
        .map(|s| s.parse::<DateTime<Utc>>())
        .transpose()
        .ok()
        .flatten();

    let until_date = until
        .map(|s| s.parse::<DateTime<Utc>>())
        .transpose()
        .ok()
        .flatten();

    query = query.with_date_range(since_date, until_date);

    let results = deep_context.query_decisions(&query)?;

    if results.is_empty() {
        println!("{}", "No decisions found matching your query.".yellow());
        return Ok(());
    }

    println!(
        "{}",
        format!("Found {} decision(s):", results.len())
            .cyan()
            .bold()
    );
    println!();

    for decision in results {
        print_decision_summary(&decision);
        println!();
    }

    Ok(())
}

fn cmd_timeline(
    repo_path: PathBuf,
    file: Option<String>,
    decision_id: Option<String>,
    visual: bool,
) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    let decisions = if let Some(decision_id) = decision_id {
        vec![deep_context
            .get_decision(&decision_id)?
            .context("Decision not found")?]
    } else if let Some(file) = file {
        deep_context.decisions_for_file(&file)?
    } else {
        deep_context.query_decisions(&DecisionQuery::default())?
    };

    if decisions.is_empty() {
        println!("{}", "No decisions found.".yellow());
        return Ok(());
    }

    let timeline = deep_context.git.decision_timeline(&decisions)?;

    if visual {
        print_visual_timeline(&timeline);
    } else {
        print_text_timeline(&timeline);
    }

    Ok(())
}

fn cmd_export(repo_path: PathBuf, format: String, output: PathBuf) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    match format.as_str() {
        "markdown" | "md" => {
            deep_context.export_markdown(&output)?;
            println!(
                "{}",
                format!("✓ Exported decisions to {:?}", output)
                    .green()
                    .bold()
            );
        }
        "json" => {
            let decisions = deep_context.query_decisions(&DecisionQuery::default())?;
            let json = serde_json::to_string_pretty(&decisions)?;
            std::fs::write(&output, json)?;
            println!(
                "{}",
                format!("✓ Exported decisions to {:?}", output)
                    .green()
                    .bold()
            );
        }
        _ => {
            anyhow::bail!("Unsupported format: {}. Use 'markdown' or 'json'", format);
        }
    }

    Ok(())
}

fn cmd_stats(repo_path: PathBuf) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    let stats = deep_context.statistics()?;

    println!("{}", "Knowledge Base Statistics".cyan().bold());
    println!();
    println!("  Total Decisions:     {}", stats.total_decisions);
    println!("  Code Contexts:       {}", stats.total_code_contexts);
    println!("  Linked Commits:      {}", stats.total_commits);
    println!("  Unique Tags:         {}", stats.total_tags);
    println!("  Files Referenced:    {}", stats.total_files);

    Ok(())
}

fn cmd_tags(repo_path: PathBuf) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    let tags = deep_context.all_tags()?;

    if tags.is_empty() {
        println!("{}", "No tags found.".yellow());
        return Ok(());
    }

    println!("{}", "Available Tags:".cyan().bold());
    println!();

    for tag in tags {
        let count = deep_context.decisions_by_tag(&tag)?.len();
        println!("  {} ({})", tag.yellow(), count);
    }

    Ok(())
}

fn cmd_show(repo_path: PathBuf, id: String) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    let decision = deep_context
        .get_decision(&id)?
        .context("Decision not found")?;

    print_decision_detailed(&decision);

    Ok(())
}

fn cmd_link_commit(repo_path: PathBuf, decision_id: String, commit_ref: String) -> Result<()> {
    let mut deep_context = DeepContext::open(repo_path)?;

    let linked = deep_context
        .git
        .link_commit_to_decision(&decision_id, &commit_ref)?;

    deep_context.index.store_commit(linked)?;

    println!(
        "{}",
        format!("✓ Linked commit to {}", decision_id).green().bold()
    );

    Ok(())
}

fn cmd_suggest_decisions(repo_path: PathBuf, files: Vec<String>) -> Result<()> {
    let deep_context = DeepContext::open(repo_path)?;

    for file in files {
        let decisions = deep_context.decisions_for_file(&file)?;

        for decision in decisions {
            println!("  {} - {}", decision.id, decision.title);
        }
    }

    Ok(())
}

// Printing helpers

fn print_decision(decision: &deep_context::models::ArchitecturalDecision) {
    println!("╭{:─<60}╮", "");
    println!("│ {:<58} │", format!("{}: {}", decision.id, decision.title).bold());
    println!(
        "│ {:<58} │",
        format!(
            "Date: {} | Author: {}",
            decision.timestamp.format("%Y-%m-%d"),
            decision.author
        )
    );
    println!("├{:─<60}┤", "");
    println!(
        "│ {:<58} │",
        format!("Status: {}", decision.status.as_str().green())
    );

    if !decision.tags.is_empty() {
        println!(
            "│ {:<58} │",
            format!("Tags: {}", decision.tags.join(", ").yellow())
        );
    }

    if !decision.related_files.is_empty() {
        println!(
            "│ {:<58} │",
            format!("Files: {}", decision.related_files.len())
        );
    }

    println!("╰{:─<60}╯", "");
}

fn print_decision_summary(decision: &deep_context::models::ArchitecturalDecision) {
    println!("╭{:─<60}╮", "");
    println!("│ {:<58} │", format!("{}: {}", decision.id, decision.title).bold());
    println!(
        "│ {:<58} │",
        format!(
            "Date: {} | Author: {}",
            decision.timestamp.format("%Y-%m-%d"),
            decision.author.split('@').next().unwrap_or(&decision.author)
        )
    );
    println!("├{:─<60}┤", "");

    // Truncate context for summary
    let context_preview = if decision.context.len() > 54 {
        format!("{}...", &decision.context[..51])
    } else {
        decision.context.clone()
    };
    println!("│ {:<58} │", format!("Context: {}", context_preview));

    println!(
        "│ {:<58} │",
        format!("Status: {}", decision.status.as_str())
    );

    if !decision.tags.is_empty() {
        println!(
            "│ {:<58} │",
            format!("Tags: {}", decision.tags.join(", "))
        );
    }

    println!("╰{:─<60}╯", "");
}

fn print_decision_detailed(decision: &deep_context::models::ArchitecturalDecision) {
    println!("{}", format!("# {}: {}", decision.id, decision.title).cyan().bold());
    println!();
    println!("Status:    {}", decision.status.as_str().green());
    println!("Date:      {}", decision.timestamp.format("%Y-%m-%d %H:%M:%S"));
    println!("Author:    {}", decision.author);

    if !decision.tags.is_empty() {
        println!("Tags:      {}", decision.tags.join(", ").yellow());
    }

    println!();
    println!("{}", "Context:".bold());
    println!("{}", decision.context);

    println!();
    println!("{}", "Decision:".bold());
    println!("{}", decision.decision);

    println!();
    println!("{}", "Rationale:".bold());
    println!("{}", decision.rationale);

    if !decision.alternatives.is_empty() {
        println!();
        println!("{}", "Alternatives Considered:".bold());
        for (i, alt) in decision.alternatives.iter().enumerate() {
            println!("  {}. {}", i + 1, alt.description);
        }
    }

    if !decision.consequences.is_empty() {
        println!();
        println!("{}", "Consequences:".bold());
        println!("{}", decision.consequences);
    }

    if !decision.related_files.is_empty() {
        println!();
        println!("{}", "Related Files:".bold());
        for file in &decision.related_files {
            println!("  - {}", file);
        }
    }
}

fn print_text_timeline(timeline: &[deep_context::git_integration::TimelineEvent]) {
    println!("{}", "Decision Timeline".cyan().bold());
    println!();

    for event in timeline {
        let date = event.timestamp.format("%Y-%m-%d").to_string();
        let event_type = match event.event_type {
            deep_context::git_integration::TimelineEventType::DecisionMade => "Decision Made".green(),
            deep_context::git_integration::TimelineEventType::DecisionImplemented => {
                "Implemented".blue()
            }
            deep_context::git_integration::TimelineEventType::DecisionSuperseded => {
                "Superseded".yellow()
            }
        };

        println!(
            "{} │ {} │ {}",
            date.cyan(),
            event_type,
            event.title
        );

        if let Some(decision_id) = &event.decision_id {
            println!("          │ Decision: {}", decision_id.yellow());
        }

        if let Some(commit) = &event.commit_hash {
            println!("          │ Commit: {}", &commit[..8]);
        }

        println!();
    }
}

fn print_visual_timeline(timeline: &[deep_context::git_integration::TimelineEvent]) {
    println!("{}", "Visual Timeline".cyan().bold());
    println!();

    let mut current_month: Option<String> = None;

    for event in timeline {
        let month = event.timestamp.format("%Y-%m").to_string();

        if current_month.as_ref() != Some(&month) {
            println!("{} │", month.cyan());
            current_month = Some(month);
        } else {
            println!("       │");
        }

        let symbol = match event.event_type {
            deep_context::git_integration::TimelineEventType::DecisionMade => "◆",
            deep_context::git_integration::TimelineEventType::DecisionImplemented => "├─→",
            deep_context::git_integration::TimelineEventType::DecisionSuperseded => "✕",
        };

        println!(
            "       {} {} {}",
            symbol.green(),
            event.decision_id.as_ref().unwrap_or(&"".to_string()).yellow(),
            event.title
        );
    }
}
