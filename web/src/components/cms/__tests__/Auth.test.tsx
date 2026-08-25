import { beforeEach, describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { createElement } from 'react'

import { Auth } from '../Auth'
import { usePermissionStore } from '../../../store/permission'
import { P } from '../../../config/permissions'

describe('components/cms/Auth 按钮级权限守卫', () => {
  beforeEach(() => {
    usePermissionStore.setState({ role: 'owner', granted: null })
  })

  it('owner：有权限 → 渲染子元素', () => {
    render(createElement(Auth, { perm: P.contentArticlesCreate }, createElement('button', null, '新建')))
    expect(screen.getByText('新建')).toBeTruthy()
  })

  it('viewer：无权限 + hide 模式 → 不渲染（对应 v-hasPermi 移除 DOM）', () => {
    usePermissionStore.setState({ role: 'viewer' })
    render(createElement(Auth, { perm: P.contentArticlesCreate }, createElement('button', null, '新建')))
    expect(screen.queryByText('新建')).toBeNull()
  })

  it('editor：无权限 + disable 模式 → 渲染但置灰不可点', () => {
    usePermissionStore.setState({ role: 'editor' })
    render(
      createElement(
        Auth,
        { perm: P.aiApprovalsDecide, mode: 'disable' },
        createElement('button', null, '批准执行'),
      ),
    )
    const btn = screen.getByText('批准执行')
    const wrapper = btn.closest('span[aria-disabled]')
    expect(wrapper).toBeTruthy()
    expect(wrapper!.className).toContain('pointer-events-none')
  })

  it('通配权限：editor 经 content.articles.* 命中 create', () => {
    usePermissionStore.setState({ role: 'editor' })
    render(createElement(Auth, { perm: 'content.articles.create' }, createElement('button', null, 'OK')))
    expect(screen.getByText('OK')).toBeTruthy()
  })
})
