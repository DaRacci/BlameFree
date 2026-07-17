use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,

    pub turns: Vec<AgentTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub id: String,

    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,

    pub role: TurnRole,

    pub content: AgentResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub thinking: String,

    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnRole {
    User,

    Assistant,

    System,
}
