//! SQLite-backed storage implementation using SeaORM.

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use crb_types::stor::Save;
use crb_types::{
    agent::{AgentSession, AgentSessionEntity, AgentTurn, AgentTurnMessage},
    benchmark::{
        golden::{GoldenComment, GoldenCommentColumn, GoldenCommentEntity, GoldenCommentModel},
        judge::{JudgeVerdict, JudgeVerdictColumn, JudgeVerdictEntity, JudgeVerdictModel},
        result::{PrResult, PrResultEntity, PrResultModel},
        standalone::{Benchmark, BenchmarkEntity, BenchmarkModel},
    },
    finding::{Finding, FindingColumn, FindingEntity, FindingModel},
    review::{Review, ReviewActiveModel, ReviewEntity, ReviewModel},
};
use mti::prelude::MagicTypeId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DeleteResult,
    EntityTrait, IntoActiveModel, QueryFilter,
};

use crate::error::Error;
use crate::traits::{Storable, Store};

/// check if a SeaORM error is a duplicate key.
fn is_duplicate_key(err: &sea_orm::DbErr) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("unique constraint") || msg.contains("primary key") || msg.contains("duplicate")
}

/// try insert, fall back to update on duplicate key.
macro_rules! upsert {
    ($db:expr, $model:expr) => {{
        let fallback = $model.clone();
        let active = $model.into_active_model();
        match active.insert($db).await {
            Ok(_) => Ok(()),
            Err(e) if is_duplicate_key(&e) => {
                let active = fallback.into_active_model();
                active
                    .update($db)
                    .await
                    .map_err(|e2| Error::Query(format!("failed to update: {e2}")))?;
                Ok(())
            }
            Err(e) => Err(Error::Query(format!("failed to insert: {e}"))),
        }
    }};
}

/// box a value as `Box<dyn Any>` without triggering trivial-cast lints.
fn box_any<T: 'static>(value: T) -> Box<dyn Any> {
    Box::new(value)
}

/// A SQLite-backed storage backend
///
/// Opens a SQLite database connection, enables WAL journal mode,
/// and runs schema migrations at construction time.
#[derive(Clone)]
pub struct SqliteStore {
    db: Arc<DatabaseConnection>,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at `path` and run migrations.
    ///
    /// WAL journal mode is enabled automatically.
    /// The `path` can be a file path or `:memory:` for an in-memory database.
    pub async fn new(path: &str) -> Result<Self, Error> {
        let db_url = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{path}?mode=rwc")
        };

        let db = Database::connect(&db_url)
            .await
            .map_err(|e| Error::Connection(format!("failed to open database: {e}")))?;

        db.execute_unprepared("PRAGMA journal_mode=wal;")
            .await
            .map_err(|e| Error::Connection(format!("failed to enable WAL mode: {e}")))?;

        db.get_schema_registry("my_crate::entity::*")
            .sync(&db)
            .await
            .map_err(|e| Error::Connection(format!("failed to sync schema: {e}")))?;
        let store = Self { db: Arc::new(db) };

        crate::migration::run_migrations(&store.db).await?;

        Ok(store)
    }

    /// Access the underlying database connection (for advanced use).
    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[allow(async_fn_in_trait)]
impl Store for SqliteStore {
    async fn save<T: Storable + Save>(&self, item: &T) -> Result<(), Error> {
        item.save(&self.db)
            .await
            .map_err(|e| Error::Query(e.to_string()))
    }

    async fn load<T: Storable>(&self, id: &MagicTypeId) -> Result<Option<T>, Error> {
        if TypeId::of::<T>() == TypeId::of::<Review>() {
            let result = load_review(&self.db, id).await?;
            let result: Option<Box<dyn Any>> = result.map(box_any);
            return Ok(result.and_then(|b| b.downcast::<T>().ok().map(|b| *b)));
        }

        if TypeId::of::<T>() == TypeId::of::<PrResult>() {
            let result = load_pr_result(&self.db, id).await?;
            let result: Option<Box<dyn Any>> = result.map(box_any);
            return Ok(result.and_then(|b| b.downcast::<T>().ok().map(|b| *b)));
        }

        if TypeId::of::<T>() == TypeId::of::<Benchmark>() {
            let result = load_benchmark(&self.db, id).await?;
            let result: Option<Box<dyn Any>> = result.map(box_any);
            return Ok(result.and_then(|b| b.downcast::<T>().ok().map(|b| *b)));
        }

        if TypeId::of::<T>() == TypeId::of::<AgentSession>() {
            let result = load_agent_session(&self.db, id).await?;
            let result: Option<Box<dyn Any>> = result.map(box_any);
            return Ok(result.and_then(|b| b.downcast::<T>().ok().map(|b| *b)));
        }

        Err(Error::Internal(
            format!("unknown type for loading: {}", std::any::type_name::<T>()).into(),
        ))
    }

    async fn list<T: Storable>(&self, _options: &T::Options) -> Result<Vec<T>, Error> {
        if TypeId::of::<T>() == TypeId::of::<Review>() {
            let result = list_reviews(&self.db).await?;
            let result: Vec<Box<dyn Any>> = result.into_iter().map(box_any).collect();
            return Ok(result
                .into_iter()
                .filter_map(|b| b.downcast::<T>().ok().map(|b| *b))
                .collect());
        }

        if TypeId::of::<T>() == TypeId::of::<PrResult>() {
            let result = list_pr_results(&self.db).await?;
            let result: Vec<Box<dyn Any>> = result.into_iter().map(box_any).collect();
            return Ok(result
                .into_iter()
                .filter_map(|b| b.downcast::<T>().ok().map(|b| *b))
                .collect());
        }

        if TypeId::of::<T>() == TypeId::of::<Benchmark>() {
            let result = list_benchmarks(&self.db).await?;
            let result: Vec<Box<dyn Any>> = result.into_iter().map(box_any).collect();
            return Ok(result
                .into_iter()
                .filter_map(|b| b.downcast::<T>().ok().map(|b| *b))
                .collect());
        }

        Err(Error::Internal(
            format!(
                "list not implemented for type: {}",
                std::any::type_name::<T>()
            )
            .into(),
        ))
    }

    async fn delete<T: Storable>(&self, id: &MagicTypeId) -> Result<bool, Error> {
        let id_str = id.to_string();

        if TypeId::of::<T>() == TypeId::of::<Review>() {
            let result: DeleteResult = ReviewEntity::delete_by_id(id_str.clone())
                .exec(&*self.db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete review: {e}")))?;
            return Ok(result.rows_affected > 0);
        }

        if TypeId::of::<T>() == TypeId::of::<PrResult>() {
            let id_str = id.to_string();

            // Manually delete related entities before the parent (FK constraint needs
            // PRAGMA foreign_keys = ON to cascade, which isn't guaranteed by seaorm-sqlite)
            // First delete judge_verdicts for findings of this pr_result
            let finding_ids: Vec<i32> = FindingEntity::find()
                .filter(FindingColumn::PrResultId.eq(&id_str))
                .all(&*self.db)
                .await
                .map_err(|e| Error::Query(format!("failed to load findings for delete: {e}")))?
                .into_iter()
                .map(|f| f.id)
                .collect();

            if !finding_ids.is_empty() {
                // Build a comma-separated list of IDs for the IN clause
                let ids_str: Vec<String> = finding_ids.iter().map(|id| id.to_string()).collect();
                let in_clause = ids_str.join(",");

                self.db
                    .execute_unprepared(&format!(
                        "DELETE FROM judge_verdicts WHERE finding_id IN ({in_clause});"
                    ))
                    .await
                    .map_err(|e| Error::Query(format!("failed to delete judge_verdicts: {e}")))?;

                self.db
                    .execute_unprepared(&format!("DELETE FROM findings WHERE id IN ({in_clause});"))
                    .await
                    .map_err(|e| Error::Query(format!("failed to delete findings: {e}")))?;
            }

            // Delete golden comments referencing this pr_result
            self.db
                .execute_unprepared(&format!(
                    "DELETE FROM golden_comments WHERE pr_result_id = '{id_str}';"
                ))
                .await
                .map_err(|e| Error::Query(format!("failed to delete golden_comments: {e}")))?;

            // Then delete the pr_result
            let result: DeleteResult = PrResultEntity::delete_by_id(id_str.clone())
                .exec(&*self.db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete pr_result: {e}")))?;
            return Ok(result.rows_affected > 0);
        }

        // --- Benchmark ---
        if TypeId::of::<T>() == TypeId::of::<Benchmark>() {
            let result: DeleteResult = BenchmarkEntity::delete_by_id(id_str.clone())
                .exec(&*self.db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete benchmark: {e}")))?;
            return Ok(result.rows_affected > 0);
        }

        // --- AgentSession ---
        if TypeId::of::<T>() == TypeId::of::<AgentSession>() {
            // TODO: Remove this PRAGMA OFF once SchemaBuilder::sync() generates
            //       ON DELETE CASCADE on SQLite FK constraints for agent_turns.
            self.db
                .execute_unprepared("PRAGMA foreign_keys = OFF;")
                .await
                .map_err(|e| Error::Query(format!("failed to disable FKs: {e}")))?;
            let result: DeleteResult = AgentSessionEntity::delete_by_id(id_str.clone())
                .exec(&*self.db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete agent_session: {e}")))?;
            return Ok(result.rows_affected > 0);
        }

        Err(Error::Internal(
            format!("unknown type for deletion: {}", std::any::type_name::<T>()).into(),
        ))
    }

    async fn migrate(&self) -> Result<(), Error> {
        crate::migration::run_migrations(&self.db).await
    }
}

/// Load a `Review` by its ID.
async fn load_review(db: &DatabaseConnection, id: &MagicTypeId) -> Result<Option<Review>, Error> {
    let model = ReviewEntity::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(|e| Error::Query(format!("failed to load review: {e}")))?;

    match model {
        Some(row) => Ok(Some(map_model_to_review(row))),
        None => Ok(None),
    }
}

/// List all `Review` entries.
async fn list_reviews(db: &DatabaseConnection) -> Result<Vec<Review>, Error> {
    let models = ReviewEntity::find()
        .all(db)
        .await
        .map_err(|e| Error::Query(format!("failed to list reviews: {e}")))?;
    Ok(models.into_iter().map(map_model_to_review).collect())
}

/// Load a `PrResult` with its children (golden_comments).
async fn load_pr_result(
    db: &DatabaseConnection,
    id: &MagicTypeId,
) -> Result<Option<PrResult>, Error> {
    let model = PrResultEntity::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(|e| Error::Query(format!("failed to load pr_result: {e}")))?;

    match model {
        Some(row) => {
            // Load golden comments
            let golden_models = GoldenCommentEntity::find()
                .filter(GoldenCommentColumn::PrResultId.eq(id.to_string()))
                .all(db)
                .await
                .map_err(|e| Error::Query(format!("failed to load golden comments: {e}")))?;

            let gcs: Vec<GoldenComment> = golden_models
                .into_iter()
                .map(|gc| GoldenComment {
                    id: Some(gc.id.to_string().parse::<MagicTypeId>().unwrap_or_default()),
                    pr_result_id: id.clone(),
                    comment: gc.comment,
                    severity: gc.severity,
                })
                .collect();

            let benchmark_id = row
                .benchmark_id
                .as_ref()
                .and_then(|s| s.parse::<MagicTypeId>().ok());

            Ok(Some(PrResult {
                id: id.clone(),
                benchmark_id,
                golden_comments: gcs,
                findings_with_verdicts: load_findings_and_verdicts(db, id).await?,
            }))
        }
        None => Ok(None),
    }
}

/// List all `PrResult` entries (lightweight, no children loaded).
async fn list_pr_results(db: &DatabaseConnection) -> Result<Vec<PrResult>, Error> {
    let models = PrResultEntity::find()
        .all(db)
        .await
        .map_err(|e| Error::Query(format!("failed to list pr_results: {e}")))?;
    Ok(models
        .into_iter()
        .map(|m| PrResult {
            id: m.id.parse::<MagicTypeId>().unwrap_or_default(),
            benchmark_id: m
                .benchmark_id
                .as_ref()
                .and_then(|s| s.parse::<MagicTypeId>().ok()),
            golden_comments: Vec::new(),
            findings_with_verdicts: Vec::new(),
        })
        .collect())
}

/// Load findings and their judge verdicts for a given pr_result.
async fn load_findings_and_verdicts(
    db: &DatabaseConnection,
    id: &MagicTypeId,
) -> Result<Vec<(Finding, JudgeVerdict)>, Error> {
    let finding_models = FindingEntity::find()
        .filter(FindingColumn::PrResultId.eq(id.to_string()))
        .all(db)
        .await
        .map_err(|e| Error::Query(format!("failed to load findings: {e}")))?;

    let mut results = Vec::with_capacity(finding_models.len());
    for fm in finding_models {
        let finding = Finding {
            id: Some(fm.id),
            pr_result_id: fm.pr_result_id,
            file: fm.file,
            line: fm.line,
            message: fm.message,
            severity: fm.severity,
            rule_code: fm.rule_code,
            severity_audited: fm.severity_audited,
            severity_audit_reason: fm.severity_audit_reason,
            evidence: fm.evidence,
            path_trace: fm.path_trace,
            confidence: fm.confidence,
            found_by: fm.found_by,
            agent_count: fm.agent_count.map(|c| c as u64),
            cross_validated: fm.cross_validated,
            cross_validated_by: fm.cross_validated_by.map(|c| c as u64),
            merged_from: fm.merged_from.map(|c| c as u64),
        };

        // Load the single verdict for this finding
        let verdict_model = JudgeVerdictEntity::find()
            .filter(JudgeVerdictColumn::FindingId.eq(fm.id))
            .one(db)
            .await
            .map_err(|e| Error::Query(format!("failed to load judge_verdict: {e}")))?;

        let verdict = match verdict_model {
            Some(vm) => JudgeVerdict {
                id: Some(vm.id),
                finding_id: vm.finding_id,
                linked_comment_id: vm.linked_comment_id,
                reasoning: vm.reasoning,
                match_: vm.match_,
                confidence: vm.confidence,
            },
            None => continue,
        };

        results.push((finding, verdict));
    }

    Ok(results)
}

/// Load a `Benchmark` by its ID.
async fn load_benchmark(
    db: &DatabaseConnection,
    id: &MagicTypeId,
) -> Result<Option<Benchmark>, Error> {
    let model = BenchmarkEntity::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(|e| Error::Query(format!("failed to load benchmark: {e}")))?;

    match model {
        Some(row) => Ok(Some(Benchmark {
            id: row.id.parse::<MagicTypeId>().unwrap_or_default(),
            dataset_name: row.dataset_name,
            dataset_version: row.dataset_version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })),
        None => Ok(None),
    }
}

/// List all `Benchmark` entries.
async fn list_benchmarks(db: &DatabaseConnection) -> Result<Vec<Benchmark>, Error> {
    let models = BenchmarkEntity::find()
        .all(db)
        .await
        .map_err(|e| Error::Query(format!("failed to list benchmarks: {e}")))?;
    Ok(models
        .into_iter()
        .map(|m| Benchmark {
            id: m.id.parse::<MagicTypeId>().unwrap_or_default(),
            dataset_name: m.dataset_name,
            dataset_version: m.dataset_version,
            created_at: m.created_at,
            updated_at: m.updated_at,
        })
        .collect())
}

/// Load an `AgentSession` with its turns and messages (3-level cascade).
async fn load_agent_session(
    db: &DatabaseConnection,
    id: &MagicTypeId,
) -> Result<Option<AgentSession>, Error> {
    let session_id_str = id.to_string();

    // 1. Load the session row
    let session_row = db
        .query_one_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            format!(
                "SELECT id, review_id, model_name FROM agent_sessions WHERE id = '{id}'",
                id = session_id_str.replace('\'', "''"),
            ),
        ))
        .await
        .map_err(|e| Error::Query(format!("failed to load agent_session: {e}")))?;

    let session_row = match session_row {
        Some(row) => row,
        None => return Ok(None),
    };

    let review_id: Option<String> = session_row.try_get_by_index::<String>(1).ok();
    let model_name: String = session_row
        .try_get_by_index::<String>(2)
        .unwrap_or_default();

    // 2. Load turns (ordered by turn_index)
    let turn_rows = db
        .query_all_raw(sea_orm::Statement::from_string(
            db.get_database_backend(),
            format!(
                "SELECT id, turn_index FROM agent_turns \
                     WHERE session_id = '{id}' ORDER BY turn_index ASC",
                id = session_id_str.replace('\'', "''"),
            ),
        ))
        .await
        .map_err(|e| Error::Query(format!("failed to load agent_turns: {e}")))?;

    let mut turns: Vec<AgentTurn> = Vec::with_capacity(turn_rows.len());

    for turn_row in &turn_rows {
        let turn_db_id: i32 = turn_row.try_get_by_index(0).unwrap_or(0);
        let turn_index: i32 = turn_row.try_get_by_index(1).unwrap_or(0);

        // 3. Load messages for this turn (ordered by msg_index)
        let msg_rows = db
            .query_all_raw(sea_orm::Statement::from_string(
                db.get_database_backend(),
                format!(
                    "SELECT msg_index, role, text_content, thinking, output, \
                         tool_name, tool_input, tool_output \
                         FROM agent_turn_messages WHERE turn_id = {tid} ORDER BY msg_index ASC",
                    tid = turn_db_id,
                ),
            ))
            .await
            .map_err(|e| Error::Query(format!("failed to load turn messages: {e}")))?;

        let mut messages: Vec<AgentTurnMessage> = Vec::with_capacity(msg_rows.len());

        for msg_row in &msg_rows {
            let _msg_index: i32 = msg_row.try_get_by_index(0).unwrap_or(0);
            let role: String = msg_row.try_get_by_index(1).unwrap_or_default();
            let text_content: Option<String> = msg_row.try_get_by_index(2).ok();
            let thinking: Option<String> = msg_row.try_get_by_index(3).ok();
            let output: Option<String> = msg_row.try_get_by_index(4).ok();
            let tool_name: Option<String> = msg_row.try_get_by_index(5).ok();
            let tool_input: Option<String> = msg_row.try_get_by_index(6).ok();
            let tool_output: Option<String> = msg_row.try_get_by_index(7).ok();

            messages.push(AgentTurnMessage {
                id: None,
                turn_id: turn_db_id,
                msg_index: _msg_index,
                role,
                text_content,
                thinking,
                output,
                tool_name,
                tool_input,
                tool_output,
            });
        }

        turns.push(AgentTurn {
            id: Some(turn_db_id),
            session_id: id.clone(),
            turn_index,
            messages,
        });
    }

    Ok(Some(AgentSession {
        id: id.clone(),
        review_id: review_id.and_then(|r| r.parse::<MagicTypeId>().ok()),
        model_name,
        turns,
    }))
}

fn map_model_to_review(model: ReviewModel) -> Review {
    model.into()
}
