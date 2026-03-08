use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;

/// A single request log entry kept in memory.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub id: String,
    pub route_id: Option<String>,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub latency_ms: f64,
    pub client_ip: String,
    pub upstream_target: Option<String>,
    pub created_at: String,
}

const MAX_LOG_ENTRIES: usize = 200;

/// In-memory request statistics for standalone mode.
/// Tracks aggregate counts, latency, and a ring buffer of recent request logs.
pub struct RequestStats {
    pub total_requests: AtomicU64,
    pub error_count: AtomicU64,
    latency_sum_us: AtomicU64,
    /// Sampled latencies for P95 calculation (kept bounded).
    latencies_ms: Mutex<Vec<f64>>,
    /// Ring buffer of the latest request logs.
    log_entries: Mutex<VecDeque<LogEntry>>,
}

const MAX_LATENCY_SAMPLES: usize = 10_000;

impl RequestStats {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            latency_sum_us: AtomicU64::new(0),
            latencies_ms: Mutex::new(Vec::with_capacity(1024)),
            log_entries: Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES + 1)),
        }
    }

    pub fn record(&self, status_code: i32, latency_ms: f64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if status_code >= 400 {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }
        self.latency_sum_us
            .fetch_add((latency_ms * 1000.0) as u64, Ordering::Relaxed);

        if let Ok(mut latencies) = self.latencies_ms.lock() {
            if latencies.len() >= MAX_LATENCY_SAMPLES {
                // Drop oldest half to keep memory bounded
                let half = latencies.len() / 2;
                latencies.drain(..half);
            }
            latencies.push(latency_ms);
        }
    }

    /// Record a full request log entry into the ring buffer.
    pub fn record_log(&self, entry: LogEntry) {
        if let Ok(mut logs) = self.log_entries.lock() {
            if logs.len() >= MAX_LOG_ENTRIES {
                logs.pop_front();
            }
            logs.push_back(entry);
        }
    }

    /// Get paginated log entries (newest first).
    pub fn get_logs(&self, page: usize, limit: usize) -> (Vec<LogEntry>, usize) {
        if let Ok(logs) = self.log_entries.lock() {
            let total = logs.len();
            let offset = (page - 1) * limit;
            let entries: Vec<LogEntry> = logs
                .iter()
                .rev()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect();
            (entries, total)
        } else {
            (vec![], 0)
        }
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        let total = self.total_requests.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let latency_sum_us = self.latency_sum_us.load(Ordering::Relaxed);

        let avg_latency_ms = if total > 0 {
            (latency_sum_us as f64 / 1000.0) / total as f64
        } else {
            0.0
        };

        let error_rate = if total > 0 {
            (errors as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let p95_latency_ms = if let Ok(mut latencies) = self.latencies_ms.lock() {
            if latencies.is_empty() {
                0.0
            } else {
                latencies.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let idx = ((latencies.len() as f64) * 0.95) as usize;
                let idx = idx.min(latencies.len() - 1);
                latencies[idx]
            }
        } else {
            0.0
        };

        StatsSnapshot {
            total_requests: total,
            error_rate,
            avg_latency_ms,
            p95_latency_ms,
        }
    }
}

pub struct StatsSnapshot {
    pub total_requests: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stats() {
        let stats = RequestStats::new();
        let snap = stats.snapshot();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.error_rate, 0.0);
        assert_eq!(snap.avg_latency_ms, 0.0);
        assert_eq!(snap.p95_latency_ms, 0.0);
    }

    #[test]
    fn records_success_and_error() {
        let stats = RequestStats::new();
        stats.record(200, 10.0);
        stats.record(200, 20.0);
        stats.record(500, 30.0);

        let snap = stats.snapshot();
        assert_eq!(snap.total_requests, 3);
        assert!((snap.error_rate - 33.333).abs() < 0.1);
        assert!((snap.avg_latency_ms - 20.0).abs() < 0.1);
    }

    #[test]
    fn p95_calculation() {
        let stats = RequestStats::new();
        for i in 1..=100 {
            stats.record(200, i as f64);
        }
        let snap = stats.snapshot();
        assert!(snap.p95_latency_ms >= 95.0);
    }
}
