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

// Auto-logout on 401 responses
api.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      window.dispatchEvent(new Event('auth:logout'));
    }
    return Promise.reject(error);
  }
);

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
  tls: boolean;
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
  upstream_path_prefix: string | null;
  service_id: string | null;
  max_body_bytes: number | null;
  auth_skip: boolean;
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
  key?: string;
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
export const getHealth = () => api.get<{ status: string; version: string }>('/health').then(r => r.data);

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

// Services
export interface Service {
  id: string;
  namespace: string;
  version: number;
  spec_url: string;
  spec_hash: string;
  upstream_id: string;
  route_id: string | null;
  description: string;
  tags: string[];
  status: string;
  created_at: string;
  updated_at: string;
}

export const getServices = (params?: { search?: string; status?: string }) =>
  api.get<Paginated<Service>>('/services', { params }).then(r => r.data.data);
export const getService = (id: string) =>
  api.get<Service>(`/services/${id}`).then(r => r.data);
export const getServiceSpec = (id: string) =>
  api.get<any>(`/services/${id}/spec`).then(r => r.data);
export const importService = (data: { url?: string; spec_content?: string; namespace: string; description?: string; tags?: string[]; status?: string }) =>
  api.post<Service>('/services/import', data).then(r => r.data);
export const updateService = (id: string, data: { description?: string; tags?: string[]; status?: string }) =>
  api.put<Service>(`/services/${id}`, data).then(r => r.data);
export const deleteService = (id: string) => api.delete(`/services/${id}`);

// Header Rules
export interface HeaderRule {
  id: string;
  route_id: string;
  phase: string;
  action: string;
  header_name: string;
  header_value: string | null;
  created_at: string;
  updated_at: string;
}

export const getHeaderRules = (routeId: string) =>
  api.get<HeaderRule[]>(`/routes/${routeId}/header-rules`).then(r => r.data);
export const createHeaderRule = (routeId: string, data: { phase?: string; action: string; header_name: string; header_value?: string }) =>
  api.post<HeaderRule>(`/routes/${routeId}/header-rules`, data).then(r => r.data);
export const deleteHeaderRule = (id: string) => api.delete(`/header-rules/${id}`);

export default api;
