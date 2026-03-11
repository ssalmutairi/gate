use crate::proxy_core::circuit_breaker::CircuitBreaker;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

const RATE_LIMITER_MAX_KEYS: usize = 100_000;
const RATE_LIMITER_CLEANUP_INTERVAL: u64 = 1000;

fn pack(window_id: u32, count: u32) -> u64 {
    ((window_id as u64) << 32) | (count as u64)
}

fn unpack(val: u64) -> (u32, u32) {
    ((val >> 32) as u32, val as u32)
}

pub struct MemoryState {
    rate_limiters: DashMap<String, AtomicU64>,
    rate_limiter_epoch: Instant,
    rate_limiter_ops: AtomicU64,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl MemoryState {
    pub fn new(circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            rate_limiters: DashMap::new(),
            rate_limiter_epoch: Instant::now(),
            rate_limiter_ops: AtomicU64::new(0),
            circuit_breaker,
        }
    }

    pub fn check_rate_limit(
        &self,
        route_id: &Uuid,
        client_identity: &str,
        requests_per_second: i32,
    ) -> Result<i32, u64> {
        let key = format!("{}:{}", route_id, client_identity);
        let current_window = self.rate_limiter_epoch.elapsed().as_secs() as u32;
        let limit = requests_per_second as u32;

        let ops = self.rate_limiter_ops.fetch_add(1, Ordering::Relaxed);
        if ops % RATE_LIMITER_CLEANUP_INTERVAL == 0 || self.rate_limiters.len() > RATE_LIMITER_MAX_KEYS {
            self.rate_limiters.retain(|_, v| {
                let (win, _) = unpack(v.load(Ordering::Relaxed));
                win >= current_window.saturating_sub(1)
            });
        }

        let entry = self
            .rate_limiters
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0));
        Self::cas_increment(&entry, current_window, limit)
    }

    fn cas_increment(
        counter: &AtomicU64,
        current_window: u32,
        limit: u32,
    ) -> Result<i32, u64> {
        loop {
            let current = counter.load(Ordering::Acquire);
            let (win, count) = unpack(current);

            if win != current_window {
                let new_val = pack(current_window, 1);
                match counter.compare_exchange_weak(
                    current,
                    new_val,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Ok(limit as i32 - 1),
                    Err(_) => continue,
                }
            } else if count >= limit {
                return Err(1);
            } else {
                let new_val = pack(current_window, count + 1);
                match counter.compare_exchange_weak(
                    current,
                    new_val,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Ok(limit as i32 - (count + 1) as i32),
                    Err(_) => continue,
                }
            }
        }
    }
}

/// Standalone only uses in-memory state (no Redis).
pub enum StateBackend {
    Memory(MemoryState),
}

impl StateBackend {
    pub async fn check_rate_limit(
        &self,
        route_id: &Uuid,
        client_identity: &str,
        requests_per_second: i32,
    ) -> Result<i32, u64> {
        match self {
            StateBackend::Memory(mem) => {
                mem.check_rate_limit(route_id, client_identity, requests_per_second)
            }
        }
    }

    pub fn circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        match self {
            StateBackend::Memory(mem) => &mem.circuit_breaker,
        }
    }

    pub fn record_cb_failure(&self, target_id: &Uuid) -> bool {
        self.circuit_breaker().record_failure(target_id)
    }

    pub fn record_cb_success(&self, target_id: &Uuid) {
        let _ = self.circuit_breaker().record_success(target_id);
    }

}
