use leptos::prelude::*;
use riv_types::agent::{AgentSession, AgentTurn, AgentTurnMessage};

use super::{empty_state::EmptyState, log_section::LogSection};

#[component]
pub fn LogViewer(agent_sessions: Vec<AgentSession>) -> impl IntoView {
    if agent_sessions.is_empty() {
        return view! {
            <EmptyState message="No agent sessions available." />
        }
        .into_any();
    }

    view! {
        <div class="session-log-viewer">
            {agent_sessions
                .into_iter()
                .enumerate()
                .map(|(index, session)| {
                    let prompt = prompt_text(&session);
                    let response = response_text(&session);
                    let reasoning = reasoning_text(&session);
                    let summary = format!("Session {}", index + 1);
                    let meta = format!("{} • {}", session.model_name, short_session_id(&session));

                    view! {
                        <details class="card" open=index == 0>
                            <summary class="card__header session-log-viewer__summary">
                                <div>
                                    <h3 class="card__title">{summary}</h3>
                                    <p class="session-log-viewer__meta">{meta}</p>
                                </div>
                            </summary>
                            <div class="card__body">
                                <LogSection label="Prompt" text=prompt />
                                <LogSection label="Response" text=response />
                                <LogSection label="Reasoning" text=reasoning />
                            </div>
                        </details>
                    }
                })
                .collect::<Vec<_>>()}
        </div>
    }
    .into_any()
}

fn short_session_id(session: &AgentSession) -> String {
    let id = session.id.to_string();
    let keep = id.len().min(8);
    id[..keep].to_string()
}

fn ordered_messages(session: &AgentSession) -> Vec<AgentTurnMessage> {
    let mut turns: Vec<AgentTurn> = session.turns.clone();
    turns.sort_by_key(|turn| turn.turn_index);

    turns
        .into_iter()
        .flat_map(|turn| {
            let mut messages = turn.messages;
            messages.sort_by_key(|message| message.msg_index);
            messages
        })
        .collect()
}

fn prompt_text(session: &AgentSession) -> String {
    ordered_messages(session)
        .into_iter()
        .filter_map(|message| match message.role.as_str() {
            "system" | "user" => message.text_content,
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn response_text(session: &AgentSession) -> String {
    ordered_messages(session)
        .into_iter()
        .filter_map(|message| match message.role.as_str() {
            "assistant" => message.output,
            "tool" => Some(format_tool_message(&message)),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn reasoning_text(session: &AgentSession) -> String {
    ordered_messages(session)
        .into_iter()
        .filter_map(|message| match message.role.as_str() {
            "assistant" => message.thinking,
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_tool_message(message: &AgentTurnMessage) -> String {
    let name = message
        .tool_name
        .clone()
        .unwrap_or_else(|| "tool".to_string());
    let input = message.tool_input.clone().unwrap_or_default();
    let output = message.tool_output.clone().unwrap_or_default();

    let mut chunks = vec![format!("[tool] {name}")];
    if !input.trim().is_empty() {
        chunks.push(format!("input:\n{input}"));
    }
    if !output.trim().is_empty() {
        chunks.push(format!("output:\n{output}"));
    }

    chunks.join("\n")
}
