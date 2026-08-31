import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { JsonForm } from '../JsonForm'

/** 与 db.rs 种子一致的真实区块结构（简化版） */
const ABOUT = {
  kicker: 'KOSX.ai 社群增长操盘手',
  tagline: 'Community Growth',
  hero: { title: '把连接，变成看得见的机会。', bio: '短简介', ctaLabel: '查看主页', ctaHref: 'https://x.com/KeKeYa88' },
}

const ORG = {
  affiliation: { eyebrow: '当前组织', name: 'KOSX.ai', logo: 'https://coucouya.com/logo.png', desc: '描述' },
  pillars: {
    eyebrow: '小标题',
    title: '标题',
    items: [{ no: '01', title: 'AI 带来新的能力' }, { no: '02', title: '人才带来创造力' }],
  },
}

describe('JsonForm', () => {
  it('对象字段渲染为中文标签，并可编辑叶子值', () => {
    const onChange = vi.fn()
    render(<JsonForm value={ABOUT} onChange={onChange} />)

    // 中文标签（LABEL_ZH 映射）
    expect(screen.getByText('眉标')).toBeTruthy()
    expect(screen.getByText('标语')).toBeTruthy()
    expect(screen.getByText('主视觉')).toBeTruthy()

    // 嵌套对象里的字段也渲染出来
    expect(screen.getByText('按钮文字')).toBeTruthy()

    const input = screen.getByDisplayValue('KOSX.ai 社群增长操盘手')
    fireEvent.change(input, { target: { value: '新眉标' } })
    expect(onChange).toHaveBeenCalled()
    const next = onChange.mock.calls[0][0] as typeof ABOUT
    expect(next.kicker).toBe('新眉标')
    // 不影响其他字段
    expect(next.tagline).toBe('Community Growth')
  })

  it('数组渲染为可增删排序的列表', () => {
    const onChange = vi.fn()
    render(<JsonForm value={ORG.pillars.items} onChange={onChange} />)

    expect(screen.getByText('#1')).toBeTruthy()
    expect(screen.getByText('#2')).toBeTruthy()

    // 上移第 2 项 → 顺序交换
    const secondBlock = screen.getByText('#2').closest('div')!.parentElement!
    const upBtn = secondBlock.querySelector('[aria-label="上移"]') as HTMLButtonElement
    fireEvent.click(upBtn)
    const swapped = onChange.mock.calls[0][0] as typeof ORG.pillars.items
    expect(swapped[0].no).toBe('02')
    expect(swapped[1].no).toBe('01')
  })

  it('新增列表项按上一项结构克隆空壳，不复制内容', () => {
    const onChange = vi.fn()
    render(<JsonForm value={ORG.pillars.items} onChange={onChange} />)

    fireEvent.click(screen.getByText('+ 添加一项'))
    const next = onChange.mock.calls[0][0] as typeof ORG.pillars.items
    expect(next).toHaveLength(3)
    expect(next[2]).toEqual({ no: '', title: '' })
  })

  it('布尔值渲染为开关，图片字段附素材库入口', () => {
    const onChange = vi.fn()
    render(<JsonForm value={{ coming: true, logo: 'https://x/y.png' }} onChange={onChange} />)

    expect(screen.getByText('建设中')).toBeTruthy()
    expect(screen.getByText('标志')).toBeTruthy()
    // 图片字段额外提供「素材库」选择入口
    expect(screen.getByText('素材库')).toBeTruthy()
  })
})
