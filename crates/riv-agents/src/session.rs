//! Incremental [`AgentSession`] construction from a streamed agent response.
//!
//! [`SessionBuilder`] is fed from the streaming loop in `stream_agent_inner`
//! (thinking/output deltas, tool call begin/end/result, turn-final markers) and
//! produces a fully-built [`AgentSession`] (turns + messages) that the review
//! pipeline attaches to a [`Review`] for cascade persistence.

use mti::prelude::MagicTypeId;
use riv_types::agent::{AgentSession, AgentTurn, AgentTurnMessage};
use riv_types::cost::SessionUsage;

/// A tool invocation being accumulated from the stream.
struct PendingTool {
    /// The name of the tool being called.
    name: String,
    /// Accumulated JSON-encoded input bits.
    input_buf: String,
    /// The tool result, collected once the tool result arrives.
    output: String,
}

/// Accumulates streamed chunks into an [`AgentSession`].
///
/// One builder is created per agent per review at the start of the stream.
/// The builder is not thread-safe; it is driven from the single streaming
/// task in `stream_agent_inner`.
pub struct SessionBuilder {
    id: MagicTypeId,
    model_name: String,
    turns: Vec<AgentTurn>,
    /// Messages of the turn currently being accumulated.
    current_messages: Vec<AgentTurnMessage>,
    /// Index of the next message within the current turn.
    msg_index: i32,
    /// Index of the next turn within the session.
    turn_index: i32,
    /// Pending assistant thinking text (streamed reasoning deltas).
    thinking: String,
    /// Pending assistant output text (streamed text chunks).
    output: String,
    /// Pending tool invocation, if one is in flight.
    current_tool: Option<PendingTool>,
    /// Per-session token usage, recorded when the final response streams in.
    usage: SessionUsage,
}

impl SessionBuilder {
    /// Create a builder seeded with the session id, model name and the
    /// first user message (the review prompt/diff).
    pub fn new(id: MagicTypeId, model_name: String, prompt: &str) -> Self {
        let first_message = AgentTurnMessage {
            role: "user".into(),
            text_content: Some(prompt.to_string()),
            ..Default::default()
        };
        Self {
            id,
            model_name,
            turns: Vec::new(),
            current_messages: vec![first_message],
            msg_index: 1,
            turn_index: 0,
            thinking: String::new(),
            output: String::new(),
            current_tool: None,
            usage: SessionUsage::default(),
        }
    }

    /// Append a thinking (reasoning delta) chunk to the pending assistant message.
    pub fn push_thinking(&mut self, text: &str) {
        self.thinking.push_str(text);
    }

    /// Append an output text chunk to the pending assistant message.
    pub fn push_output(&mut self, text: &str) {
        self.output.push_str(text);
    }

    /// Begin a tool invocation, flushing any pending assistant message first.
    pub fn tool_begin(&mut self, name: String) {
        self.flush_assistant();
        self.current_tool = Some(PendingTool {
            name,
            input_buf: String::new(),
            output: String::new(),
        });
    }

    /// Append a chunk of the tool input JSON.
    pub fn tool_delta(&mut self, delta: &str) {
        if let Some(tool) = &mut self.current_tool {
            tool.input_buf.push_str(delta);
        }
    }

    /// Marks the tool input as complete.
    ///
    /// The tool input is already accumulated via [`Self::tool_delta`]; the
    /// tool output arrives with the tool result.
    pub fn tool_end(&mut self) {
        // Nothing to do — kept for symmetry with the stream event.
    }

    /// Record the tool result and push the completed tool message.
    pub fn tool_result(&mut self, pieces: Vec<String>) {
        let Some(mut tool) = self.current_tool.take() else {
            return;
        };
        tool.output = pieces.join("\n");
        let message = AgentTurnMessage {
            role: "tool".into(),
            tool_name: Some(tool.name),
            tool_input: Some(tool.input_buf),
            tool_output: Some(tool.output),
            msg_index: self.msg_index,
            ..Default::default()
        };
        self.msg_index += 1;
        self.current_messages.push(message);
    }

    /// Record the per-session token usage (from the streamed final response).
    pub fn set_usage(&mut self, usage: SessionUsage) {
        self.usage = usage;
    }

    /// Close out the current turn: flush any pending assistant message and
    /// push the accumulated turn into the session.
    pub fn finalize_turn(&mut self) {
        self.flush_assistant();
        if self.current_messages.is_empty() {
            return;
        }
        self.turns.push(AgentTurn {
            id: None,
            session_id: self.id.clone(),
            turn_index: self.turn_index,
            messages: std::mem::take(&mut self.current_messages),
        });
        self.turn_index += 1;
        self.msg_index = 0;
    }

    /// Consume the builder, flushing any remaining state and returning the fully-built session.
    pub fn finish(mut self) -> AgentSession {
        self.finalize_turn();
        AgentSession {
            id: self.id,
            review_id: None,
            model_name: self.model_name,
            usage: self.usage,
            turns: self.turns,
        }
    }

    /// Push a pending assistant message (if any thinking/output was accumulated).
    fn flush_assistant(&mut self) {
        let thinking = self.thinking.trim().to_string();
        let output = self.output.trim().to_string();
        self.thinking.clear();
        self.output.clear();

        if thinking.is_empty() && output.is_empty() {
            return;
        }

        let message = AgentTurnMessage {
            role: "assistant".into(),
            thinking: Some(thinking),
            output: Some(output),
            msg_index: self.msg_index,
            ..Default::default()
        };

        self.msg_index += 1;
        self.current_messages.push(message);
    }
}

#[cfg(test)]
mod tests {
    use mti::prelude::{MagicTypeIdExt, V7};

    use super::*;

    fn make_id(s: &str) -> MagicTypeId {
        s.create_type_id::<V7>()
    }

    #[test]
    fn prompt_thinking_output_final_yields_user_and_assistant_messages() {
        let id = make_id("sess-1");
        let mut builder =
            SessionBuilder::new(id.clone(), "test-model".to_string(), "review this diff");

        builder.push_thinking("thinking about the diff");
        builder.push_output("found a bug");
        builder.finalize_turn();

        let session = builder.finish();
        assert_eq!(session.model_name, "test-model");
        assert_eq!(session.id.to_string(), id.to_string());
        assert_eq!(session.turns.len(), 1);

        let turn = &session.turns[0];
        assert_eq!(turn.turn_index, 0);
        assert_eq!(turn.messages.len(), 2);

        assert_eq!(turn.messages[0].role, "user");
        assert_eq!(turn.messages[0].msg_index, 0);
        assert_eq!(
            turn.messages[0].text_content.as_deref(),
            Some("review this diff")
        );
        assert!(turn.messages[0].thinking.is_none());
        assert!(turn.messages[0].output.is_none());

        assert_eq!(turn.messages[1].role, "assistant");
        assert_eq!(turn.messages[1].msg_index, 1);
        assert_eq!(
            turn.messages[1].thinking.as_deref(),
            Some("thinking about the diff")
        );
        assert_eq!(turn.messages[1].output.as_deref(), Some("found a bug"));
    }

    #[test]
    fn tool_call_flow_yields_tool_message_after_assistant_message() {
        let mut builder = SessionBuilder::new(
            make_id("sess-2"),
            "test-model".to_string(),
            "review this diff",
        );

        builder.push_thinking("need to run a linter");
        builder.tool_begin("linter".to_string());
        builder.tool_delta(r#"{"file":"src/main.rs""#);
        builder.tool_delta(r#","rule":"clippy"}"#);
        builder.tool_end();
        builder.tool_result(vec!["warning: unused variable".to_string()]);
        builder.finalize_turn();

        let session = builder.finish();
        assert_eq!(session.turns.len(), 1);
        let turn = &session.turns[0];
        assert_eq!(turn.messages.len(), 3);

        let assistant = &turn.messages[1];
        assert_eq!(assistant.role, "assistant");
        assert_eq!(assistant.msg_index, 1);
        assert_eq!(assistant.thinking.as_deref(), Some("need to run a linter"));

        let tool = &turn.messages[2];
        assert_eq!(tool.role, "tool");
        assert_eq!(tool.msg_index, 2);
        assert_eq!(tool.tool_name.as_deref(), Some("linter"));
        assert_eq!(
            tool.tool_input.as_deref(),
            Some(r#"{"file":"src/main.rs","rule":"clippy"}"#)
        );
        assert_eq!(
            tool.tool_output.as_deref(),
            Some("warning: unused variable")
        );
    }

    #[test]
    fn finish_sets_model_name_and_id() {
        let id = make_id("sess-3");
        let builder = SessionBuilder::new(id.clone(), "gpt-test".to_string(), "prompt");
        let session = builder.finish();

        assert_eq!(session.id.to_string(), id.to_string());
        assert_eq!(session.model_name, "gpt-test");
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].messages.len(), 1);
        assert_eq!(session.turns[0].messages[0].role, "user");
    }

    #[test]
    fn empty_thinking_and_output_produce_no_assistant_message() {
        let mut builder =
            SessionBuilder::new(make_id("sess-4"), "test-model".to_string(), "prompt");

        builder.finalize_turn();

        let session = builder.finish();
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].messages.len(), 1);
        assert_eq!(session.turns[0].messages[0].role, "user");
    }

    #[test]
    fn multiple_turns_are_indexed_sequentially() {
        let mut builder = SessionBuilder::new(
            make_id("sess-5"),
            "test-model".to_string(),
            "review this diff",
        );

        builder.push_output("first answer");
        builder.finalize_turn();

        builder.push_thinking("second turn thinking");
        builder.push_output("second answer");
        builder.finalize_turn();

        let session = builder.finish();
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[0].turn_index, 0);
        assert_eq!(session.turns[0].messages[1].msg_index, 1);
        assert_eq!(session.turns[1].turn_index, 1);
        assert_eq!(session.turns[1].messages[0].msg_index, 0);
        assert_eq!(
            session.turns[1].messages[0].thinking.as_deref(),
            Some("second turn thinking")
        );
    }
}
