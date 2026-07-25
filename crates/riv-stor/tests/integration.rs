//! Integration tests for riv-stor.
//!
//! Tests verify the full save/load round-trip for each domain type,
//! multi-table cascade persistence, WAL journal mode, and FK cascade deletes.

use std::collections::HashMap;

use chrono::Utc;
use crb_types::{
    agent::{
        AgentResponse, AgentSession, AgentTurn, AgentTurnMessage, RoleMessage, ToolInvocation,
    },
    benchmark::{golden::GoldenComment, result::PrResult, standalone::Benchmark},
    review::{Review, ReviewStatus},
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
        benchmark_id: None,
        findings_with_verdicts: Vec::new(),
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
        model_name: "spongebob".to_string(),
        review_id: None,
        turns: vec![
            AgentTurn {
                id: None,
                session_id: id.clone(),
                turn_index: 0,
                messages: vec![
                    RoleMessage::User("Hello, review this PR".to_string()).into(),
                    RoleMessage::Assistant(AgentResponse {
                        thinking: "Thinking about security issues...".to_string(),
                        output: "I'll check the code for vulnerabilities.".to_string(),
                    })
                    .into(),
                    RoleMessage::Tool(ToolInvocation {
                        tool_name: "view_file".to_string(),
                        input: serde_json::json!({"path": "src/main.rs"}),
                        output: serde_json::json!({"content": "fn main() {}"}),
                    })
                    .into(),
                ],
            },
            AgentTurn {
                id: None,
                session_id: id.clone(),
                turn_index: 1,
                messages: vec![
                    RoleMessage::System("You are a security reviewer.".to_string()).into(),
                    RoleMessage::User("Focus on auth issues.".to_string()).into(),
                    RoleMessage::Assistant(AgentResponse {
                        thinking: "Checking authentication logic...".to_string(),
                        output: "Found no auth issues in this file.".to_string(),
                    })
                    .into(),
                ],
            },
        ],
    };

    store.save(&session).await.unwrap();
    let loaded: AgentSession = store
        .load(&id)
        .await
        .unwrap()
        .expect("session should exist");

    assert_eq!(loaded.id, id);
    assert_eq!(loaded.model_name, "spongebob");
    assert_eq!(loaded.turns.len(), 2, "should have 2 turns");
    let turn0 = &loaded.turns[0];
    let turn1 = &loaded.turns[1];
    assert_eq!(turn0.messages.len(), 3, "turn 0 should have 3 messages");
    assert_eq!(&turn0.messages[0].role, "user");
    assert_eq!(
        turn0.messages[0].text_content.as_deref(),
        Some("Hello, review this PR")
    );
    assert_eq!(&turn0.messages[1].role, "assistant");
    assert_eq!(
        turn0.messages[1].thinking.as_deref(),
        Some("Thinking about security issues...")
    );
    assert_eq!(
        turn0.messages[1].output.as_deref(),
        Some("I'll check the code for vulnerabilities.")
    );
    assert_eq!(&turn0.messages[2].role, "tool");
    assert_eq!(turn0.messages[2].tool_name.as_deref(), Some("view_file"));

    assert_eq!(turn1.messages.len(), 3, "turn 1 should have 3 messages");
    assert_eq!(&turn1.messages[0].role, "system");
    assert_eq!(
        turn1.messages[0].text_content.as_deref(),
        Some("You are a security reviewer.")
    );
    assert_eq!(&turn1.messages[1].role, "user");
    assert_eq!(
        turn1.messages[1].text_content.as_deref(),
        Some("Focus on auth issues.")
    );
    assert_eq!(&turn1.messages[2].role, "assistant");
    assert_eq!(
        turn1.messages[2].thinking.as_deref(),
        Some("Checking authentication logic...")
    );
    assert_eq!(
        turn1.messages[2].output.as_deref(),
        Some("Found no auth issues in this file.")
    );
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
        benchmark_id: None,
        findings_with_verdicts: Vec::new(),
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
        model_name: "patrick".to_string(),
        review_id: None,
        turns: vec![AgentTurn {
            id: None,
            session_id: id.clone(),
            turn_index: 0,
            messages: vec![AgentTurnMessage {
                role: "user".into(),
                text_content: Some("test".to_string()),
                ..Default::default()
            }],
        }],
    };

    store.save(&session).await.unwrap();

    let loaded: Option<AgentSession> = store.load(&id).await.unwrap();
    assert!(loaded.is_some(), "session should exist after save");

    let deleted = store.delete::<AgentSession>(&id).await.unwrap();
    assert!(deleted, "session should be deleted");

    let loaded: Option<AgentSession> = store.load(&id).await.unwrap();
    assert!(loaded.is_none(), "session should be gone after delete");
}
