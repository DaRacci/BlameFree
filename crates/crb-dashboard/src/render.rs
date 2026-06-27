//! Ratatui rendering logic for the review harness dashboard.
//!
//! Draws a 4-pane agent view, progress bar, and summary/report panes.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::{AgentStatus, Dashboard};

/// Render the entire dashboard into the given terminal frame.
pub fn render_dashboard(frame: &mut Frame, dash: &Dashboard) {
    let area = frame.area();

    if dash.finished {
        render_final_report(frame, area, dash);
        return;
    }

    let summary_height = std::cmp::min(dash.pr_summaries.len() + 2, 10) as u16;

    // ── Layout ───────────────────────────────────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),       // Progress bar row
            Constraint::Min(8),          // Agent panes
            Constraint::Length(summary_height), // PR summaries
        ])
        .split(area);

    // ── Progress bar ────────────────────────────────────────────────
    render_progress_bar(frame, chunks[0], dash);

    // ── Agent panes ─────────────────────────────────────────────────
    render_agent_panes(frame, chunks[1], dash);

    // ── PR summaries ────────────────────────────────────────────────
    render_pr_summaries(frame, chunks[2], dash);
}

/// Render the progress bar row at the top.
fn render_progress_bar(frame: &mut Frame, area: Rect, dash: &Dashboard) {
    let progress = if dash.total_prs > 0 {
        dash.completed_prs as f64 / dash.total_prs as f64
    } else {
        0.0
    };

    let pr_label = dash.current_pr.as_deref().unwrap_or("(idle)");
    let elapsed = dash.elapsed_secs();

    let label = format!(
        " {}/{} PRs | {} | {:.1}s | ${:.4}",
        dash.completed_prs, dash.total_prs, pr_label, elapsed, dash.total_cost,
    );

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("  Progress  ")
                .style(Style::default().fg(Color::Cyan)),
        )
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(progress as f64)
        .label(label);

    frame.render_widget(gauge, area);
}

/// Render the 4 agent panes as a 2x2 grid.
fn render_agent_panes(frame: &mut Frame, area: Rect, dash: &Dashboard) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(chunks[0]);

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(chunks[1]);

    let pane_areas = [
        top_chunks[0],    // SA
        top_chunks[1],    // CL
        bottom_chunks[0], // AR
        bottom_chunks[1], // SEC
    ];

    for (idx, pane) in dash.agent_panes.iter().enumerate() {
        if idx < pane_areas.len() {
            render_agent_pane(frame, pane_areas[idx], pane);
        }
    }
}

/// Render a single agent pane with its status and response buffer.
fn render_agent_pane(frame: &mut Frame, area: Rect, pane: &crate::AgentPane) {
    let (status_symbol, status_text, fg_color) = match &pane.status {
        AgentStatus::Pending => ("⏳", "pending", Color::DarkGray),
        AgentStatus::Reviewing => ("🔄", "reviewing...", Color::Yellow),
        AgentStatus::Done { findings } => {
            let text = if *findings > 0 {
                format!("{} finding(s)", findings)
            } else {
                "no findings".to_string()
            };
            ("✅", text, Color::Green)
        }
        AgentStatus::Failed => ("❌", "failed/timeout", Color::Red),
    };

    let title = format!(" {} {} ({}) ", status_symbol, pane.role, status_text);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(Style::default().fg(fg_color).add_modifier(Modifier::BOLD))
        .border_style(Style::default().fg(fg_color));

    // Build response lines
    let max_width = area.width.saturating_sub(4) as usize;
    let lines: Vec<ListItem> = pane
        .response_buffer
        .iter()
        .map(|line| {
            let truncated = if line.len() > max_width {
                format!("{}…", &line[..max_width.saturating_sub(1)])
            } else {
                line.clone()
            };
            ListItem::new(Line::from(Span::raw(truncated)))
        })
        .collect();

    let list = List::from_iter(lines).block(block);
    frame.render_widget(list, area);
}

/// Render the PR summaries pane (after each PR completes).
fn render_pr_summaries(frame: &mut Frame, area: Rect, dash: &Dashboard) {
    if dash.pr_summaries.is_empty() {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  PR Summaries  ")
        .title_style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(Color::Magenta));

    // Show last N summaries
    let max_visible = area.height.saturating_sub(2) as usize;
    let start = dash.pr_summaries.len().saturating_sub(max_visible);
    let max_width = area.width.saturating_sub(4) as usize;
    let visible: Vec<ListItem> = dash.pr_summaries[start..]
        .iter()
        .map(|s| {
            let truncated = if s.len() > max_width {
                format!("{}…", &s[..max_width.saturating_sub(1)])
            } else {
                s.clone()
            };
            ListItem::new(Line::from(Span::raw(truncated)))
        })
        .collect();

    let list = List::from_iter(visible).block(block);
    frame.render_widget(list, area);
}

/// Render the final aggregate report when all PRs finish.
fn render_final_report(frame: &mut Frame, area: Rect, dash: &Dashboard) {
    let total_prs = dash.total_prs;
    let agg = &dash.aggregated;
    let f1 = if (agg.precision + agg.recall) > 0.0 {
        2.0 * agg.precision * agg.recall / (agg.precision + agg.recall)
    } else {
        0.0
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "═══════════════════════════════════════════════",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(Span::styled(
            format!(
                " {} PRs | {:.1}% F1 | {} TP {} FP {} FN",
                total_prs, f1 * 100.0, agg.total_tp, agg.total_fp, agg.total_fn
            ),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                " {} agent calls | {} tokens | ${:.4}",
                dash.total_agent_calls, dash.total_tokens, dash.total_cost
            ),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            format!(
                " Avg precision: {:.3} | Avg recall: {:.3} | Avg F1: {:.3}",
                agg.precision, agg.recall, agg.f1
            ),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(Span::styled(
            format!(" Elapsed: {:.1}s", dash.elapsed_secs()),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "═══════════════════════════════════════════════",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(" Press 'q' or Ctrl+C to exit."),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("  Final Report  ")
                .title_style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(paragraph, area);
}
