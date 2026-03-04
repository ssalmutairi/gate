use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Closed,
    Open,
    HalfOpen,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Closed => write!(f, "closed"),
            State::Open => write!(f, "open"),
            State::HalfOpen => write!(f, "half_open"),
        }
    }
}

impl std::str::FromStr for State {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "closed" => Ok(State::Closed),
            "open" => Ok(State::Open),
            "half_open" => Ok(State::HalfOpen),
            _ => Err(()),
        }
    }
}

struct TargetState {
    state: State,
    consecutive_failures: u32,
    last_failure: Option<Instant>,
    threshold: u32,
    duration: Duration,
}

pub struct CircuitBreaker {
    targets: RwLock<HashMap<Uuid, TargetState>>,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            targets: RwLock::new(HashMap::new()),
        }
    }

    /// Set or update circuit breaker config for a target.
    pub fn configure(&self, target_id: Uuid, threshold: u32, duration_secs: u32) {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        let entry = targets.entry(target_id).or_insert_with(|| TargetState {
            state: State::Closed,
            consecutive_failures: 0,
            last_failure: None,
            threshold,
            duration: Duration::from_secs(duration_secs as u64),
        });
        entry.threshold = threshold;
        entry.duration = Duration::from_secs(duration_secs as u64);
    }

    /// Returns false if the circuit is OPEN (target should not receive traffic).
    /// Transitions OPEN → HALF_OPEN if duration has elapsed.
    pub fn is_available(&self, target_id: &Uuid) -> bool {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        let Some(ts) = targets.get_mut(target_id) else {
            return true; // No config = always available
        };

        match ts.state {
            State::Closed | State::HalfOpen => true,
            State::Open => {
                // Check if duration has elapsed → transition to HalfOpen
                if let Some(last_failure) = ts.last_failure {
                    if last_failure.elapsed() >= ts.duration {
                        ts.state = State::HalfOpen;
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Record a successful request to a target. Returns true if the circuit transitioned from HalfOpen → Closed.
    pub fn record_success(&self, target_id: &Uuid) -> bool {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        let Some(ts) = targets.get_mut(target_id) else {
            return false;
        };

        match ts.state {
            State::HalfOpen => {
                // Success in half-open → close the circuit
                ts.state = State::Closed;
                ts.consecutive_failures = 0;
                ts.last_failure = None;
                true
            }
            State::Closed => {
                ts.consecutive_failures = 0;
                false
            }
            State::Open => {
                // Shouldn't happen (requests blocked in Open), but reset anyway
                false
            }
        }
    }

    /// Record a failed request to a target. Returns true if the circuit just tripped to OPEN.
    pub fn record_failure(&self, target_id: &Uuid) -> bool {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        let Some(ts) = targets.get_mut(target_id) else {
            return false;
        };

        ts.last_failure = Some(Instant::now());

        match ts.state {
            State::Closed => {
                ts.consecutive_failures += 1;
                if ts.consecutive_failures >= ts.threshold {
                    ts.state = State::Open;
                    return true;
                }
                false
            }
            State::HalfOpen => {
                // Failure in half-open → reopen
                ts.state = State::Open;
                true
            }
            State::Open => false,
        }
    }

    /// Force a target's circuit breaker to a specific state (used for cross-instance sync via Redis).
    #[allow(dead_code)]
    pub fn set_state(&self, target_id: &Uuid, new_state: State) {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        if let Some(ts) = targets.get_mut(target_id) {
            if ts.state != new_state {
                tracing::debug!(
                    target_id = %target_id,
                    old_state = ?ts.state,
                    new_state = ?new_state,
                    "Circuit breaker state synced from Redis"
                );
                ts.state = new_state;
                if new_state == State::Closed {
                    ts.consecutive_failures = 0;
                    ts.last_failure = None;
                }
            }
        }
    }

    /// Get the current state of a target's circuit breaker.
    #[cfg(test)]
    pub fn get_state(&self, target_id: &Uuid) -> State {
        let targets = self.targets.read().unwrap_or_else(|e| e.into_inner());
        targets
            .get(target_id)
            .map(|ts| ts.state)
            .unwrap_or(State::Closed)
    }

    /// Sync the circuit breaker with current upstream configs.
    /// Adds new entries, updates existing configs, removes stale ones.
    /// Preserves state for existing entries.
    pub fn rebuild(&self, configs: &[(Uuid, u32, u32)]) {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());

        // Build set of valid IDs
        let valid_ids: std::collections::HashSet<Uuid> =
            configs.iter().map(|(id, _, _)| *id).collect();

        // Remove entries not in configs
        targets.retain(|id, _| valid_ids.contains(id));

        // Add/update entries
        for &(id, threshold, duration_secs) in configs {
            let entry = targets.entry(id).or_insert_with(|| TargetState {
                state: State::Closed,
                consecutive_failures: 0,
                last_failure: None,
                threshold,
                duration: Duration::from_secs(duration_secs as u64),
            });
            entry.threshold = threshold;
            entry.duration = Duration::from_secs(duration_secs as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_initially() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 3, 30);
        assert_eq!(cb.get_state(&id), State::Closed);
        assert!(cb.is_available(&id));
    }

    #[test]
    fn record_failures_trips_open() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 3, 30);

        assert!(!cb.record_failure(&id)); // 1
        assert!(!cb.record_failure(&id)); // 2
        assert!(cb.record_failure(&id)); // 3 → trips to OPEN
        assert_eq!(cb.get_state(&id), State::Open);
    }

    #[test]
    fn open_rejects_requests() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 1, 30);

        cb.record_failure(&id); // trips immediately
        assert!(!cb.is_available(&id));
    }

    #[test]
    fn open_transitions_to_half_open_after_duration() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 1, 0); // 0 second duration for instant transition

        cb.record_failure(&id); // trips to OPEN
        assert_eq!(cb.get_state(&id), State::Open);

        // With 0s duration, the next is_available call should transition to HalfOpen
        std::thread::sleep(Duration::from_millis(10));
        assert!(cb.is_available(&id));
        assert_eq!(cb.get_state(&id), State::HalfOpen);
    }

    #[test]
    fn half_open_success_resets_to_closed() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 1, 0);

        cb.record_failure(&id); // → OPEN
        std::thread::sleep(Duration::from_millis(10));
        cb.is_available(&id); // → HALF_OPEN

        cb.record_success(&id); // → CLOSED
        assert_eq!(cb.get_state(&id), State::Closed);
        assert!(cb.is_available(&id));
    }

    #[test]
    fn half_open_failure_reopens() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 1, 0);

        cb.record_failure(&id); // → OPEN
        std::thread::sleep(Duration::from_millis(10));
        cb.is_available(&id); // → HALF_OPEN

        assert!(cb.record_failure(&id)); // → OPEN again
        assert_eq!(cb.get_state(&id), State::Open);
    }

    #[test]
    fn is_available_true_when_closed() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 5, 30);

        // A few failures but under threshold
        cb.record_failure(&id);
        cb.record_failure(&id);
        assert!(cb.is_available(&id));
    }

    #[test]
    fn rebuild_preserves_state() {
        let cb = CircuitBreaker::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        cb.configure(id1, 2, 30);
        cb.configure(id2, 2, 30);

        // Trip id1
        cb.record_failure(&id1);
        cb.record_failure(&id1);
        assert_eq!(cb.get_state(&id1), State::Open);

        // Rebuild with only id1 (removes id2, preserves id1 state)
        cb.rebuild(&[(id1, 2, 60)]);

        assert_eq!(cb.get_state(&id1), State::Open); // state preserved
        assert_eq!(cb.get_state(&id2), State::Closed); // removed, returns default
    }

    #[test]
    fn unconfigured_target_always_available() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        // No configure call
        assert!(cb.is_available(&id));
        assert_eq!(cb.get_state(&id), State::Closed);
    }

    #[test]
    fn success_resets_failure_count() {
        let cb = CircuitBreaker::new();
        let id = Uuid::new_v4();
        cb.configure(id, 3, 30);

        cb.record_failure(&id); // 1
        cb.record_failure(&id); // 2
        cb.record_success(&id); // resets
        cb.record_failure(&id); // 1 again
        cb.record_failure(&id); // 2 again

        assert_eq!(cb.get_state(&id), State::Closed); // not tripped yet
    }
}
