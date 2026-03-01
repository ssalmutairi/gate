import { vi, describe, it, expect, beforeEach } from 'vitest'
import axios from 'axios'
import type { AxiosInstance } from 'axios'

// Mock axios
vi.mock('axios', () => {
  const mockInstance = {
    get: vi.fn().mockResolvedValue({ data: {} }),
    post: vi.fn().mockResolvedValue({ data: {} }),
    put: vi.fn().mockResolvedValue({ data: {} }),
    delete: vi.fn().mockResolvedValue({ data: {} }),
    interceptors: {
      request: { use: vi.fn() },
      response: { use: vi.fn() },
    },
  }
  return {
    default: {
      create: vi.fn(() => mockInstance),
    },
  }
})

// Import after mock setup so the module uses the mocked axios
let api: AxiosInstance
let apiModule: typeof import('../../lib/api')

beforeEach(async () => {
  vi.resetModules()
  const mod = await import('../../lib/api')
  apiModule = mod
  api = (axios.create as ReturnType<typeof vi.fn>).mock.results[
    (axios.create as ReturnType<typeof vi.fn>).mock.results.length - 1
  ].value
  vi.mocked(api.get).mockResolvedValue({ data: { data: [], total: 0, page: 1, limit: 20 } })
  vi.mocked(api.post).mockResolvedValue({ data: {} })
  vi.mocked(api.put).mockResolvedValue({ data: {} })
  vi.mocked(api.delete).mockResolvedValue({ data: {} })
})

describe('Upstreams API', () => {
  it('getUpstreams calls GET /upstreams', async () => {
    await apiModule.getUpstreams()
    expect(api.get).toHaveBeenCalledWith('/upstreams')
  })

  it('createUpstream calls POST /upstreams', async () => {
    await apiModule.createUpstream({ name: 'test' })
    expect(api.post).toHaveBeenCalledWith('/upstreams', { name: 'test' })
  })

  it('updateUpstream calls PUT /upstreams/:id', async () => {
    await apiModule.updateUpstream('abc', { name: 'updated' })
    expect(api.put).toHaveBeenCalledWith('/upstreams/abc', { name: 'updated' })
  })

  it('deleteUpstream calls DELETE /upstreams/:id', async () => {
    await apiModule.deleteUpstream('abc')
    expect(api.delete).toHaveBeenCalledWith('/upstreams/abc')
  })
})

describe('Routes API', () => {
  it('getRoutes calls GET /routes', async () => {
    await apiModule.getRoutes()
    expect(api.get).toHaveBeenCalledWith('/routes')
  })

  it('createRoute calls POST /routes', async () => {
    const data = { name: 'r1', path_prefix: '/api', upstream_id: 'u1' }
    await apiModule.createRoute(data)
    expect(api.post).toHaveBeenCalledWith('/routes', data)
  })

  it('deleteRoute calls DELETE /routes/:id', async () => {
    await apiModule.deleteRoute('r1')
    expect(api.delete).toHaveBeenCalledWith('/routes/r1')
  })
})

describe('API Keys API', () => {
  it('getApiKeys calls GET /api-keys', async () => {
    await apiModule.getApiKeys()
    expect(api.get).toHaveBeenCalledWith('/api-keys')
  })

  it('createApiKey calls POST /api-keys', async () => {
    await apiModule.createApiKey({ name: 'key1' })
    expect(api.post).toHaveBeenCalledWith('/api-keys', { name: 'key1' })
  })

  it('deleteApiKey calls DELETE /api-keys/:id', async () => {
    await apiModule.deleteApiKey('k1')
    expect(api.delete).toHaveBeenCalledWith('/api-keys/k1')
  })
})

describe('Rate Limits API', () => {
  it('getRateLimits calls GET /rate-limits', async () => {
    await apiModule.getRateLimits()
    expect(api.get).toHaveBeenCalledWith('/rate-limits')
  })

  it('createRateLimit calls POST /rate-limits', async () => {
    const data = { route_id: 'r1', requests_per_second: 100 }
    await apiModule.createRateLimit(data)
    expect(api.post).toHaveBeenCalledWith('/rate-limits', data)
  })

  it('deleteRateLimit calls DELETE /rate-limits/:id', async () => {
    await apiModule.deleteRateLimit('rl1')
    expect(api.delete).toHaveBeenCalledWith('/rate-limits/rl1')
  })
})

describe('Services API', () => {
  it('getServices calls GET /services', async () => {
    await apiModule.getServices({ search: 'test' })
    expect(api.get).toHaveBeenCalledWith('/services', { params: { search: 'test' } })
  })

  it('importService calls POST /services/import', async () => {
    const data = { url: 'http://example.com/spec.json', namespace: 'test' }
    await apiModule.importService(data)
    expect(api.post).toHaveBeenCalledWith('/services/import', data)
  })

  it('updateService calls PUT /services/:id', async () => {
    await apiModule.updateService('s1', { status: 'deprecated' })
    expect(api.put).toHaveBeenCalledWith('/services/s1', { status: 'deprecated' })
  })

  it('deleteService calls DELETE /services/:id', async () => {
    await apiModule.deleteService('s1')
    expect(api.delete).toHaveBeenCalledWith('/services/s1')
  })
})

describe('Header Rules API', () => {
  it('getHeaderRules calls GET /routes/:id/header-rules', async () => {
    await apiModule.getHeaderRules('r1')
    expect(api.get).toHaveBeenCalledWith('/routes/r1/header-rules')
  })

  it('createHeaderRule calls POST /routes/:id/header-rules', async () => {
    const data = { action: 'set', header_name: 'X-Test', header_value: 'val' }
    await apiModule.createHeaderRule('r1', data)
    expect(api.post).toHaveBeenCalledWith('/routes/r1/header-rules', data)
  })

  it('deleteHeaderRule calls DELETE /header-rules/:id', async () => {
    await apiModule.deleteHeaderRule('hr1')
    expect(api.delete).toHaveBeenCalledWith('/header-rules/hr1')
  })
})

describe('Health & Stats API', () => {
  it('getHealth calls GET /health', async () => {
    vi.mocked(api.get).mockResolvedValue({ data: { status: 'ok' } })
    await apiModule.getHealth()
    expect(api.get).toHaveBeenCalledWith('/health')
  })

  it('getStats calls GET /stats', async () => {
    vi.mocked(api.get).mockResolvedValue({ data: { total_requests_today: 0 } })
    await apiModule.getStats()
    expect(api.get).toHaveBeenCalledWith('/stats')
  })

  it('getLogs calls GET /logs with params', async () => {
    vi.mocked(api.get).mockResolvedValue({ data: { data: [], total: 0, page: 1, limit: 50 } })
    await apiModule.getLogs({ page: 2, method: 'GET' })
    expect(api.get).toHaveBeenCalledWith('/logs', { params: { page: 2, method: 'GET' } })
  })
})
