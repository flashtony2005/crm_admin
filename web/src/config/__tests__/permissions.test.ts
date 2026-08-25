import { describe, expect, it } from 'vitest'

import { P, ROLE_PERMS, permMatches } from '../permissions'
import type { PermString } from '../permissions'

describe('config/permissions 权限码表与矩阵', () => {
  it('owner 为全量权限', () => {
    expect(ROLE_PERMS.owner).toEqual(['*'])
  })

  it('editor 可增删改内容但没有发布权（纲领 §7：Editor 的 AI 不能直接发布）', () => {
    const editor = ROLE_PERMS.editor
    expect(permMatches(editor, P.contentArticlesCreate)).toBe(true)
    expect(permMatches(editor, P.contentArticlesUpdate)).toBe(true)
    expect(permMatches(editor, P.contentArticlesDelete)).toBe(true)
    expect(permMatches(editor, P.contentArticlesPublish)).toBe(false)
    // 审批裁决权 Owner 专属
    expect(permMatches(editor, P.aiApprovalsDecide)).toBe(false)
    expect(permMatches(editor, P.teamUsersInvite)).toBe(false)
  })

  it('viewer 只读：无任何写权限', () => {
    const viewer = ROLE_PERMS.viewer
    expect(permMatches(viewer, P.contentArticlesView)).toBe(true)
    for (const code of Object.values(P)) {
      if (code.endsWith('.view') || code === P.aiAssistantUse) continue
      if (['ai.tasks.view', 'ai.approvals.view'].includes(code)) continue
      expect(permMatches(viewer, code)).toBe(false)
    }
  })

  it('permMatches 支持通配尾段与全量通配', () => {
    const granted: PermString[] = ['content.articles.*']
    expect(permMatches(granted, 'content.articles.create')).toBe(true)
    expect(permMatches(granted, 'content.articles.delete')).toBe(true)
    expect(permMatches(granted, 'content.products.create')).toBe(false)
    expect(permMatches(['*'], 'anything.at.all')).toBe(true)
    // 精确优先于通配，前缀相似不误命中
    expect(permMatches(['content.articles.*'], 'content.articlesX.create')).toBe(false)
  })

  it('权限码命名规范：两到三段点分', () => {
    for (const code of Object.values(P)) {
      expect(code).toMatch(/^[a-z]+(\.[a-z]+){1,2}$/)
    }
  })
})
