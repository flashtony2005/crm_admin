// 海报截图：HTML → 2x PNG（精确中文排版，避免图像模型渲染中文失真）
// 运行：node shot.cjs  （Playwright 走本机已装的绝对路径）
const path = require('path')
const fs = require('fs')
const { pathToFileURL } = require('url')

const PW = 'C:/Users/Administrator/.workbuddy/binaries/node/versions/22.22.2-6/node_modules/@playwright/cli/node_modules/playwright'
const { chromium } = require(PW)

const here = __dirname
const uploads = path.resolve(here, '../../../server/uploads')

const jobs = [
  { html: 'rpx-hero.html', png: 'rpx-hero.png', width: 1600, height: 900 },
  { html: 'rpx-capabilities.html', png: 'rpx-capabilities.png', width: 1600, height: 1000 },
  { html: 'rpx-architecture.html', png: 'rpx-architecture.png', width: 1600, height: 1000 },
  { html: 'rpx-mark.html', png: 'rpx-mark.png', width: 400, height: 400 },
]

;(async () => {
  fs.mkdirSync(uploads, { recursive: true })
  const browser = await chromium.launch()
  for (const job of jobs) {
    const page = await browser.newPage({
      viewport: { width: job.width, height: job.height },
      deviceScaleFactor: 2,
    })
    const src = pathToFileURL(path.join(here, job.html)).href
    await page.goto(src, { waitUntil: 'load' })
    await page.evaluate(() => document.fonts && document.fonts.ready)
    await page.waitForTimeout(350)
    const out = path.join(here, job.png)
    await page.screenshot({ path: out })
    fs.copyFileSync(out, path.join(uploads, job.png))
    console.log('OK', out, '->', path.join(uploads, job.png))
    await page.close()
  }
  await browser.close()
})().catch((e) => { console.error('FAILED', e); process.exit(1) })
