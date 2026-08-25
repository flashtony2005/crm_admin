import { test, expect } from '@playwright/test'
import {
  API, OWNER, EDITOR, apiLogin, loggedInCtx, createDraft, editorInvokePublish, getArticleStatus,
} from './helpers'

/**
 * G4 核心链路（纲领 Phase 2 验收）：
 * editor 经 AI 发起发布 → 转审批（不能直接发布）→ owner 在 UI 批准 → 文章真实发布。
 */
test.describe('审批流', () => {
  test('editor 发布转审批 → owner 批准 → published', async ({ browser }) => {
    const oTok = await apiLogin('owner')
    const eTok = await apiLogin('editor')
    const title = `e2e-审批-${Date.now() % 100000}`
    const articleId = await createDraft(oTok, title)

    // editor 触发发布 → 需要审批
    const inv = await editorInvokePublish(eTok, articleId)
    expect(inv.decision).toBe('needs_approval')

    // owner 打开审批页，看到待办并批准
    const ctx = await loggedInCtx(browser, OWNER, oTok)
    const page = await ctx.newPage()
    await page.goto('/ai/approvals')
    await page.waitForLoadState('networkidle')
    await expect(page.getByText('待审批')).not.toHaveCount(0)
    const item = page.locator('li', { hasText: title }).first()
    await expect(item).toBeVisible()
    await item.getByText('批准', { exact: false }).first().click()
    await page.waitForTimeout(1_000)

    // 文章真实发布
    expect(await getArticleStatus(oTok, articleId)).toBe('published')
    await ctx.close()
  })

  test('editor 可看审批列表但无裁决按钮（decide 仅 owner）', async ({ browser }) => {
    const ctx = await loggedInCtx(browser, EDITOR)
    const page = await ctx.newPage()
    await page.goto('/ai/approvals')
    await page.waitForLoadState('networkidle')
    // 有 ai.approvals.view → 列表可达；无 decide → 裁决按钮被 Auth 隐藏/禁用
    expect(page.url()).toContain('/ai/approvals')
    // 裁决按钮对 editor 不可见（tab 文案「待审批」不算）
    await expect(page.getByRole('button', { name: '批准执行', exact: true })).toHaveCount(0)
    await expect(page.getByRole('button', { name: '驳回', exact: true })).toHaveCount(0)
    await ctx.close()
  })

  test('审计留痕：escalated + executed 均可查', async ({ browser }) => {
    const oTok = await apiLogin('owner')
    // 用 decision 过滤分别验证两种裁决留痕（避免大量 escalated 淹没 executed）
    for (const d of ['escalated', 'approved'] as const) {
      const res = await fetch(`${API}/api/ai/audit?decision=${d}`, {
        headers: { Authorization: `Bearer ${oTok}` },
      })
      const body = await res.json()
      expect((body.data as unknown[]).length, `${d} 应有留痕`).toBeGreaterThan(0)
    }
  })
})
