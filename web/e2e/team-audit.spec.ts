import { test, expect } from '@playwright/test'
import { API, OWNER, EDITOR, apiLogin, loggedInCtx } from './helpers'

/**
 * 团队管理 + 审计查询（纲领 Phase 3 交付）。
 */
test.describe('团队与审计', () => {
  test('owner UI 邀请成员 → 新账号可登录 → 停用后 401', async ({ browser }) => {
    const oTok = await apiLogin('owner')
    const ctx = await loggedInCtx(browser, OWNER, oTok)
    const page = await ctx.newPage()
    await page.goto('/team/users')
    await page.waitForLoadState('networkidle')

    // 种子账号可见
    await expect(page.locator('main').getByText(OWNER.nickname)).toBeVisible()
    await expect(page.locator('main').getByText(EDITOR.nickname)).toBeVisible()

    // 邀请（用户名带时间戳保证幂等）
    const uname = `e2e_${Date.now() % 1000000}`
    await page.getByText('+ 邀请成员').first().click()
    await page.fill('#inv-username', uname)
    await page.fill('#inv-nickname', '前台小周')
    await page.selectOption('#inv-role', 'viewer')
    await page.getByText('创建账号').first().click()
    await page.waitForTimeout(900)
    await expect(page.getByText(`@${uname}`)).toBeVisible()

    // 新账号真实登录
    const loginRes = await fetch(`${API}/api/auth/login`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ username: uname, password: 'demo1234' }),
    })
    expect(loginRes.status).toBe(200)

    // 停用 → 登录 401
    const listRes = await fetch(`${API}/api/team/users`, { headers: { Authorization: `Bearer ${oTok}` } })
    const member = ((await listRes.json()).data as { id: string; username: string }[])
      .find((u) => u.username === uname)!
    await fetch(`${API}/api/team/users/${member.id}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json', Authorization: `Bearer ${oTok}` },
      body: JSON.stringify({ status: 0 }),
    })
    const relogin = await fetch(`${API}/api/auth/login`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ username: uname, password: 'demo1234' }),
    })
    expect(relogin.status).toBe(401)
    await ctx.close()
  })

  test('editor 无邀请权：按钮不可见/接口 403', async ({ browser }) => {
    const eTok = await apiLogin('editor')
    const ctx = await loggedInCtx(browser, EDITOR, eTok)
    const page = await ctx.newPage()
    await page.goto('/team/users')
    await page.waitForLoadState('networkidle')
    await expect(page.getByText('+ 邀请成员')).toHaveCount(0)
    const res = await fetch(`${API}/api/team/users`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', Authorization: `Bearer ${eTok}` },
      body: JSON.stringify({ username: 'nope', nickname: 'n', role: 'viewer' }),
    })
    expect(res.status).toBe(403)
    await ctx.close()
  })

  test('AI 任务页审计 tab 渲染流水', async ({ browser }) => {
    const ctx = await loggedInCtx(browser, OWNER)
    const page = await ctx.newPage()
    await page.goto('/ai/tasks')
    await page.waitForLoadState('networkidle')
    await page.getByText('审计日志').first().click()
    await page.waitForTimeout(800)
    // 至少出现一种裁决状态标签
    await expect(page.getByText(/转审批|已执行|已拒绝/).first()).toBeVisible()
    await ctx.close()
  })
})
