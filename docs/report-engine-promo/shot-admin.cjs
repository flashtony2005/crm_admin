// admin 后台预览：API 登录 → 注入 localStorage → 截图看板 / 文章列表 / 站点外观
const path = require('path')
const fs = require('fs')

const PW = 'C:/Users/Administrator/.workbuddy/binaries/node/versions/22.22.2-6/node_modules/@playwright/cli/node_modules/playwright'
const { chromium } = require(PW)

const outDir = path.resolve(__dirname, 'preview')
fs.mkdirSync(outDir, { recursive: true })

;(async () => {
  const browser = await chromium.launch()
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 })

  await page.goto('http://127.0.0.1:5188/login', { waitUntil: 'networkidle', timeout: 60000 })

  const body = await page.evaluate(async () => {
    const r = await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username: 'owner', password: 'demo1234' }),
    })
    return await r.json()
  })
  console.log('LOGIN RESPONSE:', JSON.stringify(body).slice(0, 240))

  const d = body.data ?? body
  const token = d.token ?? d.access_token
  const user = d.user ?? d.userInfo
  if (!token) throw new Error('登录响应里没有 token')

  await page.evaluate(({ token, user }) => {
    localStorage.setItem('auth_token', token)
    localStorage.setItem('auth-store', JSON.stringify({ state: { token, user, isAuthenticated: true }, version: 0 }))
    if (user) localStorage.setItem('user:cache', JSON.stringify(user))
  }, { token, user })

  await page.goto('http://127.0.0.1:5188/home', { waitUntil: 'networkidle', timeout: 60000 })
  await page.waitForTimeout(1600)
  console.log('HOME URL:', page.url())
  const homeText = await page.locator('body').innerText().catch(() => '')
  console.log('HOME 文本片段:', homeText.replace(/\s+/g, ' ').slice(0, 160))
  await page.screenshot({ path: path.join(outDir, 'admin-home.png'), fullPage: true })

  await page.goto('http://127.0.0.1:5188/content/articles', { waitUntil: 'networkidle', timeout: 60000 })
  await page.waitForTimeout(1600)
  const artText = await page.locator('body').innerText().catch(() => '')
  console.log('文章页命中:', ['把报表引擎重写成 Rust', '一份 JSON，跑通', '扩展格', '报表定义即代码'].map(w => `${w}:${artText.includes(w)}`).join(' | '))
  await page.screenshot({ path: path.join(outDir, 'admin-articles.png'), fullPage: true })

  await page.goto('http://127.0.0.1:5188/site-appearance', { waitUntil: 'networkidle', timeout: 60000 })
  await page.waitForTimeout(1400)
  await page.screenshot({ path: path.join(outDir, 'admin-appearance.png'), fullPage: true })

  await browser.close()
  console.log('OK: admin-home.png / admin-articles.png / admin-appearance.png')
})().catch((e) => { console.error('FAILED', e); process.exit(1) })
