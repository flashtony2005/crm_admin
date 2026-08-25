import { defineConfig } from 'vitest/config'

// Standalone (does not load the TanStack Router / Tailwind build plugins)
// so test runs stay fast and isolated from the production bundler.
export default defineConfig({
  esbuild: {
    // Match the project's tsconfig (jsx: "react-jsx" / automatic runtime)
    // so test files don't need an explicit `import React`.
    jsx: 'automatic',
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}', 'src/**/__tests__/**/*.{ts,tsx}'],
    clearMocks: true,
    restoreMocks: true,
    // 覆盖率：仅 `npm run test:coverage` 时激活（需先装 @vitest/coverage-v8@3.2.7）。
    // 默认 `npm test`（vitest run）不加载 provider，保持秒级运行。
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'clover'],
      reportsDirectory: './coverage',
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/**/__tests__/**',
        'src/test/**',
        'src/**/*.d.ts',
        'src/main.tsx',
        'src/router.tsx',
        'src/api/agent.ts', // 纯类型模块
        'src/components/icons/**',
      ],
    },
  },
})
