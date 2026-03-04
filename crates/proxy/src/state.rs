use crate::circuit_breaker::CircuitBreaker;
#[cfg(feature = "redis-backend")]
use crate::circuit_breaker::State;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Maximum number of unique rate limiter keys before eviction is forced.
const RATE_LIMITER_MAX_KEYS: usize = 100_000;
/// How often (in requests) to run rate limiter cleanup.
const RATE_LIMITER_CLEANUP_INTERVAL: u64 = 1000;

/// Pack a (window_id, count) pair into a single u64 for atomic CAS.
/// High 32 bits = window_id, low 32 bits = count.
fn pack(window_id: u32, count: u32) -> u64 {
    ((window_id as u64) << 32) | (count as u64)
}

/// Unpack a u64 into (window_id, count).
fn unpack(val: u64) -> (u32, u32) {
    ((val >> 32) as u32, val as u32)
}

/// In-memory state backend (single-instance mode).
pub struct MemoryState {
    /// Rate limiter state: key = "{route_id}:{client_identity}", value = packed (window_id, count).
    rate_limiters: DashMap<String, AtomicU64>,
    /// Baseline instant for computing fixed-window IDs.
    rate_limiter_epoch: Instant,
    /// Counter for periodic rate limiter cleanup.
    rate_limiter_ops: AtomicU64,
    /// Circuit breaker state tracker.
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

    /// Check rate limit using in-memory fixed-window counter with atomic CAS.
    /// Returns Ok(remaining) or Err(retry_after_secs).
    pub fn check_rate_limit(
        &self,
        route_id: &Uuid,
        client_identity: &str,
        requests_per_second: i32,
    ) -> Result<i32, u64> {
        let key = format!("{}:{}", route_id, client_identity);
        let current_window = self.rate_limiter_epoch.elapsed().as_secs() as u32;
        let limit = requests_per_second as u32;

        // Periodic cleanup: evict stale entries (per-shard locks, not global)
        let ops = self.rate_limiter_ops.fetch_add(1, Ordering::Relaxed);
        if ops % RATE_LIMITER_CLEANUP_INTERVAL == 0 || self.rate_limiters.len() > RATE_LIMITER_MAX_KEYS {
            self.rate_limiters.retain(|_, v| {
                let (win, _) = unpack(v.load(Ordering::Relaxed));
                win >= current_window.saturating_sub(1)
            });
        }

        // Use entry API directly — avoids double lookup (get then entry)
        let entry = self
            .rate_limiters
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0));
        Self::cas_increment(&entry, current_window, limit)
    }

    /// Atomically increment the counter for the current window via CAS loop.
    /// Returns Ok(remaining) or Err(retry_after_secs).
    fn cas_increment(
        counter: &AtomicU64,
        current_window: u32,
        limit: u32,
    ) -> Result<i32, u64> {
        loop {
            let current = counter.load(Ordering::Acquire);
            let (win, count) = unpack(current);

            if win != current_window {
                // Window expired — reset to count=1
                let new_val = pack(current_window, 1);
                match counter.compare_exchange_weak(
                    current,
                    new_val,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Ok(limit as i32 - 1),
                    Err(_) => continue, // Retry CAS
                }
            } else if count >= limit {
                // Over limit
                return Err(1);
            } else {
                // Under limit — increment
                let new_val = pack(current_window, count + 1);
                match counter.compare_exchange_weak(
                    current,
                    new_val,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Ok(limit as i32 - (count + 1) as i32),
                    Err(_) => continue, // Retry CAS
                }
            }
        }
    }
}

/// Redis state backend (multi-instance mode).
#[cfg(feature = "redis-backend")]
pub struct RedisState {
    pool: deadpool_redis::Pool,
    /// Local circuit breaker — Redis syncs state transitions across instances.
    circuit_breaker: Arc<CircuitBreaker>,
}

#[cfg(feature = "redis-backend")]
impl RedisState {
    pub fn new(pool: deadpool_redis::Pool, circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            pool,
            circuit_breaker,
        }
    }

    /// Check rate limit via Redis Lua script (atomic INCR + conditional EXPIRE).
    /// Fails open on Redis errors (returns Ok with remaining = limit).
    pub async fn check_rate_limit(
        &self,
        route_id: &Uuid,
        client_identity: &str,
        requests_per_second: i32,
    ) -> Result<i32, u64> {
        let key = format!("gate:rl:{}:{}", route_id, client_identity);
        let limit = requests_per_second;

        let result: Result<i32, Box<dyn std::error::Error + Send + Sync>> = async {
            let mut conn = self.pool.get().await.map_err(|e| {
                tracing::warn!(error = %e, "Redis pool error (rate limit) — failing open");
                crate::metrics::REDIS_ERRORS_TOTAL
                    .with_label_values(&["rate_limit"])
                    .inc();
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

            let current: i32 = redis::Script::new(
                r#"
                local current = redis.call('INCR', KEYS[1])
                if current == 1 then
                    redis.call('EXPIRE', KEYS[1], 1)
                end
                return current
                "#,
            )
            .key(&key)
            .invoke_async(&mut *conn)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Redis script error (rate limit) — failing open");
                crate::metrics::REDIS_ERRORS_TOTAL
                    .with_label_values(&["rate_limit"])
                    .inc();
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

            Ok(current)
        }
        .await;

        match result {
            Ok(current) => {
                if current > limit {
                    Err(1)
                } else {
                    Ok(limit - current)
                }
            }
            Err(_) => {
                // Fail-open: allow the request through
                Ok(limit)
            }
        }
    }

    /// Poll Redis for circuit breaker state from other instances and sync locally.
    pub async fn sync_cb_states(&self) {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
            let mut conn = self.pool.get().await?;

            // Use SCAN instead of KEYS to avoid blocking the Redis server
            let mut cursor: u64 = 0;
            let mut all_keys: Vec<String> = Vec::new();
            loop {
                let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg("gate:cb:*")
                    .arg("COUNT")
                    .arg(100)
                    .query_async(&mut *conn)
                    .await?;
                all_keys.extend(keys);
                cursor = next_cursor;
                if cursor == 0 {
                    break;
                }
            }

            if all_keys.is_empty() {
                return Ok(());
            }

            // Batch fetch all values with MGET instead of N individual GETs
            let values: Vec<Option<String>> = redis::cmd("MGET")
                .arg(&all_keys)
                .query_async(&mut *conn)
                .await?;

            for (key, value) in all_keys.iter().zip(values) {
                let target_id_str = key.strip_prefix("gate:cb:").unwrap_or(key);
                let Ok(target_id) = target_id_str.parse::<Uuid>() else {
                    continue;
                };

                if let Some(val) = value {
                    // Parse "state:failures"
                    if let Some((state_str, _)) = val.split_once(':') {
                        if let Ok(state) = state_str.parse::<State>() {
                            self.circuit_breaker.set_state(&target_id, state);
                        }
                    }
                }
            }
            Ok(())
        }
        .await;

        if let Err(e) = result {
            tracing::warn!(error = %e, "Failed to sync CB states from Redis");
            crate::metrics::REDIS_ERRORS_TOTAL
                .with_label_values(&["cb_sync"])
                .inc();
        }
    }
}

/// Publish a circuit breaker state transition to Redis (fire-and-forget).
#[cfg(feature = "redis-backend")]
async fn publish_cb_state(pool: &deadpool_redis::Pool, target_id: &Uuid, state: State) {
    let key = format!("gate:cb:{}", target_id);
    let value = format!("{}:0", state);

    let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
        let mut conn = pool.get().await.map_err(|e| {
            crate::metrics::REDIS_ERRORS_TOTAL
                .with_label_values(&["cb_publish"])
                .inc();
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;
        // Set with 60s TTL — stale entries auto-expire
        let _: () = redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("EX")
            .arg(60)
            .query_async(&mut *conn)
            .await
            .map_err(|e| {
                crate::metrics::REDIS_ERRORS_TOTAL
                    .with_label_values(&["cb_publish"])
                    .inc();
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;
        Ok(())
    }
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "Failed to publish CB state to Redis");
    }
}

/// Unified state backend — dispatches to Memory or Redis based on config.
pub enum StateBackend {
    Memory(MemoryState),
    #[cfg(feature = "redis-backend")]
    Redis(RedisState),
}

impl StateBackend {
    /// Check rate limit. Delegates to the active backend.
    /// For Memory: synchronous (fast path). For Redis: async with fail-open.
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
            #[cfg(feature = "redis-backend")]
            StateBackend::Redis(redis) => {
                redis
                    .check_rate_limit(route_id, client_identity, requests_per_second)
                    .await
            }
        }
    }

    /// Get a reference to the circuit breaker.
    pub fn circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        match self {
            StateBackend::Memory(mem) => &mem.circuit_breaker,
            #[cfg(feature = "redis-backend")]
            StateBackend::Redis(redis) => &redis.circuit_breaker,
        }
    }

    /// Record a circuit breaker failure and optionally publish to Redis.
    pub fn record_cb_failure(&self, target_id: &Uuid) -> bool {
        let tripped = self.circuit_breaker().record_failure(target_id);
        #[cfg(feature = "redis-backend")]
        if tripped {
            if let StateBackend::Redis(redis) = self {
                let target_id = *target_id;
                let pool = redis.pool.clone();
                tokio::spawn(async move {
                    publish_cb_state(&pool, &target_id, State::Open).await;
                });
            }
        }
        tripped
    }

    /// Record a circuit breaker success and optionally publish to Redis.
    pub fn record_cb_success(&self, target_id: &Uuid) {
        let _transitioned = self.circuit_breaker().record_success(target_id);
        // Only publish to Redis on actual HalfOpen → Closed transitions
        #[cfg(feature = "redis-backend")]
        if _transitioned {
            if let StateBackend::Redis(redis) = self {
                let target_id = *target_id;
                let pool = redis.pool.clone();
                tokio::spawn(async move {
                    publish_cb_state(&pool, &target_id, State::Closed).await;
                });
            }
        }
    }

    /// Returns true if using Redis backend.
    pub fn is_redis(&self) -> bool {
        match self {
            StateBackend::Memory(_) => false,
            #[cfg(feature = "redis-backend")]
            StateBackend::Redis(_) => true,
        }
    }

    /// Spawn the circuit breaker sync background task (Redis only).
    /// Polls Redis every 2 seconds for CB state changes from other instances.
    #[cfg(feature = "redis-backend")]
    pub fn spawn_cb_sync_task(state: Arc<StateBackend>) {
        if let StateBackend::Redis(_) = &*state {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to build CB sync runtime");

                rt.block_on(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        if let StateBackend::Redis(redis) = &*state {
                            redis.sync_cb_states().await;
                        }
                    }
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory_state() -> StateBackend {
        StateBackend::Memory(MemoryState::new(Arc::new(CircuitBreaker::new())))
    }

    #[tokio::test]
    async fn memory_check_rate_limit_allows_within_limit() {
        let state = make_memory_state();
        let route_id = Uuid::new_v4();

        let result = state.check_rate_limit(&route_id, "client1", 10).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 9);
    }

    #[tokio::test]
    async fn memory_check_rate_limit_blocks_over_limit() {
        let state = make_memory_state();
        let route_id = Uuid::new_v4();

        let r1 = state.check_rate_limit(&route_id, "client1", 2).await;
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap(), 1);

        let r2 = state.check_rate_limit(&route_id, "client1", 2).await;
        assert!(r2.is_ok());
        assert_eq!(r2.unwrap(), 0);

        let r3 = state.check_rate_limit(&route_id, "client1", 2).await;
        assert!(r3.is_err());
    }

    #[tokio::test]
    async fn memory_separate_clients_independent() {
        let state = make_memory_state();
        let route_id = Uuid::new_v4();

        let r1 = state.check_rate_limit(&route_id, "client1", 1).await;
        let r2 = state.check_rate_limit(&route_id, "client2", 1).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn memory_separate_routes_independent() {
        let state = make_memory_state();
        let route1 = Uuid::new_v4();
        let route2 = Uuid::new_v4();

        let r1 = state.check_rate_limit(&route1, "client1", 1).await;
        let r2 = state.check_rate_limit(&route2, "client1", 1).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[test]
    fn circuit_breaker_accessible() {
        let state = make_memory_state();
        let cb = state.circuit_breaker();
        let id = Uuid::new_v4();
        cb.configure(id, 3, 30);
        assert!(cb.is_available(&id));
    }

    #[test]
    fn is_redis_returns_false_for_memory() {
        let state = make_memory_state();
        assert!(!state.is_redis());
    }
}
