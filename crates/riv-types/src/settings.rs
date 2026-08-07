//! Admin-configured settings persisted to SQLite.
//!
//! The allowlist of models an admin permits for review/benchmark runs. The
//! live OpenRouter catalog (fetched by `riv-harness::model_capabilities`) is
//! only the candidate pool admins curate from; the actual set of runnable
//! models is this table.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single allowed model identifier.
///
/// Only a stable model identifier is stored per row.
/// All other details come from the live OpenRouter response at runtime.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(riv_macros::EntityModel),
    sea_orm(table_name = "allowed_models")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllowedModel {
    /// Stable model identifier.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(primary_key))]
    pub id: String,

    /// When this allowlist entry was added.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(default))]
    pub created_at: NaiveDateTime,
}

impl AllowedModel {
    /// Get `created_at` as a `DateTime<Utc>`.
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }
}
