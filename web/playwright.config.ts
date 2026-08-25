import { defineConfig, devices } from '@playwright/test'

/**
 * AI-Native CMS E2E 配置。
 *
 * - 后端 admin :8088 需先启动（vite.config.ts 已配置 /api 代理）；
 * - vite dev 监听 5188，需以 VITE_CMS_MODE=real 启动；
 * - 使用 Playwright 自带 Chromium（本机系统 Chrome --headless=new 会启动挂起）。
 */
export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]],
  use: {
    baseURL: 'http://localhost:5188',
    headless: true,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    locale: 'zh-CN', // 产品界面默认中文，选择器按中文文案编写
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5188',
    reuseExistingServer: true,
    timeout: 120_000,
  },
})
