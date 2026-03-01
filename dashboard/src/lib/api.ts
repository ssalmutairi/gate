import axios from 'axios';

const api = axios.create({
  baseURL: '/admin',
  headers: {
    'Content-Type': 'application/json',
  },
});

// Add admin token interceptor
api.interceptors.request.use((config) => {
  const token = localStorage.getItem('admin_token');
  if (token) {
    config.headers['X-Admin-Token'] = token;
  }
  return config;
});

// Types
export interface Upstream {
  id: string;
  name: string;
  algorithm: string;
  active: boolean;
  targets?: Target[];
  created_at: string;
  updated_at: string;
}

export interface Target {
  id: string;
  upstream_id: string;
  host: string;
  port: number;
  weight: number;
  healthy: boolean;
  created_at: string;
  updated_at: string;
}

export interface Route {
  id: string;
  name: string;
  path_prefix: string;
  methods: string[] | null;
  upstream_id: string;
  upstream_name?: string;
  strip_prefix: boolean;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface ApiKey {
  id: string;
  name: string;
  key_hash: string;
  route_id: string | null;
  active: boolean;
  expires_at: string | null;
  created_at: string;
  updated_at: string;
  plaintext_key?: string;
}

export interface RateLimit {
  id: string;
  route_id: string;
  route_name?: string;
  requests_per_second: number;
  requests_per_minute: number | null;
  requests_per_hour: number | null;
  limit_by: string;
  created_at: string;
  updated_at: string;
}

// Paginated response wrapper
interface Paginated<T> { data: T[]; total: number; page: number; limit: number; }

// Upstreams
export const getUpstreams = () => api.get<Paginated<Upstream>>('/upstreams').then(r => r.data.data);
export const createUpstream = (data: { name: string; algorithm?: string }) =>
  api.post<Upstream>('/upstreams', data).then(r => r.data);
export const updateUpstream = (id: string, data: { name?: string; algorithm?: string; active?: boolean }) =>
  api.put<Upstream>(`/upstreams/${id}`, data).then(r => r.data);
export const deleteUpstream = (id: string) => api.delete(`/upstreams/${id}`);

// Targets
export const createTarget = (upstreamId: string, data: { host: string; port: number; weight?: number }) =>
  api.post<Target>(`/upstreams/${upstreamId}/targets`, data).then(r => r.data);
export const deleteTarget = (upstreamId: string, targetId: string) =>
  api.delete(`/upstreams/${upstreamId}/targets/${targetId}`);

// Routes
export const getRoutes = () => api.get<Paginated<Route>>('/routes').then(r => r.data.data);
export const createRoute = (data: {
  name: string;
  path_prefix: string;
  methods?: string[];
  upstream_id: string;
  strip_prefix?: boolean;
}) => api.post<Route>('/routes', data).then(r => r.data);
export const updateRoute = (id: string, data: Partial<Route>) =>
  api.put<Route>(`/routes/${id}`, data).then(r => r.data);
export const deleteRoute = (id: string) => api.delete(`/routes/${id}`);

// API Keys
export const getApiKeys = () => api.get<Paginated<ApiKey>>('/api-keys').then(r => r.data.data);
export const createApiKey = (data: {
  name: string;
  route_id?: string;
  expires_at?: string;
}) => api.post<ApiKey>('/api-keys', data).then(r => r.data);
export const updateApiKey = (id: string, data: { active?: boolean }) =>
  api.put<ApiKey>(`/api-keys/${id}`, data).then(r => r.data);
export const deleteApiKey = (id: string) => api.delete(`/api-keys/${id}`);

// Rate Limits
export const getRateLimits = () => api.get<Paginated<RateLimit>>('/rate-limits').then(r => r.data.data);
export const createRateLimit = (data: {
  route_id: string;
  requests_per_second: number;
  requests_per_minute?: number;
  requests_per_hour?: number;
  limit_by?: string;
}) => api.post<RateLimit>('/rate-limits', data).then(r => r.data);
export const updateRateLimit = (id: string, data: Partial<RateLimit>) =>
  api.put<RateLimit>(`/rate-limits/${id}`, data).then(r => r.data);
export const deleteRateLimit = (id: string) => api.delete(`/rate-limits/${id}`);

// Health
export const getHealth = () => api.get<{ status: string }>('/health').then(r => r.data);

// Stats
export interface Stats {
  total_requests_today: number;
  error_rate: number;
  avg_latency_ms: number;
  p95_latency_ms: number;
  active_routes: number;
}
export const getStats = () => api.get<Stats>('/stats').then(r => r.data);

// Logs
export interface LogEntry {
  id: string;
  route_id: string | null;
  method: string;
  path: string;
  status_code: number;
  latency_ms: number;
  client_ip: string;
  upstream_target: string | null;
  created_at: string;
}
export const getLogs = (params?: { page?: number; limit?: number; route_id?: string; status?: number; method?: string }) =>
  api.get<Paginated<LogEntry>>('/logs', { params }).then(r => r.data);

export default api;
