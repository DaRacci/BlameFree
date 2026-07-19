//! Integration tests for riv-stor.
//!
//! Tests verify the full save/load round-trip for each domain type,
//! multi-table cascade persistence, WAL journal mode, and FK cascade deletes.

use std::collections::HashMap;

use chrono::Utc;
use crb_types::{
    agent::{AgentResponse, AgentSession, RoleMessage, ToolInvocation},
    benchmark::{golden::GoldenComment, result::PrResult, standalone::Benchmark},
    review::{Review, ReviewStatus},
    wrappers::Model,
};
use mti::prelude::MagicTypeId;
use riv_stor::store::SqliteStore;
use riv_stor::traits::Store;
use sea_orm::ConnectionTrait;

/// create an in-memory SqliteStore with migrations applied.
async fn make_store() -> SqliteStore {
    SqliteStore::new(":memory:").await.unwrap()
}

/// create a deterministic MagicTypeId from a u64.
fn make_id(n: u64) -> MagicTypeId {
    format!("test-id-{n}")
        .parse::<MagicTypeId>()
        .unwrap_or_default()
}

#[tokio::test]
async fn test_store_creation_and_migration() {
    // Use a temp file to verify WAL mode (in-memory DBs always report "memory")
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let store = SqliteStore::new(tmp.path().to_str().unwrap())
        .await
        .unwrap();

    let row = store
        .connection()
        .query_one_raw(sea_orm::Statement::from_string(
            store.connection().get_database_backend(),
            "PRAGMA journal_mode;".to_string(),
        ))
        .await
        .unwrap();
    let mode: String = row.unwrap().try_get_by_index::<String>(0).unwrap();
    assert_eq!(
        mode.to_lowercase(),
        "wal",
        "WAL journal mode should be enabled"
    );
}

#[tokio::test]
async fn test_review_round_trip() {
    let store = make_store().await;

    let id = make_id(1);
    let review = Review {
        id: id.clone(),
        agent_sessions: HashMap::new(),
        analytics: None,
        duration: None,
        status: ReviewStatus::Running,
        metadata: crb_types::review::ReviewMetadata::Plain,
    };

    store.save(&review).await.unwrap();
    let loaded: Review = store.load(&id).await.unwrap().expect("review should exist");

    assert_eq!(loaded.id.to_string(), id.to_string());
    assert_eq!(loaded.status, ReviewStatus::Running);
    assert!(loaded.agent_sessions.is_empty());
}

#[tokio::test]
async fn test_benchmark_round_trip() {
    let store = make_store().await;

    let id = make_id(10);
    let now = Utc::now().naive_utc();
    let benchmark = Benchmark {
        id: id.clone(),
        dataset_name: "test-dataset".to_string(),
        dataset_version: Some("v1.0".to_string()),
        created_at: now,
        updated_at: now,
    };

    store.save(&benchmark).await.unwrap();
    let loaded: Benchmark = store
        .load(&id)
        .await
        .unwrap()
        .expect("benchmark should exist");

    assert_eq!(loaded.id.to_string(), id.to_string());
    assert_eq!(loaded.dataset_name, "test-dataset");
    assert_eq!(loaded.dataset_version, Some("v1.0".to_string()));
    assert_eq!(loaded.created_at, now);
    assert_eq!(loaded.updated_at, now);
}

#[tokio::test]
async fn test_pr_result_with_golden_comments() {
    let store = make_store().await;

    let id = make_id(20);

    let gc1 = GoldenComment {
        id: None,
        pr_result_id: id.clone(),
        comment: "Expected comment 1".to_string(),
        severity: crb_types::severity::Severity::High,
    };
    let gc2 = GoldenComment {
        id: None,
        pr_result_id: id.clone(),
        comment: "Expected comment 2".to_string(),
        severity: crb_types::severity::Severity::Low,
    };

    let pr = PrResult {
        id: id.clone(),
        golden_comments: vec![gc1, gc2],
        metrics: Default::default(),
        findings_with_verdicts: Vec::new(),
        cost: Default::default(),
    };

    store.save(&pr).await.unwrap();
    let loaded: PrResult = store
        .load(&id)
        .await
        .unwrap()
        .expect("pr_result should exist");

    assert_eq!(loaded.id, id);
    assert_eq!(loaded.golden_comments.len(), 2);
    let comments: Vec<&str> = loaded
        .golden_comments
        .iter()
        .map(|g| g.comment.as_str())
        .collect();
    assert!(
        comments.contains(&"Expected comment 1"),
        "missing comment 1"
    );
    assert!(
        comments.contains(&"Expected comment 2"),
        "missing comment 2"
    );
    let found_gc1 = loaded
        .golden_comments
        .iter()
        .find(|g| g.comment == "Expected comment 1")
        .unwrap();
    let found_gc2 = loaded
        .golden_comments
        .iter()
        .find(|g| g.comment == "Expected comment 2")
        .unwrap();
    assert_eq!(found_gc1.severity, crb_types::severity::Severity::High);
    assert_eq!(found_gc2.severity, crb_types::severity::Severity::Low);
}

#[tokio::test]
async fn test_agent_session_round_trip() {
    let store = make_store().await;

    let id = make_id(30);

    // Build a session with 2 turns, each with multiple messages
    let session = AgentSession {
        id: id.clone(),
        model: Model("spongebob".to_string()),
        turns: vec![
            vec![
                RoleMessage::User("Hello, review this PR".to_string()),
                RoleMessage::Assistant(AgentResponse {
                    thinking: "Thinking about security issues...".to_string(),
                    output: "I'll check the code for vulnerabilities.".to_string(),
                }),
                RoleMessage::Tool(ToolInvocation {
                    tool_name: "view_file".to_string(),
                    input: serde_json::json!({"path": "src/main.rs"}),
                    output: serde_json::json!({"content": "fn main() {}"}),
                }),
            ],
            vec![
                RoleMessage::System("You are a security reviewer.".to_string()),
                RoleMessage::User("Focus on auth issues.".to_string()),
                RoleMessage::Assistant(AgentResponse {
                    thinking: "Checking authentication logic...".to_string(),
                    output: "Found no auth issues in this file.".to_string(),
                }),
            ],
        ],
    };

    store.save(&session).await.unwrap();
    let loaded: AgentSession = store
        .load(&id)
        .await
        .unwrap()
        .expect("session should exist");

    assert_eq!(loaded.id, id);
    assert_eq!(loaded.model.0, "spongebob");
    assert_eq!(loaded.turns.len(), 2, "should have 2 turns");

    assert_eq!(loaded.turns[0].len(), 3, "turn 0 should have 3 messages");
    match &loaded.turns[0][0] {
        RoleMessage::User(text) => assert_eq!(text, "Hello, review this PR"),
        other => panic!("expected User message, got {other:?}"),
    }
    match &loaded.turns[0][1] {
        RoleMessage::Assistant(resp) => {
            assert_eq!(resp.thinking, "Thinking about security issues...");
            assert_eq!(resp.output, "I'll check the code for vulnerabilities.");
        }
        other => panic!("expected Assistant message, got {other:?}"),
    }
    match &loaded.turns[0][2] {
        RoleMessage::Tool(invocation) => {
            assert_eq!(invocation.tool_name, "view_file");
        }
        other => panic!("expected Tool message, got {other:?}"),
    }

    assert_eq!(loaded.turns[1].len(), 3, "turn 1 should have 3 messages");
    match &loaded.turns[1][0] {
        RoleMessage::System(text) => assert_eq!(text, "You are a security reviewer."),
        other => panic!("expected System message, got {other:?}"),
    }
    match &loaded.turns[1][1] {
        RoleMessage::User(text) => assert_eq!(text, "Focus on auth issues."),
        other => panic!("expected User message, got {other:?}"),
    }
    match &loaded.turns[1][2] {
        RoleMessage::Assistant(resp) => {
            assert_eq!(resp.thinking, "Checking authentication logic...");
            assert_eq!(resp.output, "Found no auth issues in this file.");
        }
        other => panic!("expected Assistant message, got {other:?}"),
    }
}

#[tokio::test]
async fn test_pr_result_cascade_delete() {
    let store = make_store().await;

    let id = make_id(40);
    let gc = GoldenComment {
        id: Some(make_id(401)),
        pr_result_id: id.clone(),
        comment: "Cascade test comment".to_string(),
        severity: crb_types::severity::Severity::Medium,
    };

    let pr = PrResult {
        id: id.clone(),
        golden_comments: vec![gc],
        metrics: Default::default(),
        findings_with_verdicts: Vec::new(),
        cost: Default::default(),
    };

    store.save(&pr).await.unwrap();

    let result: PrResult = store.load(&id).await.unwrap().unwrap();
    assert_eq!(result.golden_comments.len(), 1);

    let deleted = store.delete::<PrResult>(&id).await.unwrap();
    assert!(deleted, "pr_result should be deleted");

    let result: Option<PrResult> = store.load(&id).await.unwrap();
    assert!(result.is_none(), "pr_result should be gone after delete");
}

#[tokio::test]
async fn test_agent_session_delete() {
    let store = make_store().await;

    let id = make_id(50);
    let session = AgentSession {
        id: id.clone(),
        model: Model("patrick".to_string()),
        turns: vec![vec![RoleMessage::User("test".to_string())]],
    };

    store.save(&session).await.unwrap();

    let loaded: Option<AgentSession> = store.load(&id).await.unwrap();
    assert!(loaded.is_some(), "session should exist after save");

    let deleted = store.delete::<AgentSession>(&id).await.unwrap();
    assert!(deleted, "session should be deleted");

    let loaded: Option<AgentSession> = store.load(&id).await.unwrap();
    assert!(loaded.is_none(), "session should be gone after delete");
}
