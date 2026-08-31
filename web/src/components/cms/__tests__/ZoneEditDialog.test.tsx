import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { ZoneEditDialog, parseEditRef } from '../ZoneEditDialog'
import { api, request } from '../../../api/client'

vi.mock('../../../api/client', () => ({
  api: vi.fn(),
  request: vi.fn(),
}))

const SECTIONS = [
  {
    id: 'sec-about',
    slug: 'about',
    title: '关于',
    subtitle: '',
    body: {
      kicker: 'KOSX.ai 社群增长操盘手',
      hero: { title: '把连接，变成看得见的机会。', bio: '短简介', ctaLabel: '查看主页' },
    },
  },
  { id: 'sec-org', slug: 'org', title: '组织', subtitle: '', body: {} },
]

beforeEach(() => {
  vi.mocked(request).mockReset()
  vi.mocked(request).mockResolvedValue({ ok: true, data: {} } as never)
  vi.mocked(api).mockReset()
})

describe('parseEditRef', () => {
  it('解析首页区块：slug + 字段路径', () => {
    const t = parseEditRef('section:about:hero.title', '主标题', '把连接')
    expect(t).toMatchObject({
      kind: 'section',
      slug: 'about',
      path: 'hero.title',
      label: '主标题',
      text: '把连接',
    })
  })

  it('解析导航 / 页脚 / 置顶 / 站点文案', () => {
    expect(parseEditRef('nav:footer')).toMatchObject({ kind: 'nav', group: 'footer' })
    expect(parseEditRef('nav:nav')).toMatchObject({ kind: 'nav', group: 'nav' })
    expect(parseEditRef('pins:writing')).toMatchObject({ kind: 'pins', slot: 'writing' })
    expect(parseEditRef('site:brand')).toMatchObject({ kind: 'site', key: 'brand' })
  })

  it('无法识别的 ref 返回 null', () => {
    expect(parseEditRef('')).toBeNull()
    expect(parseEditRef('whatever:x')).toBeNull()
    expect(parseEditRef('section:')).toBeNull()
  })
})

describe('ZoneEditDialog 首页区块', () => {
  it('按路径取出子结构，保存时只改该子树', async () => {
    vi.mocked(api).mockImplementation(async (url: string) => {
      if (url === '/api/sections') return SECTIONS as never
      return [] as never
    })

    const onSaved = vi.fn()
    const onClose = vi.fn()
    render(
      <ZoneEditDialog
        target={parseEditRef('section:about:hero', '主视觉', '把连接')!}
        onClose={onClose}
        onSaved={onSaved}
      />,
    )

    // 只显示 hero 这一段的字段，不应出现 kicker
    await waitFor(() => expect(screen.getByDisplayValue('把连接，变成看得见的机会。')).toBeTruthy())
    expect(screen.queryByDisplayValue('KOSX.ai 社群增长操盘手')).toBeNull()

    // 改标题后保存
    const input = screen.getByDisplayValue('把连接，变成看得见的机会。')
    fireEvent.change(input, { target: { value: '新的主标题' } })
    fireEvent.click(screen.getByText('保存'))

    await waitFor(() => expect(request).toHaveBeenCalled())
    const [url, init] = vi.mocked(request).mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/api/sections/sec-about')
    const body = JSON.parse(String(init.body))
    // hero 被替换，同级 kicker 原样保留
    expect(body.body.hero.title).toBe('新的主标题')
    expect(body.body.hero.bio).toBe('短简介')
    expect(body.body.kicker).toBe('KOSX.ai 社群增长操盘手')
    expect(onSaved).toHaveBeenCalled()
    expect(onClose).toHaveBeenCalled()
  })

  it('区块不存在时给出明确报错，不渲染表单', async () => {
    vi.mocked(api).mockResolvedValue([] as never)
    render(
      <ZoneEditDialog
        target={parseEditRef('section:about:hero', '主视觉', '')!}
        onClose={() => {}}
        onSaved={() => {}}
      />,
    )
    await waitFor(() =>
      expect(screen.getByText(/未找到区块/)).toBeTruthy(),
    )
    expect(screen.queryByText('保存')).toBeNull()
  })
})

describe('ZoneEditDialog 导航链接', () => {
  it('列出该分组的链接并支持删除', async () => {
    vi.mocked(api).mockResolvedValue([
      { id: 'n1', grp: 'footer', label: 'X 主页', href: 'https://x.com', target: '_blank', sort: 1, enabled: true },
      { id: 'n2', grp: 'nav', label: '关于', href: '#about', target: '', sort: 1, enabled: true },
    ] as never)

    const onSaved = vi.fn()
    render(
      <ZoneEditDialog
        target={parseEditRef('nav:footer', '页脚链接', '')!}
        onClose={() => {}}
        onSaved={onSaved}
      />,
    )

    await waitFor(() => expect(screen.getByDisplayValue('X 主页')).toBeTruthy())
    // 只显示 footer 分组，nav 分组的「关于」不应出现
    expect(screen.queryByDisplayValue('#about')).toBeNull()

    fireEvent.click(screen.getByText('删除'))
    await waitFor(() => expect(request).toHaveBeenCalledWith('/api/navlinks/n1', { method: 'DELETE' }))
    expect(onSaved).toHaveBeenCalled()
  })
})
