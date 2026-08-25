import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { StatCard } from '../StatCard'

const icon = <span data-testid="icon">i</span>

describe('StatCard', () => {
  it('renders the title and value', () => {
    render(<StatCard title="活跃用户" value={42} icon={icon} />)
    expect(screen.getByText('活跃用户')).toBeInTheDocument()
    expect(screen.getByText('42')).toBeInTheDocument()
  })

  it('shows an up-trend (with abs value) for a positive trend', () => {
    const { container } = render(<StatCard title="T" value={1} icon={icon} trend={{ value: 12 }} />)
    expect(container.querySelector('.dash-trend-up')).toBeInTheDocument()
    expect(screen.getByText('12%')).toBeInTheDocument()
  })

  it('shows a down-trend (with abs value) for a negative trend', () => {
    const { container } = render(<StatCard title="T" value={1} icon={icon} trend={{ value: -8 }} />)
    expect(container.querySelector('.dash-trend-down')).toBeInTheDocument()
    // Math.abs(-8) = 8
    expect(screen.getByText('8%')).toBeInTheDocument()
  })

  it('does not render a trend badge when no trend is given', () => {
    const { container } = render(<StatCard title="T" value={1} icon={icon} />)
    expect(container.querySelector('.dash-trend-up')).toBeNull()
    expect(container.querySelector('.dash-trend-down')).toBeNull()
  })

  it('renders the trend label when no spark data is provided', () => {
    render(<StatCard title="T" value={1} icon={icon} trend={{ value: 1, label: '周环比' }} />)
    expect(screen.getByText('周环比')).toBeInTheDocument()
  })

  it('renders an inline sparkline svg when spark data is provided', () => {
    const { container } = render(<StatCard title="T" value={1} icon={icon} spark={[1, 4, 2, 8, 5]} />)
    // Sparkline 纯 SVG，无 canvas 依赖
    expect(container.querySelector('svg')).toBeInTheDocument()
  })
})
