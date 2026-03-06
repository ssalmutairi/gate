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

    pub fn is_available(&self, target_id: &Uuid) -> bool {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        let Some(ts) = targets.get_mut(target_id) else {
            return true;
        };

        match ts.state {
            State::Closed | State::HalfOpen => true,
            State::Open => {
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

    pub fn record_success(&self, target_id: &Uuid) -> bool {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());
        let Some(ts) = targets.get_mut(target_id) else {
            return false;
        };

        match ts.state {
            State::HalfOpen => {
                ts.state = State::Closed;
                ts.consecutive_failures = 0;
                ts.last_failure = None;
                true
            }
            State::Closed => {
                ts.consecutive_failures = 0;
                false
            }
            State::Open => false,
        }
    }

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
                ts.state = State::Open;
                true
            }
            State::Open => false,
        }
    }

    pub fn rebuild(&self, configs: &[(Uuid, u32, u32)]) {
        let mut targets = self.targets.write().unwrap_or_else(|e| e.into_inner());

        let valid_ids: std::collections::HashSet<Uuid> =
            configs.iter().map(|(id, _, _)| *id).collect();

        targets.retain(|id, _| valid_ids.contains(id));

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
