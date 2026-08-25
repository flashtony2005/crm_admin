import { Browser, BrowserContext, Page, expect } from '@playwright/test'

/**
 * 解开登录页的拖拽滑块验证：按住手柄拖到轨道最右端后松开。
 */
export async function solveDragVerify(page: Page) {
  const handle = page.locator('.cursor-grab').first()
  await expect(handle).toBeVisible()
  const hb = (await handle.boundingBox())!
  const wrapper = handle.locator('xpath=..')
  const wb = (await wrapper.boundingBox())!
  const startX = hb.x + hb.width / 2
  const startY = hb.y + hb.height / 2
  const endX = wb.x + wb.width - hb.width / 2
  await page.mouse.move(startX, startY)
  await page.mouse.down()
  await page.mouse.move((startX + endX) / 2, startY, { steps: 8 })
  await page.mouse.move(endX, startY, { steps: 8 })
  await page.mouse.up()
}

/**
 * AI-Native CMS e2e 公共设施（现行产品版）。
 *
 * 运行前提（与冒烟一致）：
 *   cd demo/server && rm -f cms.db && PORT=8088 ./target/debug/cms-server
 *   cd demo/web && VITE_CMS_MODE=real pnpm dev   # :5188，vite 代理 /api → :8088
 *
 * 账号：owner / editor / viewer，密码 demo1234（服务端种子）。
 * 角色页测试用 token 注入（addInitScript 写 auth_token/auth-store/perm-store），
 * 登录流程本身在 auth.spec.ts 用真实 UI 验证。
 */

export const API = 'http://localhost:8088'
export const DEMO_PASSWORD = 'demo1234'

export interface Account {
  username: string
  nickname: string
  role: 'owner' | 'editor' | 'viewer'
}

export const OWNER: Account = { username: 'owner', nickname: '老板（Owner）', role: 'owner' }
export const EDITOR: Account = { username: 'editor', nickname: '店员（Editor）', role: 'editor' }

/** API 登录，返回 token */
export async function apiLogin(username: string, password = DEMO_PASSWORD): Promise<string> {
  const res = await fetch(`${API}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password }),
  })
  expect(res.status).toBe(200)
  const body = await res.json()
  return body.data.token as string
}

/** 已登录上下文：注入 token 与权限矩阵（owner 用 ['*'] 通配） */
export async function loggedInCtx(
  browser: Browser,
  account: Account,
  token?: string,
): Promise<BrowserContext> {
  const tok = token ?? (await apiLogin(account.username))
  const perms = account.role === 'owner' ? ['*'] : undefined // 非 owner 留空 → 由前端矩阵兜底
  const ctx = await browser.newContext()
  await ctx.addInitScript(
    ([t, r, n, ps]) => {
      localStorage.setItem('auth_token', t)
      localStorage.setItem(
        'auth-store',
        JSON.stringify({ state: { token: t, user: { nickname: n, role: r }, isAuthenticated: true }, version: 0 }),
      )
      localStorage.setItem('perm-store', JSON.stringify({ state: { role: r, granted: ps ?? [] }, version: 0 }))
    },
    [tok, account.role, account.nickname, perms] as const,
  )
  return ctx
}

/** 创建草稿文章（owner 权限） */
export async function createDraft(token: string, title: string): Promise<string> {
  const res = await fetch(`${API}/api/articles`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', Authorization: `Bearer ${token}` },
    body: JSON.stringify({ title, status: 'draft' }),
  })
  expect(res.status).toBe(200)
  return (await res.json()).data.id as string
}

/** editor 经 AI 发起发布（应产生待审批） */
export async function editorInvokePublish(token: string, articleId: string) {
  const res = await fetch(`${API}/api/ai/invoke`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', Authorization: `Bearer ${token}` },
    body: JSON.stringify({ capability: 'content.articles.publish', input: { article_id: articleId } }),
  })
  const body = await res.json()
  // 响应形如 {ok, data:{decision:'needs_approval', approvalId}} 或 {decision:'executed'}
  const decision = body?.data?.decision ?? body?.data
  return { status: res.status, decision }
}

export async function getArticleStatus(token: string, id: string): Promise<string> {
  const res = await fetch(`${API}/api/articles/${id}`, {
    headers: { Authorization: `Bearer ${token}` },
  })
  return (await res.json()).data.status as string
}
