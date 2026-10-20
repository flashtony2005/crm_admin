// 站点预览截图：首页 + 一篇文章（用于验收海报/文章卡片/主页文案）
const path = require('path')
const fs = require('fs')

const PW = 'C:/Users/Administrator/.workbuddy/binaries/node/versions/22.22.2-6/node_modules/@playwright/cli/node_modules/playwright'
const { chromium } = require(PW)

const outDir = path.resolve(__dirname, 'preview')
fs.mkdirSync(outDir, { recursive: true })

;(async () => {
  const browser = await chromium.launch()
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 })

  await page.goto('http://127.0.0.1:5199/', { waitUntil: 'networkidle', timeout: 60000 })
  await page.waitForTimeout(1200)

  // Reveal 用 framer-motion 的 whileInView：整页截图不滚动，视口外内容会保持 opacity:0。
  // 先逐屏滚到底触发全部入场，再回到顶部截整页。
  const height = await page.evaluate(() => document.documentElement.scrollHeight)
  for (let y = 0; y < height; y += 700) {
    await page.evaluate((top) => window.scrollTo(0, top), y)
    await page.waitForTimeout(180)
  }
  await page.waitForTimeout(600)
  await page.evaluate(() => window.scrollTo(0, 0))
  await page.waitForTimeout(500)

  const homeH1 = await page.locator('h1').first().innerText().catch(() => '(no h1)')
  console.log('HOME H1:', homeH1.replace(/\n/g, ' / '))
  await page.screenshot({ path: path.join(outDir, 'home.png'), fullPage: true })

  // 滚到文章区再截一张（上面 fullPage 已含全部；这里留一张首屏供快速查看）
  await page.screenshot({ path: path.join(outDir, 'home-fold.png') })

  await page.goto('http://127.0.0.1:5199/post/rpx-why-rust', { waitUntil: 'networkidle', timeout: 60000 })
  await page.waitForTimeout(1200)
  const artH1 = await page.locator('h1').first().innerText().catch(() => '(no h1)')
  console.log('ARTICLE H1:', artH1.replace(/\n/g, ' / '))
  await page.screenshot({ path: path.join(outDir, 'article.png'), fullPage: true })

  await browser.close()
  console.log('OK: preview/home.png, preview/home-fold.png, preview/article.png')
})().catch((e) => { console.error('FAILED', e); process.exit(1) })
