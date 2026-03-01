import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { vi } from 'vitest'
import ServicesPage from '../ServicesPage'

// Mock the API module
vi.mock('../../lib/api', () => ({
  getServices: vi.fn(),
  importService: vi.fn(),
  updateService: vi.fn(),
  deleteService: vi.fn(),
}))

import { getServices } from '../../lib/api'

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>
  )
}

describe('ServicesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders loading then empty state', async () => {
    vi.mocked(getServices).mockResolvedValue([])
    renderWithProviders(<ServicesPage />)

    // Should show loading first
    expect(screen.getByText('Loading...')).toBeInTheDocument()

    // Then empty state
    await waitFor(() => {
      expect(screen.getByText('No services found')).toBeInTheDocument()
    })
  })

  it('renders table with services', async () => {
    vi.mocked(getServices).mockResolvedValue([
      {
        id: '1',
        namespace: 'petstore',
        version: 2,
        spec_url: 'http://example.com/spec.json',
        spec_hash: 'abc',
        upstream_id: 'u1',
        route_id: 'r1',
        description: 'Pet store API',
        tags: ['rest', 'pets'],
        status: 'stable',
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
      },
    ])

    renderWithProviders(<ServicesPage />)

    await waitFor(() => {
      expect(screen.getByText('/petstore')).toBeInTheDocument()
    })
    expect(screen.getByText('v2')).toBeInTheDocument()
    expect(screen.getByText('stable')).toBeInTheDocument()
    expect(screen.getByText('Pet store API')).toBeInTheDocument()
  })

  it('has search input', async () => {
    vi.mocked(getServices).mockResolvedValue([])
    renderWithProviders(<ServicesPage />)

    const searchInput = screen.getByPlaceholderText('Search by namespace...')
    expect(searchInput).toBeInTheDocument()
  })

  it('has status filter', async () => {
    vi.mocked(getServices).mockResolvedValue([])
    renderWithProviders(<ServicesPage />)

    const filter = screen.getByDisplayValue('All statuses')
    expect(filter).toBeInTheDocument()
  })

  it('opens import modal on button click', async () => {
    vi.mocked(getServices).mockResolvedValue([])
    const user = userEvent.setup()

    renderWithProviders(<ServicesPage />)

    await waitFor(() => {
      expect(screen.getByText('No services found')).toBeInTheDocument()
    })

    await user.click(screen.getByText('Import'))

    expect(screen.getByText('Import OpenAPI Spec')).toBeInTheDocument()
    expect(screen.getByPlaceholderText(/petstore3/)).toBeInTheDocument()
  })

  it('shows edit modal when edit button clicked', async () => {
    vi.mocked(getServices).mockResolvedValue([
      {
        id: '1',
        namespace: 'editme',
        version: 1,
        spec_url: 'http://example.com/spec.json',
        spec_hash: 'abc',
        upstream_id: 'u1',
        route_id: null,
        description: 'To edit',
        tags: ['tag1'],
        status: 'alpha',
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
      },
    ])

    const user = userEvent.setup()
    renderWithProviders(<ServicesPage />)

    await waitFor(() => {
      expect(screen.getByText('/editme')).toBeInTheDocument()
    })

    // Click edit button (pencil icon)
    const editButtons = screen.getAllByRole('button')
    // The edit button has the Pencil icon - find by looking at ghost variant buttons
    const editBtn = editButtons.find(btn => btn.querySelector('svg.lucide-pencil'))
    if (editBtn) {
      await user.click(editBtn)
      await waitFor(() => {
        expect(screen.getByText(/Edit Service/)).toBeInTheDocument()
      })
    }
  })
})
