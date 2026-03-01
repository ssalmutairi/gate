use shared::models::Target;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// Load balancing algorithm selector.
pub enum Algorithm {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnections,
}

impl Algorithm {
    pub fn from_str(s: &str) -> Self {
        match s {
            "weighted_round_robin" => Algorithm::WeightedRoundRobin,
            "least_connections" => Algorithm::LeastConnections,
            _ => Algorithm::RoundRobin, // default
        }
    }
}

/// Tracks active connection counts per target for least-connections.
pub struct ConnectionTracker {
    /// Active connection count per target ID.
    counts: HashMap<Uuid, AtomicUsize>,
}

impl ConnectionTracker {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    pub fn increment(&self, target_id: &Uuid) {
        if let Some(counter) = self.counts.get(target_id) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn decrement(&self, target_id: &Uuid) {
        if let Some(counter) = self.counts.get(target_id) {
            // Saturating subtract to avoid underflow
            let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            });
        }
    }

    pub fn get(&self, target_id: &Uuid) -> usize {
        self.counts
            .get(target_id)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Rebuild the tracker for a new set of target IDs.
    /// Preserves counts for targets that still exist.
    pub fn rebuild(&mut self, target_ids: &[Uuid]) {
        let mut new_counts = HashMap::new();
        for id in target_ids {
            let existing = self
                .counts
                .get(id)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0);
            new_counts.insert(*id, AtomicUsize::new(existing));
        }
        self.counts = new_counts;
    }
}

/// Select a target using round-robin.
pub fn select_round_robin<'a>(
    targets: &[&'a Target],
    counter: &AtomicUsize,
) -> Option<&'a Target> {
    if targets.is_empty() {
        return None;
    }
    let idx = counter.fetch_add(1, Ordering::Relaxed) % targets.len();
    Some(targets[idx])
}

/// Select a target using weighted round-robin.
/// Builds an expanded list where each target appears `weight` times,
/// then rotates through it using the counter.
pub fn select_weighted_round_robin<'a>(
    targets: &[&'a Target],
    counter: &AtomicUsize,
) -> Option<&'a Target> {
    if targets.is_empty() {
        return None;
    }

    // Build weighted list: each target appears `weight` times
    let total_weight: usize = targets.iter().map(|t| t.weight.max(1) as usize).sum();
    if total_weight == 0 {
        return None;
    }

    let idx = counter.fetch_add(1, Ordering::Relaxed) % total_weight;

    let mut cumulative = 0usize;
    for target in targets {
        cumulative += target.weight.max(1) as usize;
        if idx < cumulative {
            return Some(target);
        }
    }

    // Fallback (shouldn't reach here)
    Some(targets[0])
}

/// Select the target with the fewest active connections.
pub fn select_least_connections<'a>(
    targets: &[&'a Target],
    tracker: &ConnectionTracker,
) -> Option<&'a Target> {
    if targets.is_empty() {
        return None;
    }

    targets
        .iter()
        .min_by_key(|t| tracker.get(&t.id))
        .copied()
}
