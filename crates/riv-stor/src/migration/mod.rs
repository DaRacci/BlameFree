//! SQL migration using SeaORM Schema API.
//!
//! Migrations generate DDL programmatically from EntityModel-derived entities via SchemaBuilder::sync() for all tables.
//! A `_schema_version` table tracks which migrations have been applied.

use sea_orm::{ConnectionTrait, DatabaseConnection, Schema};

use crate::error::Error;

use riv_types::{
    agent::{AgentSessionEntity, AgentTurnEntity, AgentTurnMessageEntity},
    benchmark::{
        golden::GoldenCommentEntity, judge::JudgeVerdictEntity, result::PrResultEntity,
        standalone::BenchmarkEntity,
    },
    cost::{AgentSessionUsageEntity, CacheUsageEntryEntity},
    finding::FindingEntity,
    review::ReviewEntity,
};

const CURRENT_VERSION: i32 = 1;

/// Create the `_schema_version` tracking table if it does not exist.
async fn create_schema_version_table(db: &DatabaseConnection) -> Result<(), Error> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS _schema_version (
            version     INTEGER PRIMARY KEY,
            applied_at  TEXT NOT NULL DEFAULT (datetime('now')),
            description TEXT
        );",
    )
    .await
    .map_err(|e| Error::Migration(format!("failed to create _schema_version table: {e}")))?;
    Ok(())
}

/// Get the current schema version from the database.
/// Returns 0 if no migrations have been applied yet.
async fn get_current_version(db: &DatabaseConnection) -> Result<i32, Error> {
    let sql = "SELECT COALESCE(MAX(version), 0) FROM _schema_version;".to_string();
    let result = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            sql,
        ))
        .await
        .map_err(|e| Error::Migration(format!("failed to query schema version: {e}")))?;

    match result {
        Some(row) => {
            let version: i32 = row.try_get_by_index::<i32>(0).unwrap_or(0);
            Ok(version)
        }
        None => Ok(0),
    }
}

/// Run all pending migrations against the given database connection.
///
/// Creates the `_schema_version` table if it does not exist, then applies
/// any migration whose version is greater than the current schema version.
///
/// This function is idempotent and safe to call multiple times.
pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), Error> {
    create_schema_version_table(db).await?;

    let current_version = get_current_version(db).await?;
    if current_version < CURRENT_VERSION {
        migrate_v1(db).await?;
    }

    Ok(())
}

async fn migrate_v1(db: &DatabaseConnection) -> Result<(), Error> {
    let schema = Schema::new(db.get_database_backend());

    schema
        .builder()
        .register(BenchmarkEntity)
        .register(ReviewEntity)
        .register(AgentSessionEntity)
        .register(AgentTurnEntity)
        .register(AgentTurnMessageEntity)
        .register(PrResultEntity)
        .register(GoldenCommentEntity)
        .register(FindingEntity)
        .register(JudgeVerdictEntity)
        .register(AgentSessionUsageEntity)
        .register(CacheUsageEntryEntity)
        .sync(db)
        .await
        .map_err(|e| Error::Migration(format!("schema sync failed: {e}")))?;

    // Enable foreign keys
    db.execute_unprepared("PRAGMA foreign_keys = ON;")
        .await
        .map_err(|e| Error::Migration(e.to_string()))?;

    // Drop the old `judged_findings` table (empty, never populated, no longer an entity)
    db.execute_unprepared("DROP TABLE IF EXISTS judged_findings;")
        .await
        .map_err(|e| Error::Migration(e.to_string()))?;

    // Create indexes
    let indexes = [
        "CREATE INDEX IF NOT EXISTS idx_golden_comments_pr ON golden_comments(pr_result_id);",
        "CREATE INDEX IF NOT EXISTS idx_findings_pr ON findings(pr_result_id);",
        "CREATE INDEX IF NOT EXISTS idx_agent_sessions_review ON agent_sessions(review_id);",
        "CREATE INDEX IF NOT EXISTS idx_agent_turns_session ON agent_turns(session_id);",
        "CREATE INDEX IF NOT EXISTS idx_agent_turn_messages_turn ON agent_turn_messages(turn_id);",
        "CREATE INDEX IF NOT EXISTS idx_reviews_status ON reviews(status);",
    ];
    for stmt in indexes {
        db.execute_unprepared(stmt)
            .await
            .map_err(|e| Error::Migration(format!("failed to create index: {e}")))?;
    }

    db.execute_unprepared(
        "INSERT INTO _schema_version (version, description) VALUES (1, 'initial schema via SchemaBuilder::sync()')",
    )
    .await
    .map_err(|e| Error::Migration(format!("failed to record schema version: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version_is_valid() {
        assert!(CURRENT_VERSION > 0, "CURRENT_VERSION must be positive");
    }

    /// Smoke test: verify all migrations run against an in-memory database.
    #[tokio::test]
    async fn test_migration_runs_successfully() {
        let db: DatabaseConnection = sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("failed to open in-memory database");

        run_migrations(&db).await.expect("migration should succeed");

        let version = get_current_version(&db).await.unwrap();
        assert_eq!(
            version, CURRENT_VERSION,
            "schema version should match current version"
        );
        let tables = [
            "reviews",
            "benchmarks",
            "pr_results",
            "findings",
            "judge_verdicts",
            "golden_comments",
            "agent_sessions",
            "agent_turns",
            "agent_turn_messages",
            "agent_session_usages",
            "cache_usage_entries",
            "_schema_version",
        ];
        for name in &tables {
            let row = db
                .query_one_raw(sea_orm::Statement::from_string(
                    db.get_database_backend(),
                    format!(
                        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{name}'"
                    ),
                ))
                .await
                .expect("query should succeed");
            let count: i32 = row
                .and_then(|r| r.try_get_by_index::<i32>(0).ok())
                .unwrap_or(0);
            assert_eq!(count, 1, "table '{name}' should exist after migration");
        }

        run_migrations(&db)
            .await
            .expect("second run should succeed");
        let version = get_current_version(&db).await.unwrap();
        assert_eq!(
            version, CURRENT_VERSION,
            "version should remain unchanged after re-run"
        );
    }
}
