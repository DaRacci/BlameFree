//! SQLite-backed storage implementation using SeaORM.

use std::{any::TypeId, sync::Arc};

use mti::prelude::MagicTypeId;
use riv_types::stor::{LoadDepth, Save};
use riv_types::{
    agent::{AgentSession, AgentSessionEntity},
    benchmark::{
        result::{PrResult, PrResultEntity},
        standalone::{Benchmark, BenchmarkEntity},
    },
    finding::{FindingColumn, FindingEntity},
    review::{Review, ReviewEntity},
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DeleteResult, EntityTrait,
    QueryFilter,
};

use crate::error::Error;
use crate::traits::{Storable, Store};

#[derive(Clone)]
pub struct SqliteStore {
    db: Arc<DatabaseConnection>,
}

impl SqliteStore {
    pub async fn new(path: &str) -> Result<Self, Error> {
        let db_url = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            std::fs::create_dir_all(
                std::path::Path::new(path)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(".")),
            )
            .map_err(|e| Error::Connection(format!("failed to create database directory: {e}")))?;

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

    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }
}

impl Store for SqliteStore {
    async fn save<T: Storable + Save>(&self, item: &T) -> Result<(), Error> {
        let db = self.db.clone();
        item.save(&db)
            .await
            .map_err(|e| Error::Query(e.to_string()))
    }

    async fn load<T: Storable + riv_types::stor::EntityLoader + riv_types::stor::LoadChildren>(
        &self,
        id: &MagicTypeId,
    ) -> Result<Option<T>, Error> {
        let db = self.db.clone();
        let id = id.clone();
        let mut entity = T::load_by_id(&db, &id)
            .await
            .map_err(|e| Error::Query(e.to_string()))?;
        if let Some(ref mut e) = entity {
            e.load_children(&db, LoadDepth::Deep)
                .await
                .map_err(|e| Error::Query(e.to_string()))?;
        }
        Ok(entity)
    }

    async fn list<T: Storable + riv_types::stor::EntityLoader>(
        &self,
        _options: &T::Options,
    ) -> Result<Vec<T>, Error> {
        let db = self.db.clone();
        T::load_all(&db)
            .await
            .map_err(|e| Error::Query(e.to_string()))
    }

    async fn delete<T: Storable>(&self, id: &MagicTypeId) -> Result<bool, Error> {
        let db = self.db.clone();
        let id = id.clone();
        let id_str = id.to_string();
        if TypeId::of::<T>() == TypeId::of::<Review>() {
            let result: DeleteResult = ReviewEntity::delete_by_id(id_str.clone())
                .exec(&*db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete review: {e}")))?;
            return Ok(result.rows_affected > 0);
        }
        if TypeId::of::<T>() == TypeId::of::<PrResult>() {
            let finding_ids: Vec<i32> = FindingEntity::find()
                .filter(FindingColumn::PrResultId.eq(&id_str))
                .all(&*db)
                .await
                .map_err(|e| Error::Query(format!("failed to load findings: {e}")))?
                .into_iter()
                .map(|f| f.id)
                .collect();
            if !finding_ids.is_empty() {
                let ids_str: Vec<String> = finding_ids.iter().map(|id| id.to_string()).collect();
                let in_clause = ids_str.join(",");
                db.execute_unprepared(&format!(
                    "DELETE FROM judge_verdicts WHERE finding_id IN ({in_clause});"
                ))
                .await
                .map_err(|e| Error::Query(format!("failed to delete judge_verdicts: {e}")))?;
                db.execute_unprepared(&format!("DELETE FROM findings WHERE id IN ({in_clause});"))
                    .await
                    .map_err(|e| Error::Query(format!("failed to delete findings: {e}")))?;
            }
            db.execute_unprepared(&format!(
                "DELETE FROM golden_comments WHERE pr_result_id = '{id_str}';"
            ))
            .await
            .map_err(|e| Error::Query(format!("failed to delete golden_comments: {e}")))?;
            let result: DeleteResult = PrResultEntity::delete_by_id(id_str.clone())
                .exec(&*db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete pr_result: {e}")))?;
            return Ok(result.rows_affected > 0);
        }
        if TypeId::of::<T>() == TypeId::of::<Benchmark>() {
            let result: DeleteResult = BenchmarkEntity::delete_by_id(id_str.clone())
                .exec(&*db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete benchmark: {e}")))?;
            return Ok(result.rows_affected > 0);
        }
        if TypeId::of::<T>() == TypeId::of::<AgentSession>() {
            db.execute_unprepared("PRAGMA foreign_keys = OFF;")
                .await
                .map_err(|e| Error::Query(format!("failed to disable FKs: {e}")))?;
            let result: DeleteResult = AgentSessionEntity::delete_by_id(id_str.clone())
                .exec(&*db)
                .await
                .map_err(|e| Error::Query(format!("failed to delete agent_session: {e}")))?;
            return Ok(result.rows_affected > 0);
        }
        Err(Error::Internal(
            format!("unknown type for deletion: {}", std::any::type_name::<T>()).into(),
        ))
    }

    async fn migrate(&self) -> Result<(), Error> {
        let db = self.db.clone();
        crate::migration::run_migrations(&db).await
    }
}
