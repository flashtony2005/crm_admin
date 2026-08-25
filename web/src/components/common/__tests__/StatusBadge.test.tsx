import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { StatusBadge, toneFromStatus } from '../StatusBadge'

describe('toneFromStatus', () => {
  it('maps positive states to success', () => {
    expect(toneFromStatus('active')).toBe('success')
    expect(toneFromStatus('online')).toBe('success')
    expect(toneFromStatus('done')).toBe('success')
  })

  it('maps negative states to danger', () => {
    expect(toneFromStatus('inactive')).toBe('danger')
    expect(toneFromStatus('deleted')).toBe('danger')
    expect(toneFromStatus('failed')).toBe('danger')
  })

  it('maps in-progress states to info', () => {
    expect(toneFromStatus('processing')).toBe('info')
    expect(toneFromStatus('running')).toBe('info')
  })

  it('maps pending states to purple', () => {
    expect(toneFromStatus('pending')).toBe('purple')
    expect(toneFromStatus('review')).toBe('purple')
  })

  it('maps warning states to warning', () => {
    expect(toneFromStatus('unfulfilled')).toBe('warning')
    expect(toneFromStatus('suspended')).toBe('warning')
  })

  it('is case-insensitive', () => {
    expect(toneFromStatus('ACTIVE')).toBe('success')
    expect(toneFromStatus('Deleted')).toBe('danger')
  })

  it('falls back to neutral for empty/unknown values', () => {
    expect(toneFromStatus(null)).toBe('neutral')
    expect(toneFromStatus(undefined)).toBe('neutral')
    expect(toneFromStatus('weird-status')).toBe('neutral')
  })
})

describe('StatusBadge', () => {
  it('renders children', () => {
    render(<StatusBadge tone="success">Healthy</StatusBadge>)
    expect(screen.getByText('Healthy')).toBeInTheDocument()
  })

  it('applies the tone class to the wrapper', () => {
    const { container } = render(<StatusBadge tone="danger">Down</StatusBadge>)
    expect(container.firstChild).toHaveClass('bg-os-danger-bg')
  })

  it('renders a leading dot when dot is set', () => {
    const { container } = render(
      <StatusBadge tone="info" dot>
        Running
      </StatusBadge>,
    )
    // The dot is the first child element inside the badge.
    expect(container.querySelector('span > span')).toBeInTheDocument()
  })
})
