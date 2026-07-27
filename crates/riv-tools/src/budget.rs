//! Tool call budget for agent tool usage.
//!
//! A [`ToolCallBudget`] limits how many tool invocations an agent can make
//! during a single review session, preventing runaway tool calls.

use std::collections::HashMap;

/// Budget for agent tool calls during a single PR evaluation.
#[derive(Debug, Clone)]
pub struct ToolCallBudget {
    /// Maximum number of tool invocations across all tools.
    pub max_total_calls: usize,

    /// Per-tool-type maximum invocations.
    pub max_per_tool: usize,

    /// Whether to hard-stop (return error) or soft-stop (warn + return empty)
    /// when the budget is exhausted.
    pub hard_stop: bool,
}

impl Default for ToolCallBudget {
    fn default() -> Self {
        Self {
            max_total_calls: 50,
            max_per_tool: 20,
            hard_stop: false,
        }
    }
}

/// Runtime tracker for tool call budgets.
#[derive(Debug, Clone)]
pub struct ToolCallTracker {
    budget: ToolCallBudget,
    total_calls: usize,
    per_tool_calls: HashMap<String, usize>,
}

impl ToolCallTracker {
    /// Create a new tracker from a budget.
    pub fn new(budget: ToolCallBudget) -> Self {
        Self {
            budget,
            total_calls: 0,
            per_tool_calls: HashMap::new(),
        }
    }

    /// Check whether a tool call is allowed.
    /// Returns `Ok(())` if within budget, `Err(reason)` if over budget.
    pub fn check_call(&mut self, tool_name: &str) -> Result<(), String> {
        if self.total_calls >= self.budget.max_total_calls {
            let msg = format!(
                "Tool call budget exhausted: {} total calls (max {})",
                self.total_calls, self.budget.max_total_calls
            );
            if self.budget.hard_stop {
                return Err(msg);
            }
            tracing::warn!("{} — allowing anyway (soft stop)", msg);
        }

        let per_tool = self
            .per_tool_calls
            .entry(tool_name.to_string())
            .or_insert(0);
        if *per_tool >= self.budget.max_per_tool {
            let msg = format!(
                "Tool '{}' call budget exhausted: {} calls (max {})",
                tool_name, *per_tool, self.budget.max_per_tool
            );
            if self.budget.hard_stop {
                return Err(msg);
            }
            tracing::warn!("{} — allowing anyway (soft stop)", msg);
        }

        self.total_calls += 1;
        *self
            .per_tool_calls
            .entry(tool_name.to_string())
            .or_insert(0) += 1;
        Ok(())
    }

    /// Get the total number of tool calls made.
    pub fn total_calls(&self) -> usize {
        self.total_calls
    }

    /// Get the number of calls made for a specific tool.
    pub fn calls_for(&self, tool_name: &str) -> usize {
        self.per_tool_calls.get(tool_name).copied().unwrap_or(0)
    }

    /// Reset the tracker (new PR session).
    pub fn reset(&mut self) {
        self.total_calls = 0;
        self.per_tool_calls.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_budget() {
        let budget = ToolCallBudget::default();
        assert_eq!(budget.max_total_calls, 50);
        assert_eq!(budget.max_per_tool, 20);
        assert!(!budget.hard_stop);
    }

    #[test]
    fn test_tracker_respects_budget() {
        let budget = ToolCallBudget {
            max_total_calls: 3,
            max_per_tool: 2,
            hard_stop: true,
        };
        let mut tracker = ToolCallTracker::new(budget);

        assert!(tracker.check_call("shell").is_ok());
        assert!(tracker.check_call("shell").is_ok());
        // Third shell call should hit per-tool limit
        assert!(tracker.check_call("shell").is_err());
        assert!(tracker.check_call("read_file").is_ok());
        // Fifth total call should hit total limit
        assert!(tracker.check_call("read_file").is_err());
    }

    #[test]
    fn test_tracker_soft_stop() {
        let budget = ToolCallBudget {
            max_total_calls: 1,
            max_per_tool: 1,
            hard_stop: false,
        };
        let mut tracker = ToolCallTracker::new(budget);

        // Soft stop allows over-budget calls with a warning
        assert!(tracker.check_call("shell").is_ok());
        assert!(tracker.check_call("shell").is_ok()); // warns but OK
    }

    #[test]
    fn test_reset() {
        let budget = ToolCallBudget::default();
        let mut tracker = ToolCallTracker::new(budget);
        tracker.check_call("shell").unwrap();
        assert_eq!(tracker.total_calls(), 1);
        tracker.reset();
        assert_eq!(tracker.total_calls(), 0);
    }
}
