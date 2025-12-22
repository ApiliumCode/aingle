use deep_context::models::{Alternative, DecisionQuery, DecisionStatus};
use deep_context::DeepContext;
use std::fs;
use tempfile::TempDir;

fn create_test_repo() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path().to_path_buf();

    // Initialize a Git repo
    let repo = git2::Repository::init(&repo_path).unwrap();

    // Configure Git
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();

    // Create an initial commit
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap();

    (temp, repo_path)
}

#[test]
fn test_init_and_open() {
    let (_temp, repo_path) = create_test_repo();

    // Initialize Deep Context
    {
        let deep_context = DeepContext::init(repo_path.clone()).unwrap();
        assert!(deep_context.context_dir.exists());
        assert!(deep_context.context_dir.join("config.toml").exists());
    } // Drop deep_context to release database lock

    // Should be able to open it again
    let opened = DeepContext::open(repo_path).unwrap();
    assert!(opened.context_dir.exists());
}

#[test]
fn test_capture_decision() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path).unwrap();

    // Capture a decision
    let decision = deep_context
        .capture_decision(
            "Test Decision".to_string(),
            "This is the context".to_string(),
            "This is the decision".to_string(),
            "This is the rationale".to_string(),
            vec![Alternative::new("Alternative 1".to_string())],
            "These are the consequences".to_string(),
            vec!["src/main.rs".to_string()],
            vec!["test".to_string(), "architecture".to_string()],
        )
        .unwrap();

    assert_eq!(decision.id, "ADR-001");
    assert_eq!(decision.title, "Test Decision");
    assert_eq!(decision.status, DecisionStatus::Accepted);
    assert_eq!(decision.tags.len(), 2);
    assert_eq!(decision.alternatives.len(), 1);
}

#[test]
fn test_query_decisions() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path).unwrap();

    // Capture multiple decisions
    deep_context
        .capture_decision(
            "Microservices Architecture".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            vec![],
            "".to_string(),
            vec![],
            vec!["architecture".to_string(), "microservices".to_string()],
        )
        .unwrap();

    deep_context
        .capture_decision(
            "Database Migration".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            vec![],
            "".to_string(),
            vec![],
            vec!["database".to_string()],
        )
        .unwrap();

    // Query all decisions
    let all = deep_context
        .query_decisions(&DecisionQuery::default())
        .unwrap();
    assert_eq!(all.len(), 2);

    // Query by text
    let results = deep_context
        .query_decisions(&DecisionQuery::new().with_text("microservices".to_string()))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Microservices Architecture");

    // Query by tag
    let results = deep_context
        .query_decisions(&DecisionQuery::new().with_tag("architecture".to_string()))
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_decisions_for_file() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path).unwrap();

    // Capture a decision with file references
    deep_context
        .capture_decision(
            "Test Decision".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            vec![],
            "".to_string(),
            vec!["src/auth/handler.rs".to_string()],
            vec![],
        )
        .unwrap();

    // Query decisions for this file
    let results = deep_context
        .decisions_for_file("src/auth/handler.rs")
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "ADR-001");
}

#[test]
fn test_tags() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path).unwrap();

    // Capture decisions with tags
    deep_context
        .capture_decision(
            "Decision 1".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            vec![],
            "".to_string(),
            vec![],
            vec!["tag1".to_string(), "tag2".to_string()],
        )
        .unwrap();

    deep_context
        .capture_decision(
            "Decision 2".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            vec![],
            "".to_string(),
            vec![],
            vec!["tag2".to_string(), "tag3".to_string()],
        )
        .unwrap();

    // Get all tags
    let tags = deep_context.all_tags().unwrap();
    assert_eq!(tags.len(), 3);
    assert!(tags.contains(&"tag1".to_string()));
    assert!(tags.contains(&"tag2".to_string()));
    assert!(tags.contains(&"tag3".to_string()));

    // Get decisions by tag
    let results = deep_context.decisions_by_tag("tag2").unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_export_markdown() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path.clone()).unwrap();

    // Capture a decision
    deep_context
        .capture_decision(
            "Test Decision".to_string(),
            "This is the context".to_string(),
            "This is the decision".to_string(),
            "This is the rationale".to_string(),
            vec![
                Alternative::new("Alternative 1".to_string())
                    .with_pros(vec!["Pro 1".to_string()])
                    .with_cons(vec!["Con 1".to_string()])
                    .with_rejection_reason("Not suitable".to_string()),
            ],
            "These are the consequences".to_string(),
            vec!["src/main.rs".to_string()],
            vec!["test".to_string()],
        )
        .unwrap();

    // Export to markdown
    let output_dir = repo_path.join("docs");
    deep_context.export_markdown(&output_dir).unwrap();

    // Check that files were created
    assert!(output_dir.join("ADR-001.md").exists());
    assert!(output_dir.join("README.md").exists());

    // Read the markdown file
    let content = fs::read_to_string(output_dir.join("ADR-001.md")).unwrap();
    assert!(content.contains("# ADR-001: Test Decision"));
    assert!(content.contains("## Context"));
    assert!(content.contains("## Decision"));
    assert!(content.contains("## Rationale"));
    assert!(content.contains("## Alternatives Considered"));
    assert!(content.contains("## Consequences"));
}

#[test]
fn test_statistics() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path).unwrap();

    // Initially empty
    let stats = deep_context.statistics().unwrap();
    assert_eq!(stats.total_decisions, 0);

    // Capture some decisions
    for i in 1..=5 {
        deep_context
            .capture_decision(
                format!("Decision {}", i),
                "Context".to_string(),
                "Decision".to_string(),
                "Rationale".to_string(),
                vec![],
                "".to_string(),
                vec![],
                vec!["tag".to_string()],
            )
            .unwrap();
    }

    // Check statistics
    let stats = deep_context.statistics().unwrap();
    assert_eq!(stats.total_decisions, 5);
    assert_eq!(stats.total_tags, 1);
}

#[test]
fn test_next_decision_id() {
    let (_temp, repo_path) = create_test_repo();
    let mut deep_context = DeepContext::init(repo_path).unwrap();

    // First ID should be ADR-001
    let id1 = deep_context.next_decision_id().unwrap();
    assert_eq!(id1, "ADR-001");

    // Capture a decision
    deep_context
        .capture_decision(
            "Decision 1".to_string(),
            "Context".to_string(),
            "Decision".to_string(),
            "Rationale".to_string(),
            vec![],
            "".to_string(),
            vec![],
            vec![],
        )
        .unwrap();

    // Next ID should be ADR-002
    let id2 = deep_context.next_decision_id().unwrap();
    assert_eq!(id2, "ADR-002");
}
