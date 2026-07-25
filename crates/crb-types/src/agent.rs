use mti::prelude::MagicTypeId;
use serde::{Deserialize, Serialize};

#[cfg(feature = "seaorm-storage")]
use crate::review::ReviewEntity;
use crate::wrappers::Model;

/// A single message in an agent turn, stored in the `agent_turn_messages` table.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "agent_turn_messages")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurnMessage {
    /// Surrogate primary key, auto-incremented by the DB.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(primary_key, auto_increment = true)
    )]
    pub id: Option<i32>,

    /// FK back to the parent [`AgentTurn`].
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(
            belongs_to,
            entity = "AgentTurnEntity",
            from = "turn_id",
            to = "id",
            on_delete = "Cascade"
        )
    )]
    pub turn_id: i32,

    /// The zero-based index of this message within the turn.
    pub msg_index: i32,

    /// The role of the message sender (e.g. "user", "assistant", "system", "tool").
    pub role: String,

    /// Plain text content (for user/system messages).
    pub text_content: Option<String>,

    /// The LLM's internal reasoning/thinking (for assistant messages).
    pub thinking: Option<String>,

    /// The final output text (for assistant messages).
    pub output: Option<String>,

    /// The tool name (for tool messages).
    pub tool_name: Option<String>,

    /// The tool input JSON string (for tool messages).
    pub tool_input: Option<String>,

    /// The tool output JSON string (for tool messages).
    pub tool_output: Option<String>,
}

impl From<RoleMessage> for AgentTurnMessage {
    fn from(msg: RoleMessage) -> Self {
        match msg {
            RoleMessage::User(text) => Self {
                role: "user".into(),
                text_content: Some(text),
                ..Default::default()
            },
            RoleMessage::System(text) => Self {
                role: "system".into(),
                text_content: Some(text),
                ..Default::default()
            },
            RoleMessage::Assistant(resp) => Self {
                role: "assistant".into(),
                thinking: Some(resp.thinking),
                output: Some(resp.output),
                ..Default::default()
            },
            RoleMessage::Tool(inv) => Self {
                role: "tool".into(),
                tool_name: Some(inv.tool_name),
                tool_input: Some(inv.input.to_string()),
                tool_output: Some(inv.output.to_string()),
                ..Default::default()
            },
        }
    }
}

impl From<AgentTurnMessage> for RoleMessage {
    fn from(msg: AgentTurnMessage) -> Self {
        match msg.role.as_str() {
            "user" => RoleMessage::User(msg.text_content.unwrap_or_default()),
            "system" => RoleMessage::System(msg.text_content.unwrap_or_default()),
            "assistant" => RoleMessage::Assistant(AgentResponse {
                thinking: msg.thinking.unwrap_or_default(),
                output: msg.output.unwrap_or_default(),
            }),
            "tool" => RoleMessage::Tool(ToolInvocation {
                tool_name: msg.tool_name.unwrap_or_default(),
                input: msg
                    .tool_input
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
                output: msg
                    .tool_output
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
            }),
            _ => RoleMessage::User(String::new()),
        }
    }
}

impl Default for AgentTurnMessage {
    fn default() -> Self {
        Self {
            id: None,
            turn_id: 0,
            msg_index: 0,
            role: String::new(),
            text_content: None,
            thinking: None,
            output: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
        }
    }
}

/// A single turn in an agent session, stored in the `agent_turns` table.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "agent_turns")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    /// Surrogate primary key, auto-incremented by the DB.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(primary_key, auto_increment = true)
    )]
    pub id: Option<i32>,

    /// FK back to the parent [`AgentSession`].
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(
            belongs_to,
            entity = "AgentSessionEntity",
            from = "session_id",
            to = "id",
            on_delete = "Cascade"
        )
    )]
    pub session_id: MagicTypeId,

    /// The zero-based index of this turn within the session.
    pub turn_index: i32,

    /// Messages in this turn.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "AgentTurnMessageEntity")
    )]
    pub messages: Vec<AgentTurnMessage>,
}

/// A single agent session, containing turns and messages.
#[cfg_attr(
    feature = "seaorm-storage",
    derive(crb_macros::EntityModel),
    sea_orm(table_name = "agent_sessions")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// The unique ID of the agent session.
    pub id: MagicTypeId,

    /// FK back to the parent [`Review`], if any.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(
            belongs_to,
            entity = "ReviewEntity",
            from = "review_id",
            to = "id",
            on_delete = "Cascade"
        )
    )]
    pub review_id: Option<MagicTypeId>,

    /// The model name used for this agent session.
    #[cfg_attr(feature = "seaorm-storage", sea_orm(column_name = "model_name"))]
    pub model_name: String,

    /// A list of turns the agent has taken in this session.
    ///
    /// Each turn contains its own ordered list of messages,
    /// which can be either user messages, tool messages, or assistant messages.
    #[cfg_attr(
        feature = "seaorm-storage",
        sea_orm(has_many, entity = "AgentTurnEntity")
    )]
    pub turns: Vec<AgentTurn>,
}

impl AgentSession {
    /// Convenience accessor for the model name as a [`Model`] wrapper.
    pub fn model(&self) -> Model {
        Model(self.model_name.clone())
    }

    /// Set model from a [`Model`] wrapper.
    pub fn set_model(&mut self, model: Model) {
        self.model_name = model.0;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoleMessage {
    /// A message from the user to the agent.
    User(String),

    /// A complete tool invocation.
    Tool(ToolInvocation),

    /// A response generated by the agent.
    Assistant(AgentResponse),

    /// A system prompt.
    System(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// The Name of the tool.
    pub tool_name: String,

    /// The generated input json from the LLM.
    pub input: serde_json::Value,

    /// The output json result from the tool.
    pub output: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// The LLMs internal thinking for this response.
    pub thinking: String,

    /// The final output generated by the LLM.
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
