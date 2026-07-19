//! Storable implementations for domain types from `crb-types`.
//!
//! These live in `riv-stor` because the orphan rule requires that a trait
//! implementation live in the crate that defines either the trait or the
//! type — `riv-stor` defines `Storable` and already depends on `crb-types`.

use crate::traits::Storable;
use crb_types::agent::AgentSession;
use crb_types::benchmark::result::PrResult;
use crb_types::benchmark::standalone::Benchmark;
use crb_types::review::Review;

impl Storable for Review {
    const TYPE_NAME: &'static str = "Review";

    fn storable_id(&self) -> String {
        self.id.to_string()
    }
}

impl Storable for PrResult {
    const TYPE_NAME: &'static str = "PrResult";

    fn storable_id(&self) -> String {
        self.id.to_string()
    }
}

impl Storable for Benchmark {
    const TYPE_NAME: &'static str = "Benchmark";

    fn storable_id(&self) -> String {
        self.id.to_string()
    }
}

impl Storable for AgentSession {
    const TYPE_NAME: &'static str = "AgentSession";

    fn storable_id(&self) -> String {
        self.id.to_string()
    }
}
