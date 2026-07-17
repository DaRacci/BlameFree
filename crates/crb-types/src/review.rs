use std::collections::HashMap;

use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

use crate::{agent::AgentSession, cost::AnalyticsSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: MagicTypeId,

    pub agent_sessions: HashMap<MagicTypeId, AgentSession>,

    pub analytics: AnalyticsSnapshot,
}
