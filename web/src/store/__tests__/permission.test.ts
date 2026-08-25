import { beforeEach, describe, expect, it } from 'vitest'

import { effectivePerms, hasPerm, usePermissionStore } from '../permission'
import type { PermString } from '../../config/permissions'

describe('store/permission 权限状态', () => {
  beforeEach(() => {
    usePermissionStore.setState({ role: 'owner', granted: null })
  })

  it('默认 owner，hasPerm 全量放行', () => {
    expect(usePermissionStore.getState().role).toBe('owner')
    expect(hasPerm(usePermissionStore.getState(), 'anything.at.all')).toBe(true)
  })

  it('切换角色后按矩阵判定', () => {
    usePermissionStore.setState({ role: 'editor' })
    const st = usePermissionStore.getState()
    expect(hasPerm(st, 'content.articles.create')).toBe(true)
    expect(hasPerm(st, 'ai.approvals.decide')).toBe(false)
  })

  it('服务端权限集（granted）优先于本地矩阵 —— 后端接入的切换点', () => {
    usePermissionStore.setState({
      role: 'viewer',
      granted: ['ai.approvals.decide'] as PermString[],
    })
    const st = usePermissionStore.getState()
    expect(hasPerm(st, 'ai.approvals.decide')).toBe(true)
    // granted 存在时 viewer 矩阵不再兜底
    expect(hasPerm(st, 'content.articles.view')).toBe(false)
    expect(effectivePerms(st)).toEqual(['ai.approvals.decide'])
  })

  it('granted 为空数组时回退本地矩阵（避免误配导致全禁）', () => {
    usePermissionStore.setState({ role: 'viewer', granted: [] })
    expect(hasPerm(usePermissionStore.getState(), 'content.articles.view')).toBe(true)
  })
})
