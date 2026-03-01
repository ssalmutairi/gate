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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    // --- select_round_robin ---

    #[test]
    fn round_robin_empty_returns_none() {
        let counter = AtomicUsize::new(0);
        assert!(select_round_robin(&[], &counter).is_none());
    }

    #[test]
    fn round_robin_single_target() {
        let u = make_upstream();
        let t = make_target(u.id);
        let targets = vec![&t];
        let counter = AtomicUsize::new(0);
        let selected = select_round_robin(&targets, &counter);
        assert_eq!(selected.unwrap().id, t.id);
    }

    #[test]
    fn round_robin_cycles_through_targets() {
        let u = make_upstream();
        let t1 = make_target(u.id);
        let t2 = make_target(u.id);
        let t3 = make_target(u.id);
        let targets = vec![&t1, &t2, &t3];
        let counter = AtomicUsize::new(0);

        let ids: Vec<Uuid> = (0..6)
            .map(|_| select_round_robin(&targets, &counter).unwrap().id)
            .collect();
        assert_eq!(ids[0], t1.id);
        assert_eq!(ids[1], t2.id);
        assert_eq!(ids[2], t3.id);
        assert_eq!(ids[3], t1.id); // wraps around
        assert_eq!(ids[4], t2.id);
        assert_eq!(ids[5], t3.id);
    }

    // --- select_weighted_round_robin ---

    #[test]
    fn weighted_rr_empty_returns_none() {
        let counter = AtomicUsize::new(0);
        assert!(select_weighted_round_robin(&[], &counter).is_none());
    }

    #[test]
    fn weighted_rr_distribution() {
        let u = make_upstream();
        let mut t_a = make_target(u.id);
        t_a.weight = 3;
        let mut t_b = make_target(u.id);
        t_b.weight = 1;
        let targets = vec![&t_a, &t_b];
        let counter = AtomicUsize::new(0);

        // Total weight = 4. A gets first 3, B gets 4th
        let mut a_count = 0;
        let mut b_count = 0;
        for _ in 0..4 {
            let selected = select_weighted_round_robin(&targets, &counter).unwrap();
            if selected.id == t_a.id {
                a_count += 1;
            } else {
                b_count += 1;
            }
        }
        assert_eq!(a_count, 3);
        assert_eq!(b_count, 1);
    }

    // --- select_least_connections ---

    #[test]
    fn least_conn_empty_returns_none() {
        let tracker = ConnectionTracker::new();
        assert!(select_least_connections(&[], &tracker).is_none());
    }

    #[test]
    fn least_conn_picks_fewest() {
        let u = make_upstream();
        let t1 = make_target(u.id);
        let t2 = make_target(u.id);
        let mut tracker = ConnectionTracker::new();
        tracker.rebuild(&[t1.id, t2.id]);
        // Give t1 more connections
        tracker.increment(&t1.id);
        tracker.increment(&t1.id);
        tracker.increment(&t2.id);

        let targets = vec![&t1, &t2];
        let selected = select_least_connections(&targets, &tracker).unwrap();
        assert_eq!(selected.id, t2.id);
    }

    // --- ConnectionTracker ---

    #[test]
    fn tracker_increment_and_get() {
        let id = Uuid::new_v4();
        let mut tracker = ConnectionTracker::new();
        tracker.rebuild(&[id]);
        assert_eq!(tracker.get(&id), 0);
        tracker.increment(&id);
        assert_eq!(tracker.get(&id), 1);
        tracker.increment(&id);
        assert_eq!(tracker.get(&id), 2);
    }

    #[test]
    fn tracker_decrement_saturates_at_zero() {
        let id = Uuid::new_v4();
        let mut tracker = ConnectionTracker::new();
        tracker.rebuild(&[id]);
        tracker.decrement(&id); // should stay at 0, not underflow
        assert_eq!(tracker.get(&id), 0);
    }

    #[test]
    fn tracker_rebuild_preserves_existing() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let mut tracker = ConnectionTracker::new();
        tracker.rebuild(&[id1]);
        tracker.increment(&id1);
        tracker.increment(&id1);

        // Rebuild with id1 + id2; id1 should keep count
        tracker.rebuild(&[id1, id2]);
        assert_eq!(tracker.get(&id1), 2);
        assert_eq!(tracker.get(&id2), 0);
    }

    // --- Algorithm::from_str ---

    #[test]
    fn algorithm_from_str_weighted_round_robin() {
        assert!(matches!(
            Algorithm::from_str("weighted_round_robin"),
            Algorithm::WeightedRoundRobin
        ));
    }

    #[test]
    fn algorithm_from_str_least_connections() {
        assert!(matches!(
            Algorithm::from_str("least_connections"),
            Algorithm::LeastConnections
        ));
    }

    #[test]
    fn algorithm_from_str_round_robin_explicit() {
        assert!(matches!(
            Algorithm::from_str("round_robin"),
            Algorithm::RoundRobin
        ));
    }

    #[test]
    fn algorithm_from_str_unknown_defaults_to_round_robin() {
        assert!(matches!(
            Algorithm::from_str("random_nonsense"),
            Algorithm::RoundRobin
        ));
    }

    // --- ConnectionTracker with unknown IDs ---

    #[test]
    fn tracker_increment_unknown_id_is_noop() {
        let id = Uuid::new_v4();
        let tracker = ConnectionTracker::new();
        tracker.increment(&id); // no-op, id not in tracker
        assert_eq!(tracker.get(&id), 0);
    }

    #[test]
    fn tracker_decrement_unknown_id_is_noop() {
        let id = Uuid::new_v4();
        let tracker = ConnectionTracker::new();
        tracker.decrement(&id); // no-op
        assert_eq!(tracker.get(&id), 0);
    }

    #[test]
    fn tracker_get_unknown_id_returns_zero() {
        let tracker = ConnectionTracker::new();
        assert_eq!(tracker.get(&Uuid::new_v4()), 0);
    }

    // --- weighted_rr single target ---

    #[test]
    fn weighted_rr_single_target() {
        let u = make_upstream();
        let mut t = make_target(u.id);
        t.weight = 5;
        let targets = vec![&t];
        let counter = AtomicUsize::new(0);

        let selected = select_weighted_round_robin(&targets, &counter).unwrap();
        assert_eq!(selected.id, t.id);
    }
}
