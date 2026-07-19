use chrono::{DateTime, NaiveDateTime, Utc};
use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

/// A single benchmark run, grouping multiple PR results together.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "benchmarks")
)]
pub struct Benchmark {
    /// The unique identifier for this benchmark run.
    pub id: MagicTypeId,

    /// Name of the dataset used for this benchmark run.
    pub dataset_name: String,

    /// Optional version/hash of the dataset for reproducibility.
    pub dataset_version: Option<String>,

    /// When this benchmark was created.
    pub created_at: NaiveDateTime,

    /// When this benchmark was last updated.
    pub updated_at: NaiveDateTime,
}

impl Benchmark {
    /// Create a new Benchmark with the current time.
    pub fn new(id: MagicTypeId, dataset_name: String, dataset_version: Option<String>) -> Self {
        let now = Utc::now().naive_utc();
        Self {
            id,
            dataset_name,
            dataset_version,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get `created_at` as a `DateTime<Utc>`.
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }

    /// Get `updated_at` as a `DateTime<Utc>`.
    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.updated_at, Utc)
    }

    /// Set `updated_at` to the current time.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now().naive_utc();
    }
}
