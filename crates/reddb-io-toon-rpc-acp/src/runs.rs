//! Bounded retention for runs created over the legacy ACP-style REST contract.
//!
//! The server keeps every created run so `GET /runs/{id}` can answer after the
//! creating request returned. That map used to grow forever: a run was only
//! ever dropped on a *successful* cancel, and the default `cancel` hook fails,
//! so finished runs were immortal. [`RunStore`] bounds the map and lets a
//! finished run be released explicitly.

use crate::types::{AgentRun, RunStatus};
use std::collections::{HashMap, VecDeque};

/// Default number of runs retained per server.
pub const DEFAULT_MAX_RUNS: usize = 1024;

/// Is this run finished, so that it may be released without cancelling?
pub fn is_terminal(status: &RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
    )
}

/// A capacity-bounded map of run id to run state, oldest-first.
#[derive(Debug)]
pub struct RunStore {
    runs: HashMap<String, AgentRun>,
    order: VecDeque<String>,
    capacity: usize,
}

impl RunStore {
    /// Create a store retaining at most `capacity` runs (minimum one).
    pub fn new(capacity: usize) -> Self {
        Self {
            runs: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Retain a run, evicting older ones once the capacity is exceeded.
    ///
    /// Finished runs are evicted before live ones, so a server under load
    /// loses history rather than losing track of what is still running.
    pub fn insert(&mut self, id: String, run: AgentRun) {
        if self.runs.insert(id.clone(), run).is_some() {
            self.order.retain(|existing| existing != &id);
        }
        self.order.push_back(id);
        while self.runs.len() > self.capacity {
            self.evict_one();
        }
    }

    /// Read a retained run. Reading does not consume it.
    pub fn get(&self, id: &str) -> Option<&AgentRun> {
        self.runs.get(id)
    }

    /// Release a retained run.
    pub fn remove(&mut self, id: &str) -> Option<AgentRun> {
        let run = self.runs.remove(id)?;
        self.order.retain(|existing| existing != id);
        Some(run)
    }

    /// Number of retained runs.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Are no runs retained?
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Maximum number of retained runs.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    fn evict_one(&mut self) {
        let victim = self
            .order
            .iter()
            .find(|id| {
                self.runs
                    .get(*id)
                    .is_some_and(|run| is_terminal(&run.status))
            })
            .or_else(|| self.order.front())
            .cloned();
        match victim {
            Some(id) => {
                self.remove(&id);
            }
            None => {
                self.runs.clear();
            }
        }
    }
}

impl Default for RunStore {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_RUNS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentMessage, AgentRunInput};

    fn run(id: &str, status: RunStatus) -> AgentRun {
        AgentRun {
            agent_run_id: id.into(),
            agent_name: "echo".into(),
            status,
            input: AgentRunInput { parts: vec![] },
            output: Vec::<AgentMessage>::new(),
            error: None,
            metadata: None,
        }
    }

    #[test]
    fn terminal_states_are_releasable() {
        assert!(is_terminal(&RunStatus::Completed));
        assert!(is_terminal(&RunStatus::Failed));
        assert!(is_terminal(&RunStatus::Cancelled));
        assert!(!is_terminal(&RunStatus::Created));
        assert!(!is_terminal(&RunStatus::InProgress));
        assert!(!is_terminal(&RunStatus::Awaiting));
    }

    #[test]
    fn store_releases_runs_explicitly() {
        let mut store = RunStore::new(4);
        assert!(store.is_empty());
        store.insert("a".into(), run("a", RunStatus::Completed));
        assert_eq!(store.len(), 1);
        assert!(store.get("a").is_some());
        assert!(store.remove("a").is_some());
        assert!(store.remove("a").is_none());
        assert!(store.is_empty());
    }

    #[test]
    fn store_is_bounded_and_evicts_finished_runs_first() {
        let mut store = RunStore::new(2);
        assert_eq!(store.capacity(), 2);
        store.insert("live".into(), run("live", RunStatus::InProgress));
        store.insert("done".into(), run("done", RunStatus::Completed));
        store.insert("next".into(), run("next", RunStatus::Completed));
        assert_eq!(store.len(), 2);
        assert!(store.get("done").is_none());
        assert!(store.get("live").is_some());
        assert!(store.get("next").is_some());
    }

    #[test]
    fn store_evicts_live_runs_when_nothing_is_finished() {
        let mut store = RunStore::new(1);
        store.insert("first".into(), run("first", RunStatus::InProgress));
        store.insert("second".into(), run("second", RunStatus::InProgress));
        assert_eq!(store.len(), 1);
        assert!(store.get("second").is_some());
    }

    #[test]
    fn reinserting_the_same_id_keeps_one_entry() {
        let mut store = RunStore::new(2);
        store.insert("a".into(), run("a", RunStatus::Created));
        store.insert("a".into(), run("a", RunStatus::Completed));
        assert_eq!(store.len(), 1);
        assert_eq!(store.get("a").unwrap().status, RunStatus::Completed);
        store.insert("b".into(), run("b", RunStatus::InProgress));
        store.insert("c".into(), run("c", RunStatus::InProgress));
        assert_eq!(store.len(), 2);
        assert!(store.get("a").is_none());
    }

    #[test]
    fn capacity_is_at_least_one() {
        let mut store = RunStore::new(0);
        assert_eq!(store.capacity(), 1);
        store.insert("a".into(), run("a", RunStatus::Completed));
        assert_eq!(store.len(), 1);
    }
}
