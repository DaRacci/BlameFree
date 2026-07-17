use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: MagicTypeId,

    pub turns: Vec<AgentTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub id: MagicTypeId,

    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: MagicTypeId,

    pub message: AgentResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoleMessage {
    User(String),
    Tool(String),
    Assistant(AgentResponse),
    System(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub thinking: String,

    pub output: String,
}

/// A chunk of an agent session, used for streaming responses of partial data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentChunk {
    Thinking {
        /// The ID of the [`AgentSession`] this chunk belongs to.
        id: MagicTypeId,

        /// The content of the thinking chunk.
        content: String,

        /// Indiciates whether this is the last chunk of this turn.
        last: bool,
    },
    Output {
        /// The ID of the [`AgentSession`] this chunk belongs to.
        id: MagicTypeId,

        /// The content of the output chunk.
        content: String,

        /// Indiciates whether this is the last chunk of this turn.
        last: bool,
    },
    Tool {
        /// The ID of the [`AgentSession`] this chunk belongs to.
        id: MagicTypeId,

        /// The Tool Invocation ID this chunk belongs to.
        invocation_id: String,

        /// A byte of the tool chunk.
        byte: ToolByte,

        /// Indiciates whether this is the last chunk of this turn.
        last: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolByte {
    /// Mark the beginning of the tool invocation.
    ///
    /// Contains the tool name
    Begin(String),

    /// A single bit of the tool invocation from the agent.
    ///
    /// Contains the streamed chunk of the tool invocation.
    Bit(String),

    /// Mark the end of the tool invocation.
    End,

    /// The final result(s) of the tool invocation.
    Result(Vec<String>),
}
