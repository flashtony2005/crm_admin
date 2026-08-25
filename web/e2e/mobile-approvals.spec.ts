import { test, expect, devices } from '@playwright/test'
import { OWNER, EDITOR, apiLogin, createDraft, editorInvokePublish, getArticleStatus } from './helpers'

/**
 * 移动审批台（纲领 Phase 3 验收）：手机端收到待办并一键批准/驳回。
 * /m 独立极简页，自带登录，不套主布局。
 */
// iPhone 视口 + Chromium 引擎（本机仅安装 Chromium，webkit 需另行下载）
test.use({ ...devices['iPhone 13'], browserName: 'chromium' })

test.describe('移动审批 /m', () => {
  test('未登录显示内联登录表单', async ({ page }) => {
    await page.goto('/m')
    await expect(page.getByPlaceholder('用户名')).toBeVisible()
  })

  test('登录 → 一键批准 AI 代发请求 → 文章发布', async ({ page }) => {
    // 前置：editor 发起一次发布审批
    const oTok = await apiLogin('owner')
    const eTok = await apiLogin('editor')
    const articleId = await createDraft(oTok, `e2e-手机审批-${Date.now() % 100000}`)
    const inv = await editorInvokePublish(eTok, articleId)
    expect(inv.decision).toBe('needs_approval')

    // 手机端登录并批准
    await page.goto('/m')
    await page.fill('input[placeholder="用户名"]', 'owner')
    await page.fill('input[placeholder="密码"]', 'demo1234')
    await page.getByRole('button', { name: '登录' }).click()
    await page.waitForTimeout(1_000)
    await page.getByText('✓ 批准').first().click()
    await page.waitForTimeout(1_000)
    await expect(page.getByText(/已批准|没有待审批/)).toBeVisible()

    expect(await getArticleStatus(oTok, articleId)).toBe('published')
  })
})
