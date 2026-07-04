/// Run the dashboard TUI.
///
/// This function takes ownership of the event receiver, sets up the terminal
/// in raw mode, and runs the rendering loop until the run finishes or the
/// user presses 'q'.
///
/// If stdout is not a real TTY, the dashboard falls back to a silent drain
/// (events are consumed and discarded) so the sender side doesn't block.
pub async fn run_dashboard(
    total_prs: usize,
    rx: mpsc::Receiver<DashboardEvent>,
) -> anyhow::Result<()> {
    use crossterm::event::{self, Event, KeyCode};
    use crossterm::terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    };
    use crossterm::ExecutableCommand;
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io::{stdout, IsTerminal};

    // If stdout isn't a real TTY, skip the TUI to avoid crashes on
    // enable_raw_mode / EnterAlternateScreen.  Events are drained
    // silently so senders never block.
    if !std::io::stdout().is_terminal() {
        tracing::warn!("stdout is not a terminal — TUI dashboard disabled, events drained");
        drain_events(rx).await;
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut dash = Dashboard::new(total_prs, rx);
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100)); // 10fps

    loop {
        interval.tick().await;

        // Check for user input (non-blocking)
        if event::poll(std::time::Duration::from_millis(1))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        // Process events from the harness
        dash.process_events();

        // Render
        terminal.draw(|frame| {
            render::render_dashboard(frame, &dash);
        })?;

        if dash.finished {
            // Show final report for a few seconds, then exit
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            break;
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Drain all events from the channel without rendering (non-TTY fallback).
async fn drain_events(mut rx: mpsc::Receiver<DashboardEvent>) {
    use tracing::info;
    while let Some(event) = rx.recv().await {
        if matches!(event, DashboardEvent::RunFinished { .. }) {
            info!("Dashboard: run finished (events drained, no TTY)");
            break;
        }
    }
}

use std::time::Instant;

use crb_judge::Metrics;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod render;

// ── Event types ──────────────────────────────────────────────────────────────

/// Events sent from the harness to the dashboard task.
#[derive(Debug, Clone, Serialize)]
pub enum DashboardEvent {
    /// An agent has started its review for a given PR.
    AgentStarted { pr_key: String, role: String },
    /// A chunk of streaming response text from an agent.
    AgentChunk { role: String, chunk: String },
    /// An agent has finished its review.
    AgentFinished {
        role: String,
        findings: usize,
        success: bool,
    },
    /// A single PR has been fully evaluated.
    PrCompleted {
        pr_key: String,
        metrics: Metrics,
        cost: f64,
        total_tokens: usize,
        agent_calls: usize,
        findings_count: usize,
    },
    /// The entire run has finished.
    RunFinished {
        total_prs: usize,
        aggregated: AggregateMetrics,
        total_cost: f64,
        total_tokens: usize,
        total_agent_calls: usize,
    },
}

/// Aggregate metrics across all PRs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateMetrics {
    #[serde(rename = "total_tp")]
    pub true_positives: usize,
    #[serde(rename = "total_fp")]
    pub false_positives: usize,
    #[serde(rename = "total_fn")]
    pub false_negatives: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

// ── Agent pane state ─────────────────────────────────────────────────────────

/// Status of a single agent pane.
#[derive(Debug, Clone)]
pub enum AgentStatus {
    /// Not yet started.
    Pending,
    /// Currently reviewing/in progress.
    Reviewing,
    /// Completed successfully.
    Done { findings: usize },
    /// Failed or timed out.
    Failed,
}

/// State for a single agent pane in the dashboard.
#[derive(Debug, Clone)]
pub struct AgentPane {
    pub role: String,
    pub status: AgentStatus,
    /// Scrollback buffer of the last N response lines.
    pub response_buffer: Vec<String>,
    pub findings_count: usize,
    pub success: bool,
}

impl AgentPane {
    pub fn new(role: &str) -> Self {
        Self {
            role: role.to_string(),
            status: AgentStatus::Pending,
            response_buffer: Vec::with_capacity(50),
            findings_count: 0,
            success: false,
        }
    }

    /// Append a response chunk, keeping only the last 20 lines.
    pub fn push_chunk(&mut self, chunk: &str) {
        for line in chunk.lines() {
            if self.response_buffer.len() >= 20 {
                self.response_buffer.remove(0);
            }
            self.response_buffer.push(line.to_string());
        }
    }
}

// ── Dashboard state ──────────────────────────────────────────────────────────

/// Full dashboard state, updated by events from the harness.
pub struct Dashboard {
    /// One pane per agent role (SA, CL, AR, SEC).
    pub agent_panes: Vec<AgentPane>,
    /// Total number of PRs in the run.
    pub total_prs: usize,
    /// Number of PRs completed so far.
    pub completed_prs: usize,
    /// The PR currently being evaluated (if any).
    pub current_pr: Option<String>,
    /// Running total cost in USD.
    pub total_cost: f64,
    /// Running total tokens.
    pub total_tokens: usize,
    /// Running total agent calls.
    pub total_agent_calls: usize,
    /// Summary lines from completed PRs.
    pub pr_summaries: Vec<String>,
    /// Aggregate metrics accumulated so far.
    pub aggregated: AggregateMetrics,
    /// When the dashboard started.
    pub start_time: Instant,
    /// Channel to receive events from the harness.
    pub rx: mpsc::Receiver<DashboardEvent>,
    /// Whether the run is finished.
    pub finished: bool,
    /// Number of PRs that have been processed (for progress display).
    pub completed_pr_keys: Vec<String>,
}

impl Dashboard {
    pub fn new(total_prs: usize, rx: mpsc::Receiver<DashboardEvent>) -> Self {
        let roles = ["SA", "CL", "ARCH", "SEC"];
        Self {
            agent_panes: roles.iter().map(|r| AgentPane::new(r)).collect(),
            total_prs,
            completed_prs: 0,
            current_pr: None,
            total_cost: 0.0,
            total_tokens: 0,
            total_agent_calls: 0,
            pr_summaries: Vec::with_capacity(total_prs),
            aggregated: AggregateMetrics::default(),
            start_time: Instant::now(),
            rx,
            finished: false,
            completed_pr_keys: Vec::new(),
        }
    }

    /// Process all available events from the channel.
    pub fn process_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: DashboardEvent) {
        match event {
            DashboardEvent::AgentStarted { pr_key, role } => {
                self.current_pr = Some(pr_key);
                if let Some(pane) = self.pane_mut(&role) {
                    pane.status = AgentStatus::Reviewing;
                    pane.findings_count = 0;
                    pane.response_buffer.clear();
                    pane.response_buffer
                        .push(format!("[Agent {} started]", role));
                }
            }
            DashboardEvent::AgentChunk { role, chunk } => {
                if let Some(pane) = self.pane_mut(&role) {
                    pane.push_chunk(&chunk);
                }
            }
            DashboardEvent::AgentFinished {
                role,
                findings,
                success,
            } => {
                if let Some(pane) = self.pane_mut(&role) {
                    pane.status = if success {
                        AgentStatus::Done { findings }
                    } else {
                        AgentStatus::Failed
                    };
                    pane.findings_count = findings;
                    pane.success = success;
                    pane.response_buffer
                        .push(format!("[Completed: {} finding(s)]", findings));
                }
            }
            DashboardEvent::PrCompleted {
                pr_key,
                metrics,
                cost,
                total_tokens,
                agent_calls,
                findings_count,
            } => {
                self.completed_prs += 1;
                self.total_cost += cost;
                self.total_tokens += total_tokens;
                self.total_agent_calls += agent_calls;
                self.completed_pr_keys.push(pr_key.clone());

                // Accumulate metrics
                self.aggregated.true_positives += metrics.true_positives;
                self.aggregated.false_positives += metrics.false_positives;
                self.aggregated.false_negatives += metrics.false_negatives;

                // Recompute running averages
                let tp_f = self.aggregated.true_positives as f64;
                let fp_f = self.aggregated.false_positives as f64;
                let fn_f = self.aggregated.false_negatives as f64;
                self.aggregated.precision = if tp_f + fp_f > 0.0 {
                    tp_f / (tp_f + fp_f)
                } else {
                    0.0
                };
                self.aggregated.recall = if tp_f + fn_f > 0.0 {
                    tp_f / (tp_f + fn_f)
                } else {
                    0.0
                };
                self.aggregated.f1 = if (self.aggregated.precision + self.aggregated.recall) > 0.0 {
                    2.0 * self.aggregated.precision * self.aggregated.recall
                        / (self.aggregated.precision + self.aggregated.recall)
                } else {
                    0.0
                };

                // Build summary line
                let summary = format!(
                    " {pr_key}: F1={:.3} | TP={} FP={} FN={} | {} findings | ${:.4}",
                    metrics.f1,
                    metrics.true_positives,
                    metrics.false_positives,
                    metrics.false_negatives,
                    findings_count,
                    cost,
                );
                self.pr_summaries.push(summary);

                // Reset agent panes for next PR
                for pane in &mut self.agent_panes {
                    pane.status = AgentStatus::Pending;
                    pane.response_buffer.clear();
                }
            }
            DashboardEvent::RunFinished {
                total_prs,
                aggregated,
                total_cost,
                total_tokens,
                total_agent_calls,
            } => {
                self.finished = true;
                self.total_prs = total_prs;
                self.aggregated = aggregated;
                self.total_cost = total_cost;
                self.total_tokens = total_tokens;
                self.total_agent_calls = total_agent_calls;
            }
        }
    }

    fn pane_mut(&mut self, role: &str) -> Option<&mut AgentPane> {
        self.agent_panes.iter_mut().find(|p| p.role == role)
    }

    /// Elapsed time since dashboard started.
    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }
}
