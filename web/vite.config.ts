import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [
    TanStackRouterVite({ autoCodeSplitting: true, generatedRouteTree: 'src/routeTree.generated.ts' }),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 5188,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8088',
        changeOrigin: true,
      },
      '/uploads': {
        target: 'http://127.0.0.1:8088',
        changeOrigin: true,
      },
      // 模板托管：/t/<slug> 与 /t/active。
      // 公开站点的 previewUrl 在「模板已部署到 server/templates/<slug>/」时是
      // 相对路径，后台的实时预览 iframe 需要经此代理才能加载（此前 5188 不代理
      // /t，导致相对 previewUrl 在开发态 404）。
      '/t': {
        target: 'http://127.0.0.1:8088',
        changeOrigin: true,
      },
    },
  },
})
