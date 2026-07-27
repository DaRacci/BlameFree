//! SQLite-backed storage implementation using SeaORM.

use std::{any::TypeId, sync::Arc};

use crb_types::stor::{LoadDepth, Save};
use crb_types::{
    agent::{AgentSession, AgentSessionEntity},
    benchmark::{
        golden::{GoldenComment, GoldenCommentColumn, GoldenCommentEntity},
        judge::{JudgeVerdict, JudgeVerdictColumn, JudgeVerdictEntity, JudgeVerdictModel},
        result::{PrResult, PrResultEntity},
        standalone::{Benchmark, BenchmarkEntity},
    },
    finding::{Finding, FindingColumn, FindingEntity, FindingModel},
    review::{Review, ReviewEntity},
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

    async fn load<T: Storable + crb_types::stor::EntityLoader + crb_types::stor::LoadChildren>(
        &self,
        id: &MagicTypeId,
    ) -> Result<Option<T>, Error> {
        let mut entity = T::load_by_id(&self.db, id)
            .await
            .map_err(|e| Error::Query(e.to_string()))?;
        if let Some(ref mut e) = entity {
            e.load_children(&self.db, LoadDepth::Deep)
                .await
                .map_err(|e| Error::Query(e.to_string()))?;
        }
        Ok(entity)
    }

    async fn list<T: Storable + crb_types::stor::EntityLoader>(
        &self,
        _options: &T::Options,
    ) -> Result<Vec<T>, Error> {
        T::load_all(&self.db)
            .await
            .map_err(|e| Error::Query(e.to_string()))
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
