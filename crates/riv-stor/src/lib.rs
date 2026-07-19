//! Trait-based persistence crate for the review-harness project.
//!
//! Provides a generic [`Store`] trait backed by concrete backend implementations.

// Dependencies pulled in for their feature propagation
use {crb_types as _, sea_orm as _};

pub mod error;
pub mod migration;
pub mod store;
pub mod traits;

use crate::traits::Storable;
use crb_types::{
    agent::AgentSession,
    benchmark::{
        golden::GoldenComment,
        result::PrResult,
        standalone::Benchmark,
    },
    cost::AnalyticsSnapshot,
    review::Review,
};
use mti::prelude::MagicTypeId;

// ---------------------------------------------------------------------------
// Storable implementations for domain types
// ---------------------------------------------------------------------------

impl Storable for Review {
    type Options = ();
    fn item_id(&self) -> &MagicTypeId {
        &self.id
    }
}

impl Storable for PrResult {
    type Options = ();
    fn item_id(&self) -> &MagicTypeId {
        &self.id
    }
}

impl Storable for Benchmark {
    type Options = ();
    fn item_id(&self) -> &MagicTypeId {
        &self.id
    }
}

use std::sync::LazyLock;

static FALLBACK_ID: LazyLock<MagicTypeId> = LazyLock::new(MagicTypeId::default);

impl Storable for GoldenComment {
    type Options = ();
    fn item_id(&self) -> &MagicTypeId {
        self.id.as_ref().unwrap_or(&FALLBACK_ID)
    }
}

impl Storable for AgentSession {
    type Options = ();
    fn item_id(&self) -> &MagicTypeId {
        &self.id
    }
}

impl Storable for AnalyticsSnapshot {
    type Options = ();
    fn item_id(&self) -> &MagicTypeId {
        static SENTINEL: LazyLock<MagicTypeId> = LazyLock::new(MagicTypeId::default);
        &SENTINEL
    }
}
