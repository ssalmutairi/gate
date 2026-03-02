import { render, screen, fireEvent } from '@testing-library/react'
import { Button } from '../ui/button'
import { Input } from '../ui/input'
import { Badge } from '../ui/badge'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../ui/dialog'
import { EmptyState } from '../ui'

describe('Button', () => {
  it('renders with children', () => {
    render(<Button>Click me</Button>)
    expect(screen.getByRole('button', { name: /click me/i })).toBeInTheDocument()
  })

  it('applies variant classes', () => {
    const { container } = render(<Button variant="destructive">Delete</Button>)
    expect(container.firstChild).toHaveClass('bg-destructive')
  })

  it('applies size classes', () => {
    const { container } = render(<Button size="sm">Small</Button>)
    expect(container.firstChild).toHaveClass('h-8')
  })

  it('calls onClick handler', () => {
    const onClick = vi.fn()
    render(<Button onClick={onClick}>Click</Button>)
    fireEvent.click(screen.getByRole('button'))
    expect(onClick).toHaveBeenCalledTimes(1)
  })

  it('is disabled when disabled prop is set', () => {
    render(<Button disabled>Disabled</Button>)
    expect(screen.getByRole('button')).toBeDisabled()
  })
})

describe('Input', () => {
  it('renders input element', () => {
    const { container } = render(<Input placeholder="Enter" />)
    expect(container.querySelector('input')).toBeInTheDocument()
  })

  it('accepts placeholder', () => {
    render(<Input placeholder="Enter value" />)
    expect(screen.getByPlaceholderText('Enter value')).toBeInTheDocument()
  })
})

describe('Badge', () => {
  it('renders children', () => {
    render(<Badge>Active</Badge>)
    expect(screen.getByText('Active')).toBeInTheDocument()
  })

  it('applies variant classes', () => {
    const { container } = render(<Badge variant="success">OK</Badge>)
    expect(container.firstChild).toHaveClass('bg-success/10')
  })
})

describe('Dialog (Modal replacement)', () => {
  it('renders when open', () => {
    render(
      <Dialog open={true} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Test Modal</DialogTitle>
          </DialogHeader>
          <p>Modal content</p>
        </DialogContent>
      </Dialog>
    )
    expect(screen.getByText('Test Modal')).toBeInTheDocument()
    expect(screen.getByText('Modal content')).toBeInTheDocument()
  })

  it('does not render when closed', () => {
    render(
      <Dialog open={false} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Hidden</DialogTitle>
          </DialogHeader>
          <p>Hidden content</p>
        </DialogContent>
      </Dialog>
    )
    expect(screen.queryByText('Hidden')).not.toBeInTheDocument()
  })
})

describe('Dialog as ConfirmDialog', () => {
  it('renders when open', () => {
    render(
      <Dialog open={true} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm Delete</DialogTitle>
            <DialogDescription>Are you sure?</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary">Cancel</Button>
            <Button variant="destructive">Delete</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )
    expect(screen.getByText('Confirm Delete')).toBeInTheDocument()
    expect(screen.getByText('Are you sure?')).toBeInTheDocument()
  })

  it('calls onClick when confirm button is clicked', () => {
    const onConfirm = vi.fn()
    render(
      <Dialog open={true} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm</DialogTitle>
            <DialogDescription>Sure?</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary">Cancel</Button>
            <Button variant="destructive" onClick={onConfirm}>Yes, Delete</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )
    fireEvent.click(screen.getByText('Yes, Delete'))
    expect(onConfirm).toHaveBeenCalledTimes(1)
  })

  it('calls onOpenChange when cancel is clicked', () => {
    const onClose = vi.fn()
    render(
      <Dialog open={true} onOpenChange={onClose}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm</DialogTitle>
            <DialogDescription>Sure?</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => onClose(false)}>Cancel</Button>
            <Button variant="destructive">Delete</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    )
    fireEvent.click(screen.getByText('Cancel'))
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})

describe('EmptyState', () => {
  it('renders message', () => {
    render(<EmptyState message="No items found" />)
    expect(screen.getByText('No items found')).toBeInTheDocument()
  })

  it('renders action button', () => {
    render(
      <EmptyState
        message="No items"
        action={<Button>Add Item</Button>}
      />
    )
    expect(screen.getByRole('button', { name: /add item/i })).toBeInTheDocument()
  })
})
