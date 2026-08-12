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
    finding::FindingEntity,
    review::ReviewEntity,
    settings::AllowedModelEntity,
};

#[allow(dead_code)]
const CURRENT_VERSION: i32 = 5;

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
/// Creates the `_schema_version` table if it does not exist,
/// then applies any migration whose version is greater than the current schema version.
///
/// This function is idempotent and safe to call multiple times.
pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), Error> {
    create_schema_version_table(db).await?;

    let current_version = get_current_version(db).await?;
    if current_version < 1 {
        migrate_v1(db).await?;
    }
    if current_version < 2 {
        migrate_v2(db).await?;
    }
    if current_version < 3 {
        migrate_v3(db).await?;
    }
    if current_version < 4 {
        migrate_v4(db).await?;
    }
    if current_version < 5 {
        migrate_v5(db).await?;
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
        .register(AllowedModelEntity)
        .sync(db)
        .await
        .map_err(|e| Error::Migration(format!("schema sync failed: {e}")))?;

    db.execute_unprepared("PRAGMA foreign_keys = ON;")
        .await
        .map_err(|e| Error::Migration(e.to_string()))?;

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

/// Add the `analytics` BLOB column to `reviews` to persist the compressed
/// JSON blob (analytics + duration) that was previously only written to disk.
///
/// `SchemaBuilder::sync()` will not ALTER an existing table, so this is an
/// explicit `ALTER TABLE`. Idempotency is guaranteed by the schema-version gate
/// in `run_migrations` (this only runs when the store is below version 2).
async fn migrate_v2(db: &DatabaseConnection) -> Result<(), Error> {
    // On a fresh DB, migrate_v1's SchemaBuilder::sync() already creates the
    // column (the entity now declares it). Only ALTER when it is missing so the
    // migration is idempotent across fresh and pre-existing stores.
    let has_column = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            "SELECT count(*) FROM pragma_table_info('reviews') WHERE name='analytics'".to_string(),
        ))
        .await
        .map_err(|e| Error::Migration(format!("failed to inspect reviews columns: {e}")))?
        .and_then(|r| r.try_get_by_index::<i32>(0).ok())
        .unwrap_or(0)
        > 0;
    if !has_column {
        db.execute_unprepared("ALTER TABLE reviews ADD COLUMN analytics BLOB")
            .await
            .map_err(|e| Error::Migration(format!("failed to add analytics column: {e}")))?;
    }

    db.execute_unprepared(
        "INSERT INTO _schema_version (version, description) VALUES (2, 'add reviews.analytics BLOB for compressed JSON blob')",
    )
    .await
    .map_err(|e| Error::Migration(format!("failed to record schema version: {e}")))?;

    Ok(())
}

/// Add the `allowed_models` table (admin-configured model allowlist).
///
/// On a fresh DB, migrate_v1's SchemaBuilder::sync() already creates the table
/// (the entity now declares it), so this only syncs on pre-existing stores and
/// is idempotent across both paths.
async fn migrate_v3(db: &DatabaseConnection) -> Result<(), Error> {
    let schema = Schema::new(db.get_database_backend());

    schema
        .builder()
        .register(AllowedModelEntity)
        .sync(db)
        .await
        .map_err(|e| Error::Migration(format!("schema sync failed (allowed_models): {e}")))?;

    db.execute_unprepared(
        "INSERT INTO _schema_version (version, description) VALUES (3, 'add allowed_models table for admin model allowlist')",
    )
    .await
    .map_err(|e| Error::Migration(format!("failed to record schema version: {e}")))?;

    Ok(())
}

/// Drop the dead `agent_session_usages` and `cache_usage_entries` tables.
///
/// The data those tables would hold is already persisted in the
/// `reviews.analytics` JSON blob (per-review analytics). The tables are never
/// written, so they are removed. `DROP TABLE IF EXISTS` keeps this idempotent
/// across fresh and pre-existing stores.
async fn migrate_v4(db: &DatabaseConnection) -> Result<(), Error> {
    db.execute_unprepared("DROP TABLE IF EXISTS agent_session_usages;")
        .await
        .map_err(|e| Error::Migration(format!("failed to drop agent_session_usages: {e}")))?;
    db.execute_unprepared("DROP TABLE IF EXISTS cache_usage_entries;")
        .await
        .map_err(|e| Error::Migration(format!("failed to drop cache_usage_entries: {e}")))?;

    db.execute_unprepared(
        "INSERT INTO _schema_version (version, description) VALUES (4, 'drop dead agent_session_usages and cache_usage_entries tables')",
    )
    .await
    .map_err(|e| Error::Migration(format!("failed to record schema version: {e}")))?;

    Ok(())
}

/// Add the `usage` BLOB column to `agent_sessions`.
///
/// Persists the per-session token usage (`AgentSession.usage`) as a compressed
/// JSON blob, mirroring `reviews.analytics`. `SchemaBuilder::sync()` does not
/// ALTER an existing table, so this is an explicit `ALTER TABLE`. Idempotency is
/// guaranteed by the schema-version gate in `run_migrations` (this only runs
/// when the store is below version 5) plus the `pragma_table_info` existence
/// check for safety across fresh and pre-existing stores.
async fn migrate_v5(db: &DatabaseConnection) -> Result<(), Error> {
    let has_column = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            "SELECT count(*) FROM pragma_table_info('agent_sessions') WHERE name='usage'"
                .to_string(),
        ))
        .await
        .map_err(|e| Error::Migration(format!("failed to inspect agent_sessions columns: {e}")))?
        .and_then(|r| r.try_get_by_index::<i32>(0).ok())
        .unwrap_or(0)
        > 0;
    if !has_column {
        db.execute_unprepared("ALTER TABLE agent_sessions ADD COLUMN usage BLOB")
            .await
            .map_err(|e| Error::Migration(format!("failed to add usage column: {e}")))?;
    }

    db.execute_unprepared(
        "INSERT INTO _schema_version (version, description) VALUES (5, 'add agent_sessions.usage BLOB for per-session token usage')",
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
            "allowed_models",
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

        // The reviews.analytics BLOB column must exist (added by migrate_v2).
        let col_row = db
            .query_one_raw(sea_orm::Statement::from_string(
                db.get_database_backend(),
                "SELECT count(*) FROM pragma_table_info('reviews') WHERE name='analytics'"
                    .to_string(),
            ))
            .await
            .expect("query should succeed");
        let col_count: i32 = col_row
            .and_then(|r| r.try_get_by_index::<i32>(0).ok())
            .unwrap_or(0);
        assert_eq!(
            col_count, 1,
            "reviews.analytics BLOB column should exist after migration"
        );

        // The agent_sessions.usage BLOB column must exist (added by migrate_v5).
        let usage_row = db
            .query_one_raw(sea_orm::Statement::from_string(
                db.get_database_backend(),
                "SELECT count(*) FROM pragma_table_info('agent_sessions') WHERE name='usage'"
                    .to_string(),
            ))
            .await
            .expect("query should succeed");
        let usage_count: i32 = usage_row
            .and_then(|r| r.try_get_by_index::<i32>(0).ok())
            .unwrap_or(0);
        assert_eq!(
            usage_count, 1,
            "agent_sessions.usage BLOB column should exist after migration"
        );

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
