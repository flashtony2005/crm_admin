import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Sparkline } from '../Sparkline'

describe('Sparkline', () => {
  it('returns nothing for empty data', () => {
    const { container } = render(<Sparkline data={[]} />)
    expect(container.querySelector('svg')).toBeNull()
  })

  it('renders an svg polyline path for non-empty data', () => {
    const { container } = render(<Sparkline data={[1, 3, 2, 5]} />)
    const svg = container.querySelector('svg')
    expect(svg).toBeInTheDocument()
    // 折线 path：以 M 起点、L 连接
    const line = container.querySelector('path[stroke]')
    expect(line?.getAttribute('d')?.startsWith('M')).toBe(true)
  })

  it('applies the configured stroke color', () => {
    const { container } = render(<Sparkline data={[1, 2, 3]} color="#FF0000" />)
    // line path 的 stroke 即传入的 color；area path 为 stroke="none"
    const line = container.querySelector('path[stroke="#FF0000"]')
    expect(line).not.toBeNull()
  })

  it('omits the area fill when showArea is false', () => {
    const { container } = render(<Sparkline data={[1, 2, 3]} showArea={false} />)
    // area path 用 fill=url(#spark-...) 标识；关闭后不应存在
    expect(container.querySelector('path[fill^="url"]')).toBeNull()
  })
})
