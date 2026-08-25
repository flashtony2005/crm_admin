import { describe, expect, it } from 'vitest'
import { ROLE_PERMS, permMatches, type PermString } from '../permissions'
import {
  PRODUCT_NAV,
  filterNavByPerm,
  findNavLabel,
  flattenNavLeaves,
  isSection,
} from '../nav'

/** 用权限矩阵构造 has() 谓词（与 Sidebar 运行时行为一致） */
function makeHas(role: keyof typeof ROLE_PERMS) {
  const granted: PermString[] = ROLE_PERMS[role]
  return (perm: string) => permMatches(granted, perm)
}

describe('config/nav 产品导航', () => {
  it('产品 IA 与 PRODUCT_VISION §3 一致（7 个一级入口）', () => {
    const keys = PRODUCT_NAV.map((n) => n.key)
    expect(keys).toEqual(['home', 'content', 'ai', 'business', 'automation', 'team', 'settings'])
  })

  it('flattenNavLeaves 展开所有叶子且不含分组', () => {
    const leaves = flattenNavLeaves()
    expect(leaves.length).toBeGreaterThanOrEqual(14)
    leaves.forEach((l) => {
      expect(l.path).toMatch(/^\//)
      expect(l.label).toBeTruthy()
      expect(isSection(l)).toBe(false)
    })
  })

  it('findNavLabel 命中与未命中', () => {
    expect(findNavLabel('/content/articles')).toBe('文章')
    expect(findNavLabel('/no-such-path')).toBeUndefined()
  })

  it('管理员（owner）：看全部菜单', () => {
    const nav = filterNavByPerm(PRODUCT_NAV, makeHas('owner'))
    const labels = nav.map((n) => n.key)
    expect(labels).toEqual(['home', 'content', 'ai', 'business', 'automation', 'team', 'settings'])
  })

  it('经办者（editor）：只显示与其角色相关的菜单', () => {
    const nav = filterNavByPerm(PRODUCT_NAV, makeHas('editor'))
    const keys = nav.map((n) => n.key)
    // 可见：Home / Content / AI(Assistant+Tasks) / Business / Automation（editor 有 workflows.toggle 可管理工作流）
    expect(keys).toEqual(['home', 'content', 'ai', 'business', 'automation'])
    // 隐藏：Team、Settings、Approvals（Owner 裁决台）
    expect(keys).not.toContain('team')
    expect(keys).not.toContain('settings')
    const ai = nav.find((n) => n.key === 'ai')
    if (ai && isSection(ai)) {
      expect(ai.children.map((c) => c.key)).toEqual(['assistant', 'tasks'])
    } else {
      throw new Error('ai section missing')
    }
  })

  it('观察者（viewer）：只读菜单，无任何管理入口', () => {
    const nav = filterNavByPerm(PRODUCT_NAV, makeHas('viewer'))
    const keys = nav.map((n) => n.key)
    expect(keys).toEqual(['home', 'content', 'ai', 'business'])
  })

  it('空分组自动隐藏：若 editor 无任何 content 查看权，Content 整组消失', () => {
    const nav = filterNavByPerm(
      [
        {
          key: 'g',
          label: 'G',
          icon: 'home',
          children: [
            { key: 'a', label: 'A', path: '/a', icon: 'home', perm: 'x.y.z' },
          ],
        },
        { key: 'solo', label: 'Solo', path: '/solo', icon: 'home' },
      ],
      () => false,
    )
    // g 组全灭；solo 无 perm 保留
    expect(nav.map((n) => n.key)).toEqual(['solo'])
  })
})
