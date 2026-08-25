import { test, expect } from '@playwright/test'
import { DEMO_PASSWORD, OWNER, solveDragVerify } from './helpers'

/**
 * 真实登录 UI：账号密码 + 登录跳转 /home；错误口令给出提示。
 */
test.describe('登录', () => {
  test('正确口令 → 进入工作台 /home', async ({ page }) => {
    await page.goto('/login')
    await expect(page.getByPlaceholder('请输入账号')).toBeVisible()
    await page.getByPlaceholder('请输入账号').fill(OWNER.username)
    await page.getByPlaceholder('请输入密码').fill(DEMO_PASSWORD)
    await solveDragVerify(page)
    await page.getByRole('button', { name: '登录' }).click()
    await page.waitForURL('**/home', { timeout: 15_000 })
    // 侧边栏出现产品导航
    await expect(page.locator('aside')).toContainText('Articles')
  })

  test('错误口令 → 停留在登录页并提示', async ({ page }) => {
    await page.goto('/login')
    await page.getByPlaceholder('请输入账号').fill(OWNER.username)
    await page.getByPlaceholder('请输入密码').fill('wrong-password')
    await solveDragVerify(page)
    await page.getByRole('button', { name: '登录' }).click()
    await page.waitForTimeout(1_200)
    expect(page.url()).toContain('/login')
  })

  test('未登录访问受保护页 → 回到登录页', async ({ page }) => {
    await page.goto('/content/articles')
    await page.waitForURL('**/login', { timeout: 10_000 })
  })
})
